//! One [`GroupBuilder`] per movement: transition and ledger post share the same sink (CONTRACT §6.2).

use rust_decimal::Decimal;
use wicket_core::{
    GroupKind, PostingError, PostingGroupHeader, PostingHandle, PostingIntent, PostingSink,
};
use wicket_db::Tx;
use wicket_ledger::{GroupBuilder, Layer};
use wicket_module::Kernel;
use wicket_statemachine::DocRef;

use crate::domain::{DOC_TYPE, DocumentKind};
use crate::error::Result;

/// Wrapper whose `finalize` is deferred to [`wicket_ledger::post`] (same pattern as kernel `BoundSink`).
struct DeferredFinalize(GroupBuilder);

impl PostingSink for DeferredFinalize {
    fn kind(&self) -> GroupKind {
        PostingSink::kind(&self.0)
    }

    fn header(&self) -> &PostingGroupHeader {
        PostingSink::header(&self.0)
    }

    fn contribute(
        &mut self,
        intent: PostingIntent,
    ) -> core::result::Result<PostingHandle, PostingError> {
        self.0.contribute(intent)
    }

    fn finalize(self: Box<Self>) -> core::result::Result<(), PostingError> {
        Ok(())
    }
}

/// Run the document state-machine edge, then insert the contributed group when non-empty.
pub async fn post_via_transition(
    kernel: &Kernel,
    tx: &mut Tx<'_>,
    ctx: &wicket_db::WriteContext,
    doc_id: wicket_core::Identifier,
    kind: DocumentKind,
    mut builder: GroupBuilder,
) -> Result<Option<wicket_core::Identifier>> {
    kernel.bind_sink(tx, &mut builder).await?;
    let watch = builder.clone();
    let doc = DocRef {
        doc_type: DOC_TYPE.into(),
        doc_id,
    };
    kernel
        .engine
        .transition(
            tx,
            Box::new(DeferredFinalize(builder)),
            &doc,
            kind.post_edge(),
            None,
            kernel.signature_gate(),
            ctx,
        )
        .await?;
    if !watch.unfinalized() {
        return Ok(None);
    }
    Ok(Some(wicket_ledger::post(tx, watch).await?))
}

pub(crate) struct CoverEdge {
    pub posting_id: wicket_core::PostingId,
    pub qty: Decimal,
    pub amount: wicket_core::Money,
}

/// Layer money covering a positive `qty` withdrawal (FIFO walk).
pub(crate) fn cover_layers(
    layers: &[Layer],
    qty: Decimal,
    lot: Option<wicket_core::LotId>,
    serial: Option<wicket_core::SerialId>,
) -> Result<(wicket_core::Money, Vec<CoverEdge>)> {
    let mut left = qty;
    let mut edges = Vec::new();
    let mut total = Decimal::ZERO;
    let mut currency = None;
    for layer in layers {
        if left <= Decimal::ZERO {
            break;
        }
        if lot.is_some() && layer.lot != lot {
            continue;
        }
        if serial.is_some() && layer.serial != serial {
            continue;
        }
        let take = layer.remaining_qty.min(left);
        if take <= Decimal::ZERO {
            continue;
        }
        let amt = if take == layer.remaining_qty {
            layer.remaining_amt
        } else {
            (layer.remaining_amt * take / layer.remaining_qty).round_dp(4)
        };
        total += amt;
        currency = Some(layer.currency);
        let money =
            wicket_core::Money::new(amt, layer.currency).map_err(wicket_core::Error::from)?;
        edges.push(CoverEdge {
            posting_id: layer.posting_id,
            qty: take,
            amount: money,
        });
        left -= take;
    }
    if left > Decimal::ZERO {
        return Err(crate::error::Error::from(
            wicket_core::PostingError::AllocationRequired(wicket_core::PostingHandle(0)),
        ));
    }
    let currency = currency.unwrap_or(wicket_core::CurrencyId(840));
    let money = wicket_core::Money::new(total, currency).map_err(wicket_core::Error::from)?;
    Ok((money, edges))
}
