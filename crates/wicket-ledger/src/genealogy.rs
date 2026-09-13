//! Backward and forward traces over `ledger.consumption` (D2 §5.3).

use rust_decimal::Decimal;
use uuid::Uuid;
use wicket_core::{
    AnyQuantity, CurrencyId, DimensionKind, LotId, Money, PostingId, SerialId, UnitId,
};
use wicket_db::Tx;

use crate::Result;

/// One node of a genealogy tree.
#[derive(Debug, Clone)]
pub struct Node {
    /// Posting this node represents.
    pub posting: PostingId,
    /// Lot on the posting, if any.
    pub lot: Option<LotId>,
    /// Serial on the posting, if any.
    pub serial: Option<SerialId>,
    /// Edge quantity into this node (stock unit of the consumed posting).
    pub quantity: AnyQuantity,
    /// Edge amount into this node.
    pub amount: Money,
    /// Recursed children.
    pub children: Vec<Node>,
}

/// Start of a trace: a posting id or a lot id.
#[derive(Debug, Clone, Copy)]
pub enum TraceStart {
    /// An existing posting.
    Posting(PostingId),
    /// All quantity postings of this lot.
    Lot(LotId),
}

type EdgeRow = (
    i64,
    i64,
    Decimal,
    Decimal,
    Option<Uuid>,
    Option<Uuid>,
    i64,
    i16,
);

const SQL_BACKWARD: &str = r#"
WITH RECURSIVE t AS (
  SELECT c.consuming_posting_id,
         c.consumed_posting_id,
         c.quantity,
         c.amount
    FROM ledger.consumption c
   WHERE c.consuming_posting_id = ANY($1::bigint[])
  UNION ALL
  SELECT c.consuming_posting_id,
         c.consumed_posting_id,
         c.quantity,
         c.amount
    FROM ledger.consumption c
    JOIN t ON c.consuming_posting_id = t.consumed_posting_id
)
SELECT t.consuming_posting_id, t.consumed_posting_id, t.quantity, t.amount,
       p.lot_id, p.serial_id, p.uom_id,
       COALESCE((
         SELECT v.currency_id FROM ledger.posting v
          WHERE v.values_posting_id = t.consumed_posting_id LIMIT 1
       ), 840::smallint)
  FROM t
  JOIN ledger.posting p ON p.posting_id = t.consumed_posting_id
"#;

const SQL_FORWARD: &str = r#"
WITH RECURSIVE t AS (
  SELECT c.consuming_posting_id,
         c.consumed_posting_id,
         c.quantity,
         c.amount
    FROM ledger.consumption c
   WHERE c.consumed_posting_id = ANY($1::bigint[])
  UNION ALL
  SELECT c.consuming_posting_id,
         c.consumed_posting_id,
         c.quantity,
         c.amount
    FROM ledger.consumption c
    JOIN t ON c.consumed_posting_id = t.consuming_posting_id
)
SELECT t.consuming_posting_id, t.consumed_posting_id, t.quantity, t.amount,
       p.lot_id, p.serial_id, p.uom_id,
       COALESCE((
         SELECT v.currency_id FROM ledger.posting v
          WHERE v.values_posting_id = t.consuming_posting_id LIMIT 1
       ), 840::smallint)
  FROM t
  JOIN ledger.posting p ON p.posting_id = t.consuming_posting_id
"#;

async fn start_ids(tx: &mut Tx<'_>, start: TraceStart) -> Result<Vec<i64>> {
    match start {
        TraceStart::Posting(p) => Ok(vec![p.0]),
        TraceStart::Lot(lot) => {
            let rows: Vec<(i64,)> = tx
                .fetch_all(
                    sqlx::query_as(
                        "SELECT posting_id FROM ledger.posting
                          WHERE lot_id = $1 AND measure = 'QUANTITY'",
                    )
                    .bind(lot.as_uuid()),
                )
                .await?;
            Ok(rows.into_iter().map(|r| r.0).collect())
        }
    }
}

fn to_node(row: &EdgeRow, children: Vec<Node>, posting: i64) -> Node {
    let (_from, _to, qty, amt, lot, serial, uom, cur) = *row;
    Node {
        posting: PostingId(posting),
        lot: lot.map(LotId::from_uuid),
        serial: serial.map(SerialId::from_uuid),
        quantity: AnyQuantity {
            amount: qty,
            unit: UnitId(uom),
            dimension: crate::infer_dimension(uom),
        },
        amount: Money::new(amt, CurrencyId(i32::from(cur)))
            .unwrap_or_else(|_| Money::zero(CurrencyId(840))),
        children,
    }
}

fn build_tree(
    roots: &[i64],
    edges: &[EdgeRow],
    child_of: impl Fn(&EdgeRow) -> (i64, i64),
) -> Vec<Node> {
    fn rec(
        id: i64,
        edges: &[EdgeRow],
        child_of: &impl Fn(&EdgeRow) -> (i64, i64),
        seen: &mut std::collections::HashSet<i64>,
    ) -> Vec<Node> {
        if !seen.insert(id) {
            return Vec::new();
        }
        let mut kids = Vec::new();
        for e in edges {
            let (parent, child) = child_of(e);
            if parent == id {
                let nested = rec(child, edges, child_of, seen);
                kids.push(to_node(e, nested, child));
            }
        }
        kids
    }
    let mut trees = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for root in roots {
        let children = rec(*root, edges, &child_of, &mut seen);
        trees.push(Node {
            posting: PostingId(*root),
            lot: None,
            serial: None,
            quantity: AnyQuantity {
                amount: Decimal::ZERO,
                unit: UnitId(1),
                dimension: DimensionKind::Count,
            },
            amount: Money::zero(CurrencyId(840)),
            children,
        });
    }
    trees
}

/// Walk consumption edges toward source postings.
pub async fn trace_backward(tx: &mut Tx<'_>, start: TraceStart) -> Result<Vec<Node>> {
    let ids = start_ids(tx, start).await?;
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<EdgeRow> = tx
        .fetch_all(sqlx::query_as(SQL_BACKWARD).bind(&ids))
        .await?;
    Ok(build_tree(&ids, &rows, |e| (e.0, e.1)))
}

/// Walk consumption edges toward later consuming postings.
pub async fn trace_forward(tx: &mut Tx<'_>, start: TraceStart) -> Result<Vec<Node>> {
    let ids = start_ids(tx, start).await?;
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<EdgeRow> = tx.fetch_all(sqlx::query_as(SQL_FORWARD).bind(&ids)).await?;
    Ok(build_tree(&ids, &rows, |e| (e.1, e.0)))
}
