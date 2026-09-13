//! Posting sink: a synchronous collector with one finalize point.
//!
//! Ratified by DECISION D-W1-3. Built verbatim from CONTRACT §6.2.

use crate::id::{Identifier, ItemId, LocationId, LotId, SerialId};
use crate::money::Money;
use crate::quantity::AnyQuantity;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Kind of posting group. Dispatch and conservation rules live in `wicket-ledger`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum GroupKind {
    /// Location-to-location movement of the same item.
    Movement,
    /// Inventory adjustment with a reason.
    Adjustment,
    /// Identity change (consume A, produce B).
    Transformation,
    /// Value-only (no quantity).
    Valuation,
    /// Reversal of an existing group.
    Reversal,
}

/// Physical or accounting boundary a quantity posting may name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Boundary {
    /// Inbound from a supplier.
    Supplier,
    /// Outbound to a customer.
    Customer,
    /// Scrap.
    Scrap,
    /// Adjustment.
    Adjustment,
    /// Rounding residual.
    Rounding,
    /// Consumed into a transformation.
    Consumed,
    /// Produced by a transformation.
    Produced,
}

/// Cost element of a value posting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CostElement {
    /// Material.
    Material,
    /// Labor.
    Labor,
    /// Burden.
    Burden,
    /// Outside processing.
    Outside,
}

/// Value account a money posting may hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ValueAccount {
    /// Inventory.
    Inventory,
    /// Work in process.
    Wip,
    /// Cost of goods sold.
    Cogs,
    /// Scrap expense.
    ScrapExpense,
    /// Adjustment expense.
    AdjustmentExpense,
    /// AP accrual.
    ApAccrual,
    /// Labor absorbed.
    LaborAbsorbed,
    /// Burden absorbed.
    BurdenAbsorbed,
    /// Purchase price variance.
    Ppv,
    /// Manufacturing variance.
    MfgVariance,
    /// Rounding residual (money).
    Rounding,
}

/// Group-local index of a contributed intent; lets a VALUE intent price a QUANTITY intent and a
/// CONSUMPTION intent name the withdrawal it allocates. Stable for the life of the sink. Only a
/// handle returned for `PostingIntent::Quantity` may appear in `values` or `consuming`; any other
/// handle there is `UnknownHandle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PostingHandle(pub u32);

/// The immutable `ledger.posting.posting_id` (D2 §5.2, `bigint`) of a posting that already exists
/// in the database. Never a `PostingHandle`, never an ordinal, never derived by arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PostingId(pub i64);

/// Metadata for `ledger.posting_group` (D2 §4.1). Set once at sink construction, never per intent.
/// `group_id`, `actor_id`, `posted_at`, `created_xid` and `reverses_kind` are deliberately absent:
/// the ledger stamps the first four from the transaction and its context (D3 §2.1) and resolves
/// `reverses_kind` from the target group row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostingGroupHeader {
    /// Module-defined source kind (work order, receipt, …).
    pub source_kind: String,
    /// Optional source record id.
    pub source_id: Option<Identifier>,
    /// Work order this group belongs to, when any.
    pub work_order_id: Option<Identifier>,
    /// Required on `ADJUSTMENT` groups.
    pub reason_code: Option<String>,
    /// Group this one reverses, when `GroupKind::Reversal`.
    pub reverses_group_id: Option<Identifier>,
}

/// Quantity (inventory) posting intent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuantityPosting {
    /// Item moved.
    pub item: ItemId,
    /// Signed quantity. Negative is a withdrawal.
    pub quantity: AnyQuantity,
    /// Location.
    pub location: LocationId,
    /// Optional identity boundary.
    pub boundary: Option<Boundary>,
    /// Lot, when the item is lot-tracked.
    pub lot: Option<LotId>,
    /// Serial, when the item is serial-tracked.
    pub serial: Option<SerialId>,
    /// Provenance only, never summed.
    pub entered: Option<AnyQuantity>,
}

/// Value posting intent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValuePosting {
    /// Account the amount hits.
    pub account: ValueAccount,
    /// Cost element.
    pub cost_element: CostElement,
    /// Cost object (typically a work order).
    pub cost_object: Option<Identifier>,
    /// Signed money.
    pub amount: Money,
    /// Quantity handle this value prices, when any.
    pub values: Option<PostingHandle>,
}

/// Allocation of a withdrawal against an existing ledger posting (a layer), and the edge the
/// genealogy graph is traversed over (D2 §5.3). The consumed posting is named by its immutable
/// `posting_id`, never by a handle. Signed quantity: a REVERSAL restores a layer with negative edges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsumptionPosting {
    /// Handle of the quantity intent that is consuming.
    pub consuming: PostingHandle,
    /// Existing ledger posting being consumed.
    pub consumed_posting_id: PostingId,
    /// Signed quantity of the edge.
    pub quantity: AnyQuantity,
    /// Signed money of the edge.
    pub amount: Money,
}

/// One contribution to a posting group.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PostingIntent {
    /// Inventory quantity.
    Quantity(QuantityPosting),
    /// Money.
    Value(ValuePosting),
    /// Consumption edge.
    Consumption(ConsumptionPosting),
}

/// Why a sink refused a contribution or finalize.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PostingError {
    /// No sink is bound (`NoPostings`, or a transition that must not post).
    #[error("no posting sink is bound")]
    NoSink,
    /// Intent shape is invalid for this group kind.
    #[error("invalid posting shape: {0}")]
    Shape(String),
    /// Handle was never issued for a quantity intent (or is out of range).
    #[error("unknown posting handle {0:?}")]
    UnknownHandle(PostingHandle),
    /// `contribute` after [`PostingSink::finalize`].
    #[error("contribute after finalize")]
    AfterFinalize,
    /// Finalize of a group with zero posting rows.
    #[error("empty posting group")]
    EmptyGroup,
    /// No open layer can cover this withdrawal.
    #[error("allocation required for handle {0:?}")]
    AllocationRequired(PostingHandle),
    /// Explicit `Consumption` edges do not reproduce the withdrawal's quantity or its money (D2 P3).
    #[error("allocation mismatch for handle {0:?}")]
    AllocationMismatch(PostingHandle),
    /// The consumed posting is a different item, an incompatible lot or serial, or an exhausted layer.
    #[error("ineligible layer: consuming {consuming:?} consumed {consumed:?}")]
    IneligibleLayer {
        /// Handle of the consuming quantity intent.
        consuming: PostingHandle,
        /// Layer that was not eligible.
        consumed: PostingId,
    },
    /// A TRANSFORMATION's produced quantity has no incoming consumption edge (D2 §5.3 genealogy).
    #[error("lineage required for handle {0:?}")]
    LineageRequired(PostingHandle),
    /// The sink took contributions and was dropped without `finalize`; the ledger refuses the commit.
    #[error("posting sink dropped without finalize")]
    Unfinalized,
    /// Operation is not implemented.
    #[error("unimplemented")]
    Unimplemented,
}

/// Synchronous collector. Hooks hold `&mut dyn PostingSink` and cannot call [`PostingSink::finalize`].
pub trait PostingSink {
    /// Group kind, fixed at construction.
    fn kind(&self) -> GroupKind;
    /// Header, fixed at construction.
    fn header(&self) -> &PostingGroupHeader;
    /// Contribute one intent to this group. Order within the group is contribution order.
    fn contribute(
        &mut self,
        intent: PostingIntent,
    ) -> core::result::Result<PostingHandle, PostingError>;
    /// Exactly once per database transaction, by the transition executor, after every hook has
    /// run. A later `contribute` on the same sink is `AfterFinalize`. A group with zero posting
    /// rows is `EmptyGroup`: an empty group never reaches the database (D2 empty-group hole).
    /// By-value and object-safe: hooks hold `&mut dyn PostingSink` and cannot call this.
    fn finalize(self: Box<Self>) -> core::result::Result<(), PostingError>;
}

/// Core's own implementation for tests and for transitions that must not post: refuses everything.
/// `contribute` and `finalize` both return [`PostingError::NoSink`].
#[derive(Debug, Clone, Copy)]
pub struct NoPostings;

fn empty_header() -> &'static PostingGroupHeader {
    static HEADER: OnceLock<PostingGroupHeader> = OnceLock::new();
    HEADER.get_or_init(|| PostingGroupHeader {
        source_kind: String::new(),
        source_id: None,
        work_order_id: None,
        reason_code: None,
        reverses_group_id: None,
    })
}

impl PostingSink for NoPostings {
    fn kind(&self) -> GroupKind {
        GroupKind::Adjustment
    }

    fn header(&self) -> &PostingGroupHeader {
        empty_header()
    }

    fn contribute(
        &mut self,
        _intent: PostingIntent,
    ) -> core::result::Result<PostingHandle, PostingError> {
        Err(PostingError::NoSink)
    }

    fn finalize(self: Box<Self>) -> core::result::Result<(), PostingError> {
        Err(PostingError::NoSink)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{ItemId, LocationId};
    use crate::money::Money;
    use crate::quantity::AnyQuantity;
    use crate::units::{CurrencyId, DimensionKind, UnitId};
    use rust_decimal::Decimal;

    fn sample_qty() -> QuantityPosting {
        QuantityPosting {
            item: ItemId::from_uuid(uuid::Uuid::nil()),
            quantity: AnyQuantity {
                amount: Decimal::from(1),
                unit: UnitId(1),
                dimension: DimensionKind::Count,
            },
            location: LocationId::from_uuid(uuid::Uuid::nil()),
            boundary: None,
            lot: None,
            serial: None,
            entered: None,
        }
    }

    fn sample_value() -> ValuePosting {
        ValuePosting {
            account: ValueAccount::Inventory,
            cost_element: CostElement::Material,
            cost_object: None,
            amount: Money::zero(CurrencyId(840)),
            values: Some(PostingHandle(0)),
        }
    }

    fn sample_cons() -> ConsumptionPosting {
        ConsumptionPosting {
            consuming: PostingHandle(0),
            consumed_posting_id: PostingId(1),
            quantity: AnyQuantity {
                amount: Decimal::from(1),
                unit: UnitId(1),
                dimension: DimensionKind::Count,
            },
            amount: Money::zero(CurrencyId(840)),
        }
    }

    #[test]
    fn no_postings_refuses_every_intent() {
        let mut sink = NoPostings;
        assert_eq!(
            sink.contribute(PostingIntent::Quantity(sample_qty()))
                .unwrap_err(),
            PostingError::NoSink
        );
        assert_eq!(
            sink.contribute(PostingIntent::Value(sample_value()))
                .unwrap_err(),
            PostingError::NoSink
        );
        assert_eq!(
            sink.contribute(PostingIntent::Consumption(sample_cons()))
                .unwrap_err(),
            PostingError::NoSink
        );
    }

    #[test]
    fn no_postings_refuses_finalize() {
        let sink: Box<dyn PostingSink> = Box::new(NoPostings);
        assert_eq!(sink.finalize().unwrap_err(), PostingError::NoSink);
    }

    struct CollectingSink {
        header: PostingGroupHeader,
        next: u32,
        quantity_handles: Vec<u32>,
        finalized: bool,
        count: u32,
    }

    impl CollectingSink {
        fn new() -> Self {
            Self {
                header: empty_header().clone(),
                next: 0,
                quantity_handles: Vec::new(),
                finalized: false,
                count: 0,
            }
        }
    }

    impl PostingSink for CollectingSink {
        fn kind(&self) -> GroupKind {
            GroupKind::Movement
        }
        fn header(&self) -> &PostingGroupHeader {
            &self.header
        }
        fn contribute(
            &mut self,
            intent: PostingIntent,
        ) -> core::result::Result<PostingHandle, PostingError> {
            if self.finalized {
                return Err(PostingError::AfterFinalize);
            }
            match &intent {
                PostingIntent::Value(v) => {
                    if let Some(h) = v.values
                        && !self.quantity_handles.contains(&h.0)
                    {
                        return Err(PostingError::UnknownHandle(h));
                    }
                }
                PostingIntent::Consumption(c) => {
                    if !self.quantity_handles.contains(&c.consuming.0) {
                        return Err(PostingError::UnknownHandle(c.consuming));
                    }
                }
                PostingIntent::Quantity(_) => {}
            }
            let handle = PostingHandle(self.next);
            if let PostingIntent::Quantity(_) = intent {
                self.quantity_handles.push(self.next);
            }
            self.next += 1;
            self.count += 1;
            Ok(handle)
        }
        fn finalize(self: Box<Self>) -> core::result::Result<(), PostingError> {
            if self.count == 0 {
                return Err(PostingError::EmptyGroup);
            }
            Ok(())
        }
    }

    #[test]
    fn handle_out_of_range() {
        let mut sink = CollectingSink::new();
        let err = sink
            .contribute(PostingIntent::Value(sample_value()))
            .unwrap_err();
        assert_eq!(err, PostingError::UnknownHandle(PostingHandle(0)));
        let _h = sink
            .contribute(PostingIntent::Quantity(sample_qty()))
            .unwrap();
        let mut bad = sample_cons();
        bad.consuming = PostingHandle(99);
        assert_eq!(
            sink.contribute(PostingIntent::Consumption(bad))
                .unwrap_err(),
            PostingError::UnknownHandle(PostingHandle(99))
        );
    }

    #[test]
    fn empty_group_on_finalize() {
        let sink: Box<dyn PostingSink> = Box::new(CollectingSink::new());
        assert_eq!(sink.finalize().unwrap_err(), PostingError::EmptyGroup);
    }
}
