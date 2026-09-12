//! Rebuildable balance and layer projections (PLAN invariant 1, SPEC item 7).

use chrono::{DateTime, Utc};
use datum_core::{Identifier, ItemId, LocationId, LotId, SerialId, UnitId};
use datum_db::Tx;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::{Error, Result};

/// Slice of on-hand quantity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BalanceSlice {
    /// Item.
    pub item: ItemId,
    /// Location.
    pub location: LocationId,
    /// Lot, when tracked.
    pub lot: Option<LotId>,
    /// Serial, when tracked.
    pub serial: Option<SerialId>,
    /// Stock unit.
    pub unit: UnitId,
}

/// Rebuild both projections from the ledger fold. Ledger wins.
pub async fn rebuild(tx: &mut Tx<'_>) -> Result<()> {
    tx.execute(sqlx::query("DELETE FROM transient.balance_projection"))
        .await?;
    tx.execute(sqlx::query("DELETE FROM transient.layer_projection"))
        .await?;
    tx.execute(sqlx::query(
        r#"
        INSERT INTO transient.balance_projection (item_id, location_id, lot_id, serial_id, uom_id, quantity)
        SELECT item_id, location_id, lot_id, serial_id, uom_id, SUM(quantity)
          FROM ledger.posting
         WHERE measure = 'QUANTITY'
         GROUP BY item_id, location_id, lot_id, serial_id, uom_id
        "#,
    ))
    .await?;
    tx.execute(sqlx::query(
        r#"
        INSERT INTO transient.layer_projection (posting_id, remaining_qty, remaining_amt)
        SELECT p.posting_id,
               p.quantity - COALESCE((
                   SELECT SUM(c.quantity) FROM ledger.consumption c
                    WHERE c.consumed_posting_id = p.posting_id
               ), 0),
               COALESCE((
                   SELECT SUM(v.amount) FROM ledger.posting v
                    WHERE v.values_posting_id = p.posting_id AND v.measure = 'VALUE'
               ), 0) - COALESCE((
                   SELECT SUM(c.amount) FROM ledger.consumption c
                    WHERE c.consumed_posting_id = p.posting_id
               ), 0)
          FROM ledger.posting p
         WHERE p.measure = 'QUANTITY' AND p.quantity > 0 AND p.boundary IS NULL
        "#,
    ))
    .await?;
    Ok(())
}

/// Apply one group's quantity postings and consumption edges incrementally.
pub async fn apply_group(tx: &mut Tx<'_>, group_id: Identifier) -> Result<()> {
    tx.execute(
        sqlx::query(
            r#"
            INSERT INTO transient.balance_projection
                (item_id, location_id, lot_id, serial_id, uom_id, quantity)
            SELECT item_id, location_id, lot_id, serial_id, uom_id, SUM(quantity)
              FROM ledger.posting
             WHERE measure = 'QUANTITY' AND group_id = $1
             GROUP BY item_id, location_id, lot_id, serial_id, uom_id
            ON CONFLICT (item_id, location_id, lot_id, serial_id, uom_id)
            DO UPDATE SET quantity =
                transient.balance_projection.quantity + EXCLUDED.quantity
            "#,
        )
        .bind(group_id.as_uuid()),
    )
    .await?;

    tx.execute(
        sqlx::query(
            r#"
            INSERT INTO transient.layer_projection (posting_id, remaining_qty, remaining_amt)
            SELECT p.posting_id,
                   p.quantity,
                   COALESCE((
                       SELECT SUM(v.amount) FROM ledger.posting v
                        WHERE v.values_posting_id = p.posting_id AND v.measure = 'VALUE'
                   ), 0)
              FROM ledger.posting p
             WHERE p.group_id = $1
               AND p.measure = 'QUANTITY'
               AND p.quantity > 0
               AND p.boundary IS NULL
            ON CONFLICT (posting_id) DO UPDATE SET
                remaining_qty = EXCLUDED.remaining_qty,
                remaining_amt = EXCLUDED.remaining_amt
            "#,
        )
        .bind(group_id.as_uuid()),
    )
    .await?;

    tx.execute(
        sqlx::query(
            r#"
            UPDATE transient.layer_projection lp
               SET remaining_qty = remaining_qty - s.qty,
                   remaining_amt = remaining_amt - s.amt
              FROM (
                SELECT consumed_posting_id AS posting_id,
                       SUM(quantity) AS qty,
                       SUM(amount) AS amt
                  FROM ledger.consumption
                 WHERE group_id = $1
                 GROUP BY consumed_posting_id
              ) s
             WHERE lp.posting_id = s.posting_id
            "#,
        )
        .bind(group_id.as_uuid()),
    )
    .await?;
    Ok(())
}

/// First divergence between projection and the ledger fold, if any.
pub async fn verify_projection(tx: &mut Tx<'_>) -> Result<()> {
    type BalanceDiv = (
        Uuid,
        Uuid,
        Option<Uuid>,
        Option<Uuid>,
        i64,
        Decimal,
        Decimal,
    );
    let row: Option<BalanceDiv> = tx
        .fetch_optional(sqlx::query_as(
            r#"
            WITH fold AS (
              SELECT item_id, location_id, lot_id, serial_id, uom_id, SUM(quantity) AS qty
                FROM ledger.posting
               WHERE measure = 'QUANTITY'
               GROUP BY 1,2,3,4,5
            )
            SELECT COALESCE(f.item_id, p.item_id),
                   COALESCE(f.location_id, p.location_id),
                   COALESCE(f.lot_id, p.lot_id),
                   COALESCE(f.serial_id, p.serial_id),
                   COALESCE(f.uom_id, p.uom_id),
                   COALESCE(f.qty, 0),
                   COALESCE(p.quantity, 0)
              FROM fold f
              FULL OUTER JOIN transient.balance_projection p
                ON p.item_id = f.item_id
               AND p.location_id = f.location_id
               AND p.uom_id = f.uom_id
               AND p.lot_id IS NOT DISTINCT FROM f.lot_id
               AND p.serial_id IS NOT DISTINCT FROM f.serial_id
             WHERE COALESCE(f.qty, 0) <> COALESCE(p.quantity, 0)
             LIMIT 1
            "#,
        ))
        .await?;
    if let Some((item, loc, lot, serial, uom, fold, proj)) = row {
        return Err(Error::ProjectionDivergence(format!(
            "item={item} loc={loc} lot={lot:?} serial={serial:?} uom={uom} fold={fold} proj={proj}"
        )));
    }

    let layer: Option<(i64, Decimal, Decimal, Decimal, Decimal)> = tx
        .fetch_optional(sqlx::query_as(
            r#"
            WITH fold AS (
              SELECT p.posting_id,
                     p.quantity - COALESCE((
                         SELECT SUM(c.quantity) FROM ledger.consumption c
                          WHERE c.consumed_posting_id = p.posting_id
                     ), 0) AS remaining_qty,
                     COALESCE((
                         SELECT SUM(v.amount) FROM ledger.posting v
                          WHERE v.values_posting_id = p.posting_id AND v.measure = 'VALUE'
                     ), 0) - COALESCE((
                         SELECT SUM(c.amount) FROM ledger.consumption c
                          WHERE c.consumed_posting_id = p.posting_id
                     ), 0) AS remaining_amt
                FROM ledger.posting p
               WHERE p.measure = 'QUANTITY' AND p.quantity > 0 AND p.boundary IS NULL
            )
            SELECT COALESCE(f.posting_id, lp.posting_id),
                   COALESCE(f.remaining_qty, 0),
                   COALESCE(lp.remaining_qty, 0),
                   COALESCE(f.remaining_amt, 0),
                   COALESCE(lp.remaining_amt, 0)
              FROM fold f
              FULL OUTER JOIN transient.layer_projection lp
                ON lp.posting_id = f.posting_id
             WHERE COALESCE(f.remaining_qty, 0) <> COALESCE(lp.remaining_qty, 0)
                OR COALESCE(f.remaining_amt, 0) <> COALESCE(lp.remaining_amt, 0)
             LIMIT 1
            "#,
        ))
        .await?;
    if let Some((pid, fold_q, proj_q, fold_a, proj_a)) = layer {
        return Err(Error::ProjectionDivergence(format!(
            "layer posting={pid} qty fold={fold_q} proj={proj_q} amt fold={fold_a} proj={proj_a}"
        )));
    }
    Ok(())
}

/// Fold quantity postings with `posted_at <= instant` for `slice` (PLAN §7 criterion 8).
pub async fn balance_at(
    tx: &mut Tx<'_>,
    slice: BalanceSlice,
    instant: DateTime<Utc>,
) -> Result<Decimal> {
    let row: (Option<Decimal>,) = tx
        .fetch_one(
            sqlx::query_as(
                r#"
                SELECT SUM(p.quantity)
                  FROM ledger.posting p
                  JOIN ledger.posting_group g ON g.group_id = p.group_id
                 WHERE p.measure = 'QUANTITY'
                   AND p.item_id = $1
                   AND p.location_id = $2
                   AND p.uom_id = $3
                   AND p.lot_id IS NOT DISTINCT FROM $4
                   AND p.serial_id IS NOT DISTINCT FROM $5
                   AND g.posted_at <= $6
                "#,
            )
            .bind(slice.item.as_uuid())
            .bind(slice.location.as_uuid())
            .bind(slice.unit.0)
            .bind(slice.lot.map(|l| l.as_uuid()))
            .bind(slice.serial.map(|s| s.as_uuid()))
            .bind(instant),
        )
        .await?;
    Ok(row.0.unwrap_or(Decimal::ZERO))
}
