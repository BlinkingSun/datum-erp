//! [`GroupBuilder`]: the [`PostingSink`] implementation (CONTRACT §6.2).

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use datum_core::{
    GroupKind, PostingError, PostingGroupHeader, PostingHandle, PostingIntent, PostingSink,
    QuantityPosting, ValueAccount, ValuePosting,
};

use crate::poison;

/// Collected intents for one posting group. Cheap to clone: the state is shared.
#[derive(Clone)]
pub struct GroupBuilder {
    kind: GroupKind,
    header: PostingGroupHeader,
    inner: Arc<Mutex<Inner>>,
    /// Bound by [`crate::bind_tx`] / [`crate::post`]; Drop poisons this xact when unfinalized.
    pub(crate) txid: Option<String>,
}

pub(crate) struct Inner {
    pub kind: GroupKind,
    pub header: PostingGroupHeader,
    pub next: u32,
    pub quantity_handles: HashSet<u32>,
    pub quantities: Vec<(PostingHandle, QuantityPosting)>,
    pub values: Vec<(PostingHandle, ValuePosting)>,
    pub consumptions: Vec<datum_core::ConsumptionPosting>,
    pub contributed: bool,
    pub finalized: bool,
}

impl GroupBuilder {
    /// Construct a sink for `kind` with header metadata. The actor is not here:
    /// it is read from the transaction context at [`crate::post`] (rule 2).
    pub fn new(kind: GroupKind, header: PostingGroupHeader) -> Self {
        Self {
            kind,
            header: header.clone(),
            inner: Arc::new(Mutex::new(Inner {
                kind,
                header,
                next: 0,
                quantity_handles: HashSet::new(),
                quantities: Vec::new(),
                values: Vec::new(),
                consumptions: Vec::new(),
                contributed: false,
                finalized: false,
            })),
            txid: None,
        }
    }

    /// Bind to `pg_current_xact_id()` so trait-path Drop poisons the transaction.
    pub fn set_txid(&mut self, txid: String) {
        self.txid = Some(txid);
    }

    /// True when this sink received contributions and was not finalized.
    pub fn unfinalized(&self) -> bool {
        self.inner
            .lock()
            .ok()
            .is_some_and(|i| i.contributed && !i.finalized)
    }

    pub(crate) fn lock_inner(
        &self,
    ) -> core::result::Result<std::sync::MutexGuard<'_, Inner>, PostingError> {
        self.inner
            .lock()
            .map_err(|_| PostingError::Shape("group builder lock poisoned".into()))
    }
}

/// Test hook: pending quantity rows without setting `contributed` (Drop-path proof; `test-utils` only).
#[cfg(feature = "test-utils")]
pub fn test_inject_quantity_without_contributed_mark(
    builder: &mut GroupBuilder,
    q: QuantityPosting,
) -> PostingHandle {
    let mut inner = builder.lock_inner().expect("lock");
    let handle = PostingHandle(inner.next);
    inner.next += 1;
    inner.quantity_handles.insert(handle.0);
    inner.quantities.push((handle, q));
    handle
}

impl Drop for GroupBuilder {
    fn drop(&mut self) {
        if let Ok(inner) = self.inner.lock()
            && inner.contributed
            && !inner.finalized
            && let Some(ref txid) = self.txid
        {
            poison::mark(txid);
        }
    }
}

impl PostingSink for GroupBuilder {
    fn kind(&self) -> GroupKind {
        self.kind
    }

    fn header(&self) -> &PostingGroupHeader {
        &self.header
    }

    fn contribute(
        &mut self,
        intent: PostingIntent,
    ) -> core::result::Result<PostingHandle, PostingError> {
        let mut inner = self.lock_inner()?;
        if inner.finalized {
            return Err(PostingError::AfterFinalize);
        }
        match &intent {
            PostingIntent::Value(v) => {
                if let Some(h) = v.values
                    && !inner.quantity_handles.contains(&h.0)
                {
                    return Err(PostingError::UnknownHandle(h));
                }
                if v.account == ValueAccount::Wip {
                    match (v.cost_object, inner.header.work_order_id) {
                        (Some(obj), Some(wo)) if obj == wo => {}
                        (Some(_), Some(_)) => {
                            return Err(PostingError::Shape(
                                "WIP cost_object must equal header.work_order_id".into(),
                            ));
                        }
                        _ => {
                            return Err(PostingError::Shape(
                                "WIP value row requires cost_object = header.work_order_id".into(),
                            ));
                        }
                    }
                }
            }
            PostingIntent::Consumption(c) => {
                if !inner.quantity_handles.contains(&c.consuming.0) {
                    return Err(PostingError::UnknownHandle(c.consuming));
                }
            }
            PostingIntent::Quantity(q) => {
                if let Some(b) = q.boundary
                    && !crate::enums::boundary_permitted(inner.kind, b)
                {
                    return Err(PostingError::Shape(format!(
                        "boundary {b:?} is not permitted for {:?}",
                        inner.kind
                    )));
                }
                if inner.kind == GroupKind::Valuation {
                    return Err(PostingError::Shape(
                        "VALUATION groups may not carry QUANTITY rows".into(),
                    ));
                }
            }
            _ => {
                return Err(PostingError::Shape("unknown posting intent".into()));
            }
        }
        let handle = PostingHandle(inner.next);
        inner.next += 1;
        inner.contributed = true;
        if let Some(ref txid) = self.txid {
            poison::mark(txid);
        }
        match intent {
            PostingIntent::Quantity(q) => {
                inner.quantity_handles.insert(handle.0);
                inner.quantities.push((handle, q));
            }
            PostingIntent::Value(v) => inner.values.push((handle, v)),
            PostingIntent::Consumption(c) => inner.consumptions.push(c),
            _ => return Err(PostingError::Shape("unknown posting intent".into())),
        }
        Ok(handle)
    }

    fn finalize(self: Box<Self>) -> core::result::Result<(), PostingError> {
        let mut inner = self.lock_inner()?;
        if inner.finalized {
            return Err(PostingError::AfterFinalize);
        }
        if inner.quantities.is_empty() && inner.values.is_empty() {
            return Err(PostingError::EmptyGroup);
        }
        if inner.kind == GroupKind::Adjustment && inner.header.reason_code.is_none() {
            return Err(PostingError::Shape(
                "ADJUSTMENT requires reason_code".into(),
            ));
        }
        if inner.kind == GroupKind::Transformation && inner.header.work_order_id.is_none() {
            return Err(PostingError::Shape(
                "TRANSFORMATION requires work_order_id".into(),
            ));
        }
        if inner.kind == GroupKind::Reversal && inner.header.reverses_group_id.is_none() {
            return Err(PostingError::Shape(
                "REVERSAL requires reverses_group_id".into(),
            ));
        }
        inner.finalized = true;
        if let Some(ref txid) = self.txid {
            poison::clear(txid);
        }
        Ok(())
    }
}

/// Reason code this crate uses for the rounding residual (D2 §7 R4).
pub const UOM_CONVERSION_RESIDUAL: &str = "UOM_CONVERSION_RESIDUAL";

/// True when `q` is a withdrawal P3 cares about: real location, negative quantity.
pub(crate) fn is_withdrawal(q: &QuantityPosting) -> bool {
    q.boundary.is_none() && q.quantity.amount.is_sign_negative()
}

/// True when a TRANSFORMATION produced lot needs lineage (rule 6).
pub(crate) fn needs_lineage(kind: GroupKind, q: &QuantityPosting) -> bool {
    kind == GroupKind::Transformation
        && q.boundary.is_none()
        && q.quantity.amount.is_sign_positive()
}
