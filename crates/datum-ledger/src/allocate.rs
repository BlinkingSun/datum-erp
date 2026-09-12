//! Automatic layer allocation (CONTRACT §6.2 rule 3, D-W1-3 (c)).
//!
//! Remaining layer quantity is re-derived from the ledger
//! (`posting.quantity − SUM(consumption.quantity)`). Money comes from the
//! consumed layer's own stored quantity and amount, never from the consuming
//! posting's value rows.

use std::sync::{Mutex, OnceLock};

use datum_core::{
    AnyQuantity, CostElement, CurrencyId, ItemId, LocationId, LotId, Money, PostingError,
    PostingHandle, PostingId, QuantityPosting, SerialId, ValueAccount, ValuePosting,
};
use datum_db::Tx;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::enums::CostMethod;
use crate::registry::StockItem;
use crate::{Error, Result};

/// An open cost layer re-derived from the ledger.
#[derive(Debug, Clone)]
pub struct Layer {
    /// Immutable posting id.
    pub posting_id: PostingId,
    /// Item.
    pub item: ItemId,
    /// Lot, if any.
    pub lot: Option<LotId>,
    /// Serial, if any.
    pub serial: Option<SerialId>,
    /// Remaining quantity (stock unit).
    pub remaining_qty: Decimal,
    /// Remaining money attached to the layer.
    pub remaining_amt: Decimal,
    /// Currency of `remaining_amt`.
    pub currency: CurrencyId,
    /// Stock unit.
    pub uom: datum_core::UnitId,
    /// Dimension of the quantity (from the withdrawal; layers share the item's unit).
    pub location: LocationId,
}

/// One consumption edge the allocator (or an explicit pick) will insert.
#[derive(Debug, Clone)]
pub struct AllocationEdge {
    /// Consuming quantity posting (this group).
    pub consuming: PostingHandle,
    /// Layer being consumed.
    pub consumed: PostingId,
    /// Signed quantity of the edge (positive for a withdrawal).
    pub quantity: AnyQuantity,
    /// Signed money of the edge (positive for a withdrawal).
    pub amount: Money,
}

/// Extra value rows the STANDARD method posts (PPV and its counterpart).
#[derive(Debug, Clone)]
pub struct ExtraValue {
    /// Value posting to insert in the same group.
    pub posting: ValuePosting,
}

/// SQL the allocator last ran, for `p3_independent_sources`.
fn query_log() -> &'static Mutex<Vec<String>> {
    static LOG: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    LOG.get_or_init(|| Mutex::new(Vec::new()))
}

/// Record an allocator SQL string. Tests read this with [`take_query_log`].
pub fn record_query(sql: &str) {
    if let Ok(mut g) = query_log().lock() {
        g.push(sql.to_string());
    }
}

/// Drain the allocator query log (test instrumentation).
pub fn take_query_log() -> Vec<String> {
    query_log()
        .lock()
        .map(|mut g| std::mem::take(&mut *g))
        .unwrap_or_default()
}

/// Layer-selection query: remaining qty is posting.quantity − SUM(consumption).
/// Money remaining is SUM(value rows attached to the *layer*) − SUM(consumption.amount).
/// The consuming posting's value rows are not read.
pub const SQL_OPEN_LAYERS: &str = r#"
SELECT p.posting_id,
       p.item_id,
       p.lot_id,
       p.serial_id,
       p.location_id,
       p.uom_id,
       p.quantity
         - COALESCE((
             SELECT SUM(c.quantity) FROM ledger.consumption c
              WHERE c.consumed_posting_id = p.posting_id
           ), 0) AS remaining_qty,
       COALESCE((
             SELECT SUM(v.amount) FROM ledger.posting v
              WHERE v.values_posting_id = p.posting_id
                AND v.measure = 'VALUE'
           ), 0)
         - COALESCE((
             SELECT SUM(c.amount) FROM ledger.consumption c
              WHERE c.consumed_posting_id = p.posting_id
           ), 0) AS remaining_amt,
       COALESCE((
             SELECT v.currency_id FROM ledger.posting v
              WHERE v.values_posting_id = p.posting_id
                AND v.measure = 'VALUE'
              LIMIT 1
           ), 840) AS currency_id,
       g.posted_at,
       p.posting_id AS ord
  FROM ledger.posting p
  JOIN ledger.posting_group g ON g.group_id = p.group_id
 WHERE p.measure = 'QUANTITY'
   AND p.item_id = $1
   AND p.location_id = $2
   AND p.boundary IS NULL
   AND p.quantity > 0
 ORDER BY g.posted_at ASC, p.posting_id ASC
"#;

type LayerRow = (
    i64,
    Uuid,
    Option<Uuid>,
    Option<Uuid>,
    Uuid,
    i64,
    Decimal,
    Decimal,
    i32,
    chrono::DateTime<chrono::Utc>,
    i64,
);

/// Load open layers for `item` at `location`, oldest first. Remaining is derived, never stored.
pub async fn load_open_layers(
    tx: &mut Tx<'_>,
    item: ItemId,
    location: LocationId,
) -> Result<Vec<Layer>> {
    record_query(SQL_OPEN_LAYERS);
    let rows: Vec<LayerRow> = tx
        .fetch_all(
            sqlx::query_as(SQL_OPEN_LAYERS)
                .bind(item.as_uuid())
                .bind(location.as_uuid()),
        )
        .await?;
    let mut out = Vec::new();
    for (pid, item_id, lot, serial, loc, uom, rem_q, rem_a, cur, _at, _) in rows {
        if rem_q <= Decimal::ZERO {
            continue;
        }
        out.push(Layer {
            posting_id: PostingId(pid),
            item: ItemId::from_uuid(item_id),
            lot: lot.map(LotId::from_uuid),
            serial: serial.map(SerialId::from_uuid),
            remaining_qty: rem_q,
            remaining_amt: rem_a,
            currency: CurrencyId(cur),
            uom: datum_core::UnitId(uom),
            location: LocationId::from_uuid(loc),
        });
    }
    Ok(out)
}

fn lot_ok(withdrawal: &QuantityPosting, layer: &Layer) -> bool {
    match (withdrawal.lot, layer.lot) {
        (Some(w), Some(l)) => w == l,
        (Some(_), None) => false,
        (None, _) => true,
    }
}

fn serial_ok(withdrawal: &QuantityPosting, layer: &Layer) -> bool {
    match (withdrawal.serial, layer.serial) {
        (Some(w), Some(s)) => w == s,
        (Some(_), None) => false,
        (None, _) => true,
    }
}

/// Check an explicit consumption edge against a live layer.
pub fn check_explicit_layer(
    consuming: PostingHandle,
    withdrawal: &QuantityPosting,
    layer: &Layer,
    take_qty: Decimal,
) -> core::result::Result<(), PostingError> {
    if layer.item != withdrawal.item || layer.location != withdrawal.location {
        return Err(PostingError::IneligibleLayer {
            consuming,
            consumed: layer.posting_id,
        });
    }
    if !lot_ok(withdrawal, layer) || !serial_ok(withdrawal, layer) {
        return Err(PostingError::IneligibleLayer {
            consuming,
            consumed: layer.posting_id,
        });
    }
    if take_qty.abs() > layer.remaining_qty {
        return Err(PostingError::IneligibleLayer {
            consuming,
            consumed: layer.posting_id,
        });
    }
    Ok(())
}

fn eligible<'a>(withdrawal: &QuantityPosting, layers: &'a [Layer]) -> Vec<&'a Layer> {
    layers
        .iter()
        .filter(|l| {
            l.item == withdrawal.item
                && l.location == withdrawal.location
                && lot_ok(withdrawal, l)
                && serial_ok(withdrawal, l)
        })
        .collect()
}

fn money_for_take(layer: &Layer, take: Decimal) -> Result<Money> {
    if layer.remaining_qty.is_zero() {
        return Err(Error::Posting(PostingError::IneligibleLayer {
            consuming: PostingHandle(0),
            consumed: layer.posting_id,
        }));
    }
    let amt = if take >= layer.remaining_qty {
        layer.remaining_amt
    } else {
        (layer.remaining_amt * take / layer.remaining_qty)
            .round_dp_with_strategy(6, rust_decimal::RoundingStrategy::MidpointNearestEven)
    };
    Ok(Money::new(amt.round_dp(6), layer.currency).map_err(datum_core::Error::from)?)
}

fn edge(
    handle: PostingHandle,
    layer: &Layer,
    take: Decimal,
    qty_template: &AnyQuantity,
    amount: Money,
) -> AllocationEdge {
    AllocationEdge {
        consuming: handle,
        consumed: layer.posting_id,
        quantity: AnyQuantity {
            amount: take,
            unit: qty_template.unit,
            dimension: qty_template.dimension,
        },
        amount,
    }
}

/// Allocate a withdrawal. Fails with [`PostingError::AllocationRequired`] rather than clamp.
pub fn allocate_withdrawal(
    handle: PostingHandle,
    withdrawal: &QuantityPosting,
    stock: &StockItem,
    layers: &[Layer],
) -> core::result::Result<(Vec<AllocationEdge>, Vec<ExtraValue>), PostingError> {
    let need = withdrawal.quantity.amount.abs();
    if need.is_zero() {
        return Err(PostingError::Shape("zero withdrawal".into()));
    }
    let pool = eligible(withdrawal, layers);
    match stock.cost_method {
        CostMethod::Fifo | CostMethod::Standard => {
            fifo_or_standard(handle, withdrawal, stock, &pool, need)
        }
        CostMethod::MovingAvg => moving_avg(handle, withdrawal, &pool, need),
    }
}

fn fifo_or_standard(
    handle: PostingHandle,
    withdrawal: &QuantityPosting,
    stock: &StockItem,
    pool: &[&Layer],
    mut need: Decimal,
) -> core::result::Result<(Vec<AllocationEdge>, Vec<ExtraValue>), PostingError> {
    let mut edges = Vec::new();
    let mut actual = Decimal::ZERO;
    let mut standard_total = Decimal::ZERO;
    let mut currency = CurrencyId(840);
    for layer in pool {
        if need <= Decimal::ZERO {
            break;
        }
        let take = if layer.remaining_qty < need {
            layer.remaining_qty
        } else {
            need
        };
        let layer_amt =
            money_for_take(layer, take).map_err(|_| PostingError::AllocationRequired(handle))?;
        currency = layer.currency;
        actual += layer_amt.amount();
        let edge_amt = if stock.cost_method == CostMethod::Standard {
            let std = stock
                .standard
                .ok_or_else(|| PostingError::Shape("STANDARD item missing standard_cost".into()))?;
            let std_amt = (std.amount() * take)
                .round_dp_with_strategy(6, rust_decimal::RoundingStrategy::ToZero);
            standard_total += std_amt;
            Money::new(std_amt, std.currency()).map_err(|e| PostingError::Shape(e.to_string()))?
        } else {
            layer_amt
        };
        edges.push(edge(handle, layer, take, &withdrawal.quantity, edge_amt));
        need -= take;
    }
    if need > Decimal::ZERO {
        return Err(PostingError::AllocationRequired(handle));
    }
    let mut extra = Vec::new();
    if stock.cost_method == CostMethod::Standard {
        let diff = actual - standard_total;
        if !diff.is_zero() {
            let ppv = Money::new(diff, currency).map_err(|e| PostingError::Shape(e.to_string()))?;
            extra.push(ExtraValue {
                posting: ValuePosting {
                    account: ValueAccount::Ppv,
                    cost_element: CostElement::Material,
                    cost_object: None,
                    amount: ppv,
                    values: Some(handle),
                },
            });
            extra.push(ExtraValue {
                posting: ValuePosting {
                    account: ValueAccount::Inventory,
                    cost_element: CostElement::Material,
                    cost_object: None,
                    amount: ppv.negate(),
                    values: Some(handle),
                },
            });
        }
    }
    Ok((edges, extra))
}

fn moving_avg(
    handle: PostingHandle,
    withdrawal: &QuantityPosting,
    pool: &[&Layer],
    need: Decimal,
) -> core::result::Result<(Vec<AllocationEdge>, Vec<ExtraValue>), PostingError> {
    let pool_qty: Decimal = pool.iter().map(|l| l.remaining_qty).sum();
    if pool_qty < need || pool_qty.is_zero() {
        return Err(PostingError::AllocationRequired(handle));
    }
    let pool_amt: Decimal = pool.iter().map(|l| l.remaining_amt).sum();
    let currency = pool.first().map(|l| l.currency).unwrap_or(CurrencyId(840));
    // Pro-rata weights at scale 8 (quantity column).
    let mut weights: Vec<u64> = Vec::new();
    for layer in pool {
        let w = (layer.remaining_qty * Decimal::from(100_000_000u64))
            .round()
            .mantissa()
            .unsigned_abs();
        weights.push(u64::try_from(w).unwrap_or(u64::MAX));
    }
    let total_money = Money::new(
        (pool_amt * (need / pool_qty))
            .round_dp_with_strategy(6, rust_decimal::RoundingStrategy::ToZero),
        currency,
    )
    .map_err(|e| PostingError::Shape(e.to_string()))?;
    let parts = total_money
        .allocate(&weights, 6)
        .map_err(|e| PostingError::Shape(e.to_string()))?;
    let qty_parts = {
        let qty_money = Money::new(
            need.round_dp_with_strategy(6, rust_decimal::RoundingStrategy::ToZero),
            currency,
        )
        .map_err(|e| PostingError::Shape(e.to_string()))?;
        qty_money
            .allocate(&weights, 6)
            .map_err(|e| PostingError::Shape(e.to_string()))?
    };
    let mut edges = Vec::new();
    let mut assigned = Decimal::ZERO;
    for (i, layer) in pool.iter().enumerate() {
        let take = qty_parts
            .get(i)
            .map(|m| m.amount())
            .unwrap_or(Decimal::ZERO);
        if take.is_zero() {
            continue;
        }
        let amt = parts
            .get(i)
            .copied()
            .unwrap_or_else(|| Money::zero(currency));
        edges.push(edge(handle, layer, take, &withdrawal.quantity, amt));
        assigned += take;
    }
    // Largest-remainder on money may leave a quantity dust vs `need` because we
    // allocated at money scale 6. Put any leftover on the last edge rather than
    // invent a layer (and fail if nothing was assigned).
    let leftover = need - assigned;
    if leftover > Decimal::ZERO {
        // SPEC deliverable 4: fail rather than clamp leftover onto the last edge.
        return Err(PostingError::AllocationRequired(handle));
    }
    Ok((edges, Vec::new()))
}

/// True when `sql` reads value rows of a *consuming* (withdrawal) posting.
///
/// The allocator must derive layer money from the consumed layer
/// (`values_posting_id = p.posting_id` on a receipt layer) and must not look
/// at the consuming posting's value rows (D-W1-3 (c) / `p3_independent_sources`).
pub fn sql_reads_consuming_value_rows(sql: &str) -> bool {
    let s = sql.to_ascii_lowercase();
    let reads_value = s.contains("values_posting_id")
        || s.contains("measure = 'value'")
        || s.contains("v.amount");
    if !reads_value {
        return false;
    }
    s.contains("consuming_posting_id")
        || (s.contains("consuming") && s.contains("quantity < 0"))
        || s.contains("p.quantity < 0")
}

/// Sum of explicit edges versus the withdrawal (P3 both halves).
pub fn explicit_matches(
    withdrawal: &QuantityPosting,
    value_sum: Money,
    edges: &[AllocationEdge],
) -> bool {
    let qty: Decimal = edges.iter().map(|e| e.quantity.amount).sum();
    let amt = Money::try_sum(edges.iter().map(|e| e.amount))
        .ok()
        .flatten();
    let qty_ok = qty == withdrawal.quantity.amount.abs();
    let amt_ok = match amt {
        Some(a) => a.amount() == value_sum.amount().abs() && a.currency() == value_sum.currency(),
        None => value_sum.amount().is_zero(),
    };
    qty_ok && amt_ok
}
