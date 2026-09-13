//! Persistence and traces. Writes go through [`wicket_db::Tx`].
//! Ledger reads go only through [`wicket_ledger::trace_backward`] /
//! [`wicket_ledger::trace_forward`].

use std::collections::{BTreeSet, HashMap};

use rust_decimal::Decimal;
use serde_json::Value;
use uuid::Uuid;
use wicket_core::{Actor, Boundary, ItemId, LotId, PostingId, SerialId};
use wicket_db::{Pool, Tx, WriteContext, WritePool};
use wicket_jobs::{EnqueueOptions, HandlerOutcome, JobHandler, JobId, Progress};
use wicket_ledger::{Node, TraceStart, trace_backward, trace_forward};
use wicket_mod_inventory::document_history;

use crate::domain::{
    DEFAULT_INLINE_MAX_POSTINGS, Direction, ExportFormat, INLINE_MAX_ENV, Impact, TraceOrigin,
    TraceRequest, Tree, TreeNode,
};
use crate::error::Result;

/// Job kind for large traces (declared in `module.toml`).
pub const TRACE_JOB: &str = "genealogy.trace";

/// Configured inline posting threshold (tested default [`DEFAULT_INLINE_MAX_POSTINGS`]).
pub fn inline_max_postings() -> u32 {
    std::env::var(INLINE_MAX_ENV)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_INLINE_MAX_POSTINGS)
}

/// Outcome of [`trace`]: small trees inline, large trees as a 202 job.
#[derive(Debug, Clone)]
pub enum TraceOutcome {
    /// Tree (or both-direction pair) small enough to return now.
    Inline(TraceBody),
    /// Enqueued; poll [`job_status`].
    Accepted {
        /// Job id.
        job_id: JobId,
        /// Result URL (`/api/v1/genealogy/jobs/{id}`).
        result_url: String,
    },
}

/// Body of a finished trace.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum TraceBody {
    /// One direction.
    One(Tree),
    /// Both directions.
    Both {
        /// Backward forest.
        backward: Tree,
        /// Forward forest.
        forward: Tree,
    },
}

fn cache_key(req: &TraceRequest) -> String {
    let start = match req.origin {
        TraceOrigin::Lot(id) => format!("lot:{id}"),
        TraceOrigin::Serial(id) => format!("serial:{id}"),
        TraceOrigin::Posting(PostingId(p)) => format!("posting:{p}"),
    };
    format!(
        "{start}:{}:{}",
        req.direction.as_str(),
        req.depth
            .map(|d| d.to_string())
            .unwrap_or_else(|| "-".into())
    )
}

fn start_from_lot(lot: LotId) -> TraceStart {
    TraceStart::Lot(lot)
}

fn start_from_posting(p: PostingId) -> TraceStart {
    TraceStart::Posting(p)
}

async fn resolve_start(
    tx: &mut Tx<'_>,
    origin: TraceOrigin,
) -> Result<(TraceStart, Option<LotId>, Option<SerialId>)> {
    match origin {
        TraceOrigin::Lot(lot) => {
            let _ = wicket_mod_lots::load_lot(tx, lot).await?;
            Ok((start_from_lot(lot), Some(lot), None))
        }
        TraceOrigin::Serial(serial) => {
            let s = wicket_mod_lots::load_serial(tx, serial).await?;
            Ok((start_from_lot(s.lot), Some(s.lot), Some(serial)))
        }
        TraceOrigin::Posting(p) => Ok((start_from_posting(p), None, None)),
    }
}

fn node_from_ledger(n: &Node, lots: &HashMap<LotId, ItemId>) -> TreeNode {
    TreeNode {
        posting: n.posting.0,
        item: n.lot.and_then(|l| lots.get(&l).copied()),
        lot: n.lot,
        serial: n.serial,
        location: None,
        quantity: n.quantity,
        amount: n.amount.amount(),
        amount_currency: n.amount.currency(),
        occurred_at: None,
        edge_quantity: n.quantity,
        children: n
            .children
            .iter()
            .map(|c| node_from_ledger(c, lots))
            .collect(),
    }
}

fn apply_depth(nodes: Vec<TreeNode>, depth: Option<u32>) -> Vec<TreeNode> {
    let Some(max) = depth else {
        return nodes;
    };
    fn rec(n: TreeNode, at: u32, max: u32) -> TreeNode {
        if at >= max {
            TreeNode {
                children: Vec::new(),
                ..n
            }
        } else {
            TreeNode {
                children: n
                    .children
                    .into_iter()
                    .map(|c| rec(c, at + 1, max))
                    .collect(),
                ..n
            }
        }
    }
    nodes.into_iter().map(|n| rec(n, 0, max)).collect()
}

fn stamp_origin(nodes: &mut [TreeNode], lot: Option<LotId>, serial: Option<SerialId>) {
    for n in nodes {
        if n.lot.is_none() {
            n.lot = lot;
        }
        if n.serial.is_none() && n.lot == lot {
            n.serial = serial;
        }
    }
}

fn count_postings(nodes: &[TreeNode]) -> u32 {
    let mut ids = BTreeSet::new();
    fn walk(n: &TreeNode, ids: &mut BTreeSet<i64>) {
        ids.insert(n.posting);
        for c in &n.children {
            walk(c, ids);
        }
    }
    for n in nodes {
        walk(n, &mut ids);
    }
    ids.len() as u32
}

/// Undirected edge set used by the same-tree assertion (PLAN §3 item 8).
pub fn undirected_edges(nodes: &[TreeNode]) -> BTreeSet<(i64, i64)> {
    let mut out = BTreeSet::new();
    fn walk(n: &TreeNode, out: &mut BTreeSet<(i64, i64)>) {
        for c in &n.children {
            let a = n.posting.min(c.posting);
            let b = n.posting.max(c.posting);
            out.insert((a, b));
            walk(c, out);
        }
    }
    for n in nodes {
        walk(n, &mut out);
    }
    out
}

async fn lot_items(tx: &mut Tx<'_>, nodes: &[Node]) -> Result<HashMap<LotId, ItemId>> {
    let mut ids = BTreeSet::new();
    fn collect(n: &Node, ids: &mut BTreeSet<LotId>) {
        if let Some(l) = n.lot {
            ids.insert(l);
        }
        for c in &n.children {
            collect(c, ids);
        }
    }
    for n in nodes {
        collect(n, &mut ids);
    }
    let mut map = HashMap::new();
    for id in ids {
        if let Ok(lot) = wicket_mod_lots::load_lot(tx, id).await {
            map.insert(id, lot.item);
        }
    }
    Ok(map)
}

async fn enrich_locations(tx: &mut Tx<'_>, nodes: &mut [TreeNode]) -> Result<()> {
    async fn fill(tx: &mut Tx<'_>, n: &mut TreeNode) -> Result<()> {
        if n.location.is_none()
            && let Some(lot) = n.lot
        {
            let hist = document_history(tx, n.item, Some(lot)).await?;
            if let Some(line) = hist
                .iter()
                .flat_map(|d| d.lines.iter())
                .find(|l| l.lot == Some(lot))
            {
                n.location = line.to_location.or(line.from_location);
            }
        }
        for c in &mut n.children {
            Box::pin(fill(tx, c)).await?;
        }
        Ok(())
    }
    for n in nodes {
        fill(tx, n).await?;
    }
    Ok(())
}

async fn ledger_nodes(
    tx: &mut Tx<'_>,
    start: TraceStart,
    direction: Direction,
) -> Result<Vec<Node>> {
    match direction {
        Direction::Backward => Ok(trace_backward(tx, start).await?),
        Direction::Forward => Ok(trace_forward(tx, start).await?),
        Direction::Both => unreachable!("split before call"),
    }
}

async fn build_tree(
    tx: &mut Tx<'_>,
    start: TraceStart,
    origin_lot: Option<LotId>,
    serial: Option<SerialId>,
    direction: Direction,
    depth: Option<u32>,
) -> Result<Tree> {
    let raw = ledger_nodes(tx, start, direction).await?;
    let lots = lot_items(tx, &raw).await?;
    let mut nodes: Vec<TreeNode> = raw.iter().map(|n| node_from_ledger(n, &lots)).collect();
    stamp_origin(&mut nodes, origin_lot, serial);
    nodes = apply_depth(nodes, depth);
    enrich_locations(tx, &mut nodes).await?;
    Ok(Tree { direction, nodes })
}

async fn compute_body(tx: &mut Tx<'_>, req: TraceRequest) -> Result<TraceBody> {
    let (start, origin_lot, serial) = resolve_start(tx, req.origin).await?;
    match req.direction {
        Direction::Both => {
            let backward = build_tree(
                tx,
                start,
                origin_lot,
                serial,
                Direction::Backward,
                req.depth,
            )
            .await?;
            let forward =
                build_tree(tx, start, origin_lot, serial, Direction::Forward, req.depth).await?;
            Ok(TraceBody::Both { backward, forward })
        }
        dir => Ok(TraceBody::One(
            build_tree(tx, start, origin_lot, serial, dir, req.depth).await?,
        )),
    }
}

fn body_posting_count(body: &TraceBody) -> u32 {
    match body {
        TraceBody::One(t) => count_postings(&t.nodes),
        TraceBody::Both { backward, forward } => {
            count_postings(&backward.nodes) + count_postings(&forward.nodes)
        }
    }
}

async fn cache_get(tx: &mut Tx<'_>, key: &str) -> Result<Option<TraceBody>> {
    let row: Option<(Value,)> = tx
        .fetch_optional(
            sqlx::query_as("SELECT tree FROM genealogy_transient.trace_cache WHERE key = $1")
                .bind(key),
        )
        .await?;
    match row {
        Some((v,)) => Ok(Some(serde_json::from_value(v)?)),
        None => Ok(None),
    }
}

async fn cache_put(tx: &mut Tx<'_>, key: &str, body: &TraceBody) -> Result<()> {
    let tree = serde_json::to_value(body)?;
    tx.execute(
        sqlx::query(
            r#"INSERT INTO genealogy_transient.trace_cache (key, computed_at, tree)
               VALUES ($1, now(), $2)
               ON CONFLICT (key) DO UPDATE
                 SET computed_at = now(), tree = EXCLUDED.tree"#,
        )
        .bind(key)
        .bind(tree),
    )
    .await?;
    Ok(())
}

/// Drop every cache row (rebuildable; never authoritative).
pub async fn drop_cache(tx: &mut Tx<'_>) -> Result<u64> {
    let n = tx
        .execute(sqlx::query("DELETE FROM genealogy_transient.trace_cache"))
        .await?
        .rows_affected();
    Ok(n)
}

/// Invalidate the cache (idempotent). Called from the event subscriber.
pub async fn invalidate_cache(tx: &mut Tx<'_>) -> Result<()> {
    let _ = drop_cache(tx).await?;
    Ok(())
}

fn job_ctx(actor: Actor) -> WriteContext {
    let mut ctx = WriteContext::new(actor, "genealogy.trace", "job");
    ctx.actor_display = Some(format!("service:{}", actor.id));
    ctx.reason = Some("genealogy trace job".into());
    ctx
}

fn enqueue_payload(req: &TraceRequest, key: String) -> Result<serde_json::Value> {
    Ok(serde_json::to_value(TraceJobPayload {
        origin_kind: match req.origin {
            TraceOrigin::Lot(_) => "lot",
            TraceOrigin::Serial(_) => "serial",
            TraceOrigin::Posting(_) => "posting",
        }
        .to_string(),
        origin: match req.origin {
            TraceOrigin::Lot(id) => id.to_string(),
            TraceOrigin::Serial(id) => id.to_string(),
            TraceOrigin::Posting(PostingId(p)) => p.to_string(),
        },
        direction: req.direction.as_str().to_string(),
        depth: req.depth,
        cache_key: key,
    })?)
}

/// Run a genealogy trace. Large forests enqueue [`TRACE_JOB`] and return 202.
pub async fn trace(tx: &mut Tx<'_>, actor: Actor, req: TraceRequest) -> Result<TraceOutcome> {
    let key = cache_key(&req);
    if let Some(cached) = cache_get(tx, &key).await? {
        return Ok(TraceOutcome::Inline(cached));
    }
    let limit = req.max_postings.unwrap_or_else(inline_max_postings);
    if limit == 0 {
        let job_id = wicket_jobs::enqueue(
            tx,
            TRACE_JOB,
            enqueue_payload(&req, key)?,
            actor.id,
            EnqueueOptions::default(),
        )
        .await?;
        return Ok(TraceOutcome::Accepted {
            job_id,
            result_url: format!("/api/v1/genealogy/jobs/{}", job_id.0),
        });
    }
    let body = compute_body(tx, req).await?;
    let n = body_posting_count(&body);
    if n > limit {
        let job_id = wicket_jobs::enqueue(
            tx,
            TRACE_JOB,
            enqueue_payload(&req, key)?,
            actor.id,
            EnqueueOptions::default(),
        )
        .await?;
        return Ok(TraceOutcome::Accepted {
            job_id,
            result_url: format!("/api/v1/genealogy/jobs/{}", job_id.0),
        });
    }
    cache_put(tx, &key, &body).await?;
    Ok(TraceOutcome::Inline(body))
}

/// Compute without the size gate (used by the job handler and cache-rebuild test).
pub async fn trace_inline(tx: &mut Tx<'_>, req: TraceRequest) -> Result<TraceBody> {
    let key = cache_key(&req);
    let body = compute_body(tx, req).await?;
    cache_put(tx, &key, &body).await?;
    Ok(body)
}

/// Forward closure to CUSTOMER shipments (D2 case g; the mockup recall list).
pub async fn impact(tx: &mut Tx<'_>, lot: LotId) -> Result<Impact> {
    let lot_row = wicket_mod_lots::load_lot(tx, lot).await?;
    let _ = wicket_ledger::has_postings(tx, lot_row.item).await?;
    let fwd = build_tree(
        tx,
        start_from_lot(lot),
        Some(lot),
        None,
        Direction::Forward,
        None,
    )
    .await?;
    let mut lots = BTreeSet::new();
    fn collect_lots(n: &TreeNode, lots: &mut BTreeSet<LotId>) {
        if let Some(l) = n.lot {
            lots.insert(l);
        }
        for c in &n.children {
            collect_lots(c, lots);
        }
    }
    for n in &fwd.nodes {
        collect_lots(n, &mut lots);
    }
    lots.insert(lot);
    let customer = wicket_mod_locations::boundary_location_id(tx, Boundary::Customer).await?;
    let mut shipments = Vec::new();
    let mut customers = Vec::new();
    let mut units = BTreeSet::new();
    for l in lots {
        let hist = document_history(tx, None, Some(l)).await?;
        for doc in hist {
            let ships = doc
                .lines
                .iter()
                .any(|line| line.to_location == Some(customer));
            if !ships {
                continue;
            }
            shipments.push(doc.id);
            if let Some(r) = doc.reference.clone() {
                customers.push(r);
            }
            for line in &doc.lines {
                if let Some(s) = line.serial {
                    units.insert(s);
                }
            }
        }
    }
    shipments.sort_by_key(|id| id.as_uuid());
    shipments.dedup();
    customers.sort();
    customers.dedup();
    Ok(Impact {
        shipments,
        customers,
        units: units.into_iter().collect(),
    })
}

/// Where-used: forward traces of lots of `item` at `revision`.
pub async fn where_used(
    tx: &mut Tx<'_>,
    pool: &Pool,
    item: ItemId,
    revision: &str,
) -> Result<Vec<Tree>> {
    let rec = wicket_mod_items::get(pool, item).await?;
    if rec.revision != revision {
        return Ok(Vec::new());
    }
    if !wicket_ledger::has_postings(tx, item).await? {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let page = wicket_mod_lots::list_lots(tx, Some(200), cursor.as_deref()).await?;
        for lot in page.data {
            if lot.item_id != item {
                continue;
            }
            out.push(
                build_tree(
                    tx,
                    start_from_lot(lot.id),
                    Some(lot.id),
                    None,
                    Direction::Forward,
                    None,
                )
                .await?,
            );
        }
        if !page.has_more {
            break;
        }
        cursor = page.next_cursor;
    }
    Ok(out)
}

/// Job status (read pool; no write).
pub async fn job_status(pool: &Pool, id: JobId) -> Result<Option<wicket_jobs::JobStatus>> {
    Ok(wicket_jobs::status(pool, id).await?)
}

/// Flatten a tree to CSV (export).
pub fn tree_csv(tree: &Tree) -> String {
    let mut rows =
        vec!["posting,item,lot,serial,location,quantity,amount,occurred_at,edge_quantity".into()];
    fn walk(n: &TreeNode, rows: &mut Vec<String>) {
        rows.push(format!(
            "{},{},{},{},{},{},{},{},{}",
            n.posting,
            n.item.map(|i| i.to_string()).unwrap_or_default(),
            n.lot.map(|i| i.to_string()).unwrap_or_default(),
            n.serial.map(|i| i.to_string()).unwrap_or_default(),
            n.location.map(|i| i.to_string()).unwrap_or_default(),
            n.quantity.amount,
            n.amount,
            n.occurred_at.map(|t| t.to_rfc3339()).unwrap_or_default(),
            n.edge_quantity.amount,
        ));
        for c in &n.children {
            walk(c, rows);
        }
    }
    for n in &tree.nodes {
        walk(n, &mut rows);
    }
    rows.join("\n")
}

/// Render a trace body as JSON or CSV.
pub fn export_body(body: &TraceBody, format: ExportFormat) -> Result<String> {
    match format {
        ExportFormat::Json => Ok(serde_json::to_string(body)?),
        ExportFormat::Csv => match body {
            TraceBody::One(t) => Ok(tree_csv(t)),
            TraceBody::Both { backward, forward } => {
                Ok(format!("{}\n{}", tree_csv(backward), tree_csv(forward)))
            }
        },
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct TraceJobPayload {
    origin_kind: String,
    origin: String,
    direction: String,
    depth: Option<u32>,
    cache_key: String,
}

/// `JobHandler` for [`TRACE_JOB`]. Holds the app pool so compute can open a `Tx`.
#[derive(Clone)]
pub struct TraceJob {
    /// App pool cloned from the kernel.
    pub pool: Pool,
    /// Service principal that writes the cache.
    pub actor: Actor,
}

impl JobHandler for TraceJob {
    fn run(
        &self,
        payload: &Value,
        progress: Progress,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = wicket_jobs::Result<HandlerOutcome>> + Send + '_>,
    > {
        let payload = payload.clone();
        let pool = self.pool.clone();
        let actor = self.actor;
        Box::pin(async move {
            progress
                .report(25, "tracing")
                .await
                .map_err(|e| wicket_jobs::Error::Invariant(e.to_string()))?;
            let parsed: TraceJobPayload = serde_json::from_value(payload)
                .map_err(|e| wicket_jobs::Error::Invariant(e.to_string()))?;
            let origin = match parsed.origin_kind.as_str() {
                "lot" => {
                    let u = Uuid::parse_str(&parsed.origin)
                        .map_err(|e| wicket_jobs::Error::Invariant(e.to_string()))?;
                    TraceOrigin::Lot(LotId::from_uuid(u))
                }
                "serial" => {
                    let u = Uuid::parse_str(&parsed.origin)
                        .map_err(|e| wicket_jobs::Error::Invariant(e.to_string()))?;
                    TraceOrigin::Serial(SerialId::from_uuid(u))
                }
                "posting" => {
                    let p: i64 = parsed
                        .origin
                        .parse()
                        .map_err(|e: std::num::ParseIntError| {
                            wicket_jobs::Error::Invariant(e.to_string())
                        })?;
                    TraceOrigin::Posting(PostingId(p))
                }
                other => {
                    return Err(wicket_jobs::Error::Invariant(format!(
                        "unknown origin kind {other}"
                    )));
                }
            };
            let direction = Direction::parse(&parsed.direction)
                .ok_or_else(|| wicket_jobs::Error::Invariant("invalid direction".into()))?;
            let req = TraceRequest {
                origin,
                direction,
                depth: parsed.depth,
                max_postings: None,
            };
            let write = WritePool::new(pool);
            let ctx = job_ctx(actor);
            let mut tx = Tx::begin(&write, &ctx)
                .await
                .map_err(|e| wicket_jobs::Error::Invariant(e.to_string()))?;
            let body = compute_body(&mut tx, req)
                .await
                .map_err(|e| wicket_jobs::Error::Invariant(e.to_string()))?;
            cache_put(&mut tx, &parsed.cache_key, &body)
                .await
                .map_err(|e| wicket_jobs::Error::Invariant(e.to_string()))?;
            tx.commit()
                .await
                .map_err(|e| wicket_jobs::Error::Invariant(e.to_string()))?;
            progress
                .report(100, "done")
                .await
                .map_err(|e| wicket_jobs::Error::Invariant(e.to_string()))?;
            let value = serde_json::to_value(&body)
                .map_err(|e| wicket_jobs::Error::Invariant(e.to_string()))?;
            Ok(HandlerOutcome::Done(value))
        })
    }
}

/// Public cache key for tests.
pub fn cache_key_for(req: &TraceRequest) -> String {
    cache_key(req)
}

/// Sum signed edge quantities (reversal must not double-count).
pub fn signed_edge_sum(nodes: &[TreeNode]) -> Decimal {
    fn walk(n: &TreeNode, acc: &mut Decimal) {
        *acc += n.edge_quantity.amount;
        for c in &n.children {
            walk(c, acc);
        }
    }
    let mut acc = Decimal::ZERO;
    for n in nodes {
        walk(n, &mut acc);
    }
    acc
}
