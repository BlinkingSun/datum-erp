//! Published posting API: plan a WIP issue asynchronously, contribute on a bound
//! [`PostingSink`] synchronously (CONTRACT §6.2). Used by `production_min` start hooks.
//!
//! Plans stashed for the start hook are keyed by PostgreSQL transaction id
//! (`pg_current_xact_id`) plus work order. The hook ABI has no `Tx`, so lookup
//! uses the current tokio task id recorded at stash (one request = one task =
//! one `Tx`). Two concurrent requests cannot see each other's plans.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use datum_core::{
    AnyQuantity, CostElement, Identifier, ItemId, LocationId, LotId, Money, PostingError,
    PostingIntent, PostingSink, QuantityPosting, ValueAccount, ValuePosting,
};
use datum_db::Tx;
use datum_ledger::load_open_layers;
use datum_mod_lots::{LotStatus, load_lot};
use datum_module::Kernel;
use rust_decimal::Decimal;
use tokio::task::Id;

use crate::domain::{Document, DocumentKind, IssueRequest};
use crate::error::{Error, Result};
use crate::posting_path::cover_layers;
use crate::store::{
    PreparedLine, begin_idempotent, hash_issue, insert_document, load_document, post_uom_residuals,
    prepare_line_with_residual, stamp_posted,
};

/// Async plan for issuing to WIP. Ledger rows are contributed later on the caller's sink.
#[derive(Debug, Clone)]
pub struct WipIssuePlan {
    /// Inventory document id (rows inserted before the transition posts).
    pub document_id: Identifier,
    /// Owning work order.
    pub work_order: Identifier,
    /// Source location.
    pub from_location: LocationId,
    /// Prepared lines with precomputed consumption/value data. Empty when replaying idempotency.
    pub lines: Vec<PlannedIssueLine>,
    /// UOM conversion residuals (separate adjustment groups after the parent posts).
    pub residuals: Vec<(ItemId, Option<LotId>, AnyQuantity)>,
    /// Set when idempotency returned an already-posted document (no contribution).
    pub replay_document: Option<Document>,
}

/// One line ready for synchronous contribution.
#[derive(Debug, Clone)]
pub struct PlannedIssueLine {
    /// Canonical line shape.
    pub(crate) prepared: PreparedLine,
    /// Inventory/WIP value movement.
    pub(crate) value_money: Money,
    /// Explicit consumption edges (named lots / serials).
    pub(crate) consumptions: Vec<PlannedConsumption>,
}

/// Explicit consumption edge resolved at plan time.
#[derive(Debug, Clone)]
pub struct PlannedConsumption {
    pub(crate) posting_id: datum_core::PostingId,
    pub(crate) quantity: AnyQuantity,
    pub(crate) amount: Money,
}

/// Build a plan (document rows + layer allocation). Does not post.
pub async fn plan_wip_issue(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    req: &IssueRequest,
) -> Result<WipIssuePlan> {
    let body_hash = hash_issue(req);
    let doc_id = Identifier::generate();
    if let Some(existing) = begin_idempotent(tx, req.idempotency_key, &body_hash, doc_id).await? {
        let doc = load_document(tx, existing).await?;
        return Ok(WipIssuePlan {
            document_id: existing,
            work_order: req.work_order,
            from_location: req.from_location,
            lines: Vec::new(),
            residuals: Vec::new(),
            replay_document: Some(doc),
        });
    }
    let wip = datum_mod_locations::ensure_wip(tx, req.work_order).await?;
    let mut planned = Vec::new();
    let mut residuals = Vec::new();
    for mut line in req.lines.clone() {
        if let Some(lot) = line.lot {
            let rec = load_lot(tx, lot).await?;
            if rec.status != LotStatus::Available {
                return Err(Error::LotNotIssuable);
            }
        }
        line.from_location = Some(req.from_location);
        line.to_location = Some(wip);
        let (p, res) = prepare_line_with_residual(tx, kernel, line).await?;
        if !res.amount.is_zero() {
            residuals.push((p.item, p.lot, res));
        }
        planned.push(plan_issue_line(tx, &p, req.from_location).await?);
    }
    let prepared: Vec<PreparedLine> = planned.iter().map(|l| l.prepared.clone()).collect();
    insert_document(
        tx,
        kernel,
        doc_id,
        DocumentKind::Issue,
        req.reference.clone(),
        &prepared,
    )
    .await?;
    let plan = WipIssuePlan {
        document_id: doc_id,
        work_order: req.work_order,
        from_location: req.from_location,
        lines: planned,
        residuals,
        replay_document: None,
    };
    stash_wip_issue_plan(tx, plan.clone()).await?;
    Ok(plan)
}

/// Contribute every planned line to the bound sink (same transaction as the WO transition).
pub fn contribute_wip_issue(
    sink: &mut dyn PostingSink,
    plan: &WipIssuePlan,
) -> core::result::Result<(), PostingError> {
    for line in &plan.lines {
        contribute_planned_issue_line(sink, plan.work_order, line)?;
    }
    Ok(())
}

fn contribute_planned_issue_line(
    sink: &mut dyn PostingSink,
    work_order: Identifier,
    line: &PlannedIssueLine,
) -> core::result::Result<(), PostingError> {
    let p = &line.prepared;
    let from = p
        .from_location
        .ok_or_else(|| PostingError::Shape("from_location".into()))?;
    let to = p
        .to_location
        .ok_or_else(|| PostingError::Shape("to_location".into()))?;
    let out = sink.contribute(PostingIntent::Quantity(quantity_posting(
        p.item,
        signed_qty(p.canonical, -1),
        from,
        None,
        p.lot,
        p.serial,
        Some(p.entered),
    )))?;
    let into = sink.contribute(PostingIntent::Quantity(quantity_posting(
        p.item,
        p.canonical,
        to,
        None,
        p.lot,
        p.serial,
        Some(p.entered),
    )))?;
    let money = line.value_money;
    if !money.amount().is_zero() {
        sink.contribute(PostingIntent::Value(ValuePosting {
            account: ValueAccount::Inventory,
            cost_element: CostElement::Material,
            cost_object: None,
            amount: money.negate(),
            values: Some(out),
        }))?;
        sink.contribute(PostingIntent::Value(ValuePosting {
            account: ValueAccount::Wip,
            cost_element: CostElement::Material,
            cost_object: Some(work_order),
            amount: money,
            values: Some(into),
        }))?;
    }
    for edge in &line.consumptions {
        sink.contribute(PostingIntent::Consumption(datum_core::ConsumptionPosting {
            consuming: out,
            consumed_posting_id: edge.posting_id,
            quantity: edge.quantity,
            amount: edge.amount,
        }))?;
    }
    Ok(())
}

fn quantity_posting(
    item: ItemId,
    quantity: AnyQuantity,
    location: LocationId,
    boundary: Option<datum_core::Boundary>,
    lot: Option<LotId>,
    serial: Option<datum_core::SerialId>,
    entered: Option<AnyQuantity>,
) -> QuantityPosting {
    QuantityPosting {
        item,
        quantity,
        location,
        boundary,
        lot,
        serial,
        entered,
    }
}

fn signed_qty(base: AnyQuantity, sign: i32) -> AnyQuantity {
    AnyQuantity {
        amount: base.amount * Decimal::from(sign),
        unit: base.unit,
        dimension: base.dimension,
    }
}

/// Stamp the inventory document, post UOM residuals, and emit issue events.
pub async fn finish_wip_issue(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    ctx: &datum_db::WriteContext,
    plan: &WipIssuePlan,
    movement_group: Identifier,
) -> Result<Document> {
    stamp_posted(tx, plan.document_id, Some(movement_group)).await?;
    post_uom_residuals(
        tx,
        kernel,
        ctx,
        plan.document_id,
        plan.from_location,
        movement_group,
        &plan.residuals,
    )
    .await?;
    for line in &plan.lines {
        kernel
            .publish_event(
                tx,
                crate::events::issued(
                    line.prepared.item,
                    line.prepared.canonical.amount,
                    line.prepared.lot,
                    Some(plan.work_order),
                    plan.document_id,
                )?,
            )
            .await?;
    }
    let doc = load_document(tx, plan.document_id).await?;
    clear_wip_issue_plan(plan.work_order);
    Ok(doc)
}

async fn plan_issue_line(
    tx: &mut Tx<'_>,
    p: &PreparedLine,
    from: LocationId,
) -> Result<PlannedIssueLine> {
    let explicit = p.lot.is_some() || p.serial.is_some();
    let (value_money, edges) = if let Some(amount) = p.amount {
        (amount, Vec::new())
    } else {
        let layers = load_open_layers(tx, p.item, from).await?;
        cover_layers(&layers, p.canonical.amount.abs(), p.lot, p.serial)?
    };
    let mut consumptions = Vec::new();
    if explicit {
        if edges.is_empty() {
            let layers = load_open_layers(tx, p.item, from).await?;
            let layer = layers
                .iter()
                .find(|l| {
                    p.lot.is_none_or(|lot| l.lot == Some(lot))
                        && p.serial.is_none_or(|s| l.serial == Some(s))
                })
                .ok_or(Error::NoEligibleLayer)?;
            let qty_abs = p.canonical.amount.abs();
            let amt = if let Some(given) = p.amount {
                Money::new(given.amount().abs(), given.currency())
                    .map_err(datum_core::Error::from)?
            } else if layer.remaining_qty.is_zero() {
                Money::zero(layer.currency)
            } else {
                let share = layer.remaining_amt * qty_abs / layer.remaining_qty;
                Money::new(share, layer.currency).map_err(datum_core::Error::from)?
            };
            consumptions.push(PlannedConsumption {
                posting_id: layer.posting_id,
                quantity: AnyQuantity {
                    amount: qty_abs,
                    unit: p.canonical.unit,
                    dimension: p.canonical.dimension,
                },
                amount: amt,
            });
        } else {
            for edge in edges {
                consumptions.push(PlannedConsumption {
                    posting_id: edge.posting_id,
                    quantity: AnyQuantity {
                        amount: edge.qty,
                        unit: p.canonical.unit,
                        dimension: p.canonical.dimension,
                    },
                    amount: edge.amount,
                });
            }
        }
    }
    Ok(PlannedIssueLine {
        prepared: p.clone(),
        value_money,
        consumptions,
    })
}

struct PlanStore {
    /// Caller (tokio task, or thread when the runtime has not spawned one)
    /// → PostgreSQL xid of that caller's open write transaction.
    caller_txid: HashMap<Caller, String>,
    /// `(txid, work_order)` so two concurrent requests never share a plan.
    plans: HashMap<(String, Identifier), WipIssuePlan>,
}

/// Hook ABI has no `Tx`. Spawned requests have a tokio task id; `#[tokio::test]`
/// block_on futures do not, so those fall back to the OS thread (stable on the
/// current-thread runtime the module tests use).
#[derive(Clone, Eq, PartialEq, Hash)]
enum Caller {
    Task(Id),
    Thread(std::thread::ThreadId),
}

fn current_caller() -> Caller {
    match tokio::task::try_id() {
        Some(id) => Caller::Task(id),
        None => Caller::Thread(std::thread::current().id()),
    }
}

fn plan_store() -> &'static Mutex<PlanStore> {
    static STORE: OnceLock<Mutex<PlanStore>> = OnceLock::new();
    STORE.get_or_init(|| {
        Mutex::new(PlanStore {
            caller_txid: HashMap::new(),
            plans: HashMap::new(),
        })
    })
}

fn lock_plans() -> std::sync::MutexGuard<'static, PlanStore> {
    plan_store()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn stash_with_txid(txid: String, plan: WipIssuePlan) -> Result<()> {
    let caller = current_caller();
    let work_order = plan.work_order;
    let mut store = lock_plans();
    store.caller_txid.insert(caller, txid.clone());
    store.plans.insert((txid, work_order), plan);
    Ok(())
}

/// Bind `plan` to this `Tx` so the `production.issue` hook can take it by work order.
pub async fn stash_wip_issue_plan(tx: &mut Tx<'_>, plan: WipIssuePlan) -> Result<()> {
    let txid = tx.pg_txid().await?;
    stash_with_txid(txid, plan)
}

/// Take the plan stashed for `work_order` on the current task's `Tx`.
pub fn take_wip_issue_plan(work_order: Identifier) -> Option<WipIssuePlan> {
    let caller = current_caller();
    let mut store = lock_plans();
    let txid = store.caller_txid.get(&caller)?.clone();
    store.plans.remove(&(txid, work_order))
}

/// Drop any plan for `work_order` on the current task's `Tx` (error / after-transition).
pub fn clear_wip_issue_plan(work_order: Identifier) {
    let caller = current_caller();
    let mut store = lock_plans();
    let Some(txid) = store.caller_txid.get(&caller).cloned() else {
        return;
    };
    store.plans.remove(&(txid.clone(), work_order));
    if !store.plans.keys().any(|(t, _)| t == &txid) {
        store.caller_txid.remove(&caller);
    }
}

#[cfg(test)]
mod isolation_tests {
    use super::*;
    use datum_core::Identifier;

    fn dummy_plan(work_order: Identifier, document_id: Identifier) -> WipIssuePlan {
        WipIssuePlan {
            document_id,
            work_order,
            from_location: LocationId::from_uuid(uuid::Uuid::nil()),
            lines: Vec::new(),
            residuals: Vec::new(),
            replay_document: None,
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn wip_issue_plans_are_isolated_per_tx_identity() {
        let work_order = Identifier::generate();
        let doc_a = Identifier::generate();
        let doc_b = Identifier::generate();
        let plan_a = dummy_plan(work_order, doc_a);
        let plan_b = dummy_plan(work_order, doc_b);

        let a = tokio::spawn(async move {
            stash_with_txid("xid-a".into(), plan_a).expect("stash a");
            tokio::task::yield_now().await;
            take_wip_issue_plan(work_order)
        });
        let b = tokio::spawn(async move {
            stash_with_txid("xid-b".into(), plan_b).expect("stash b");
            tokio::task::yield_now().await;
            take_wip_issue_plan(work_order)
        });
        let got_a = a.await.expect("join a");
        let got_b = b.await.expect("join b");

        assert_eq!(
            got_a.expect("task A plan").document_id,
            doc_a,
            "task A must not observe task B's plan for the same work order"
        );
        assert_eq!(
            got_b.expect("task B plan").document_id,
            doc_b,
            "task B must not observe task A's plan for the same work order"
        );
    }
}
