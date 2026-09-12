//! Commit-mode job queue tests (SPEC).

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use datum_core::Identifier;
use datum_db::Tx;
use datum_events::{Dispatcher, Event, SchemaRegistry, publish};
use datum_jobs::{
    EnqueueOptions, HandlerOutcome, JobHandler, JobState, Progress, Registry, Worker, cancel,
    enqueue, status,
};
use datum_test::db_case;
use serde_json::{Value, json};
use sqlx::{query, query_scalar};

use common::{migrate_and_install, pg_code, service_actor, user_ctx, write_pool, write_pool_wide};

struct OkHandler;

impl JobHandler for OkHandler {
    fn run(
        &self,
        payload: &Value,
        _progress: Progress,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = datum_jobs::Result<HandlerOutcome>> + Send + '_>,
    > {
        let v = payload.clone();
        Box::pin(async move { Ok(HandlerOutcome::Done(v)) })
    }
}

struct FailHandler;

impl JobHandler for FailHandler {
    fn run(
        &self,
        _payload: &Value,
        _progress: Progress,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = datum_jobs::Result<HandlerOutcome>> + Send + '_>,
    > {
        Box::pin(async move { Err(datum_jobs::Error::Invariant("boom".into())) })
    }
}

struct ComputeThreads {
    max_threads: Arc<AtomicU32>,
}

impl JobHandler for ComputeThreads {
    fn run(
        &self,
        _payload: &Value,
        _progress: Progress,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = datum_jobs::Result<HandlerOutcome>> + Send + '_>,
    > {
        let max_threads = self.max_threads.clone();
        Box::pin(async move {
            let work = Box::new(move |prog: Progress| {
                let n = std::thread::available_parallelism()
                    .map(|p| p.get())
                    .unwrap_or(2)
                    .min(8);
                let active = Arc::new(AtomicU32::new(0));
                std::thread::scope(|s| {
                    for i in 0..n {
                        s.spawn(|| {
                            let cur = active.fetch_add(1, Ordering::SeqCst) + 1;
                            let prev = max_threads.load(Ordering::SeqCst);
                            if cur > prev {
                                max_threads.store(cur, Ordering::SeqCst);
                            }
                            std::thread::sleep(Duration::from_millis(80));
                            active.fetch_sub(1, Ordering::SeqCst);
                            let _ = i;
                        });
                    }
                });
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("rt");
                rt.block_on(prog.report(50, "half"))?;
                rt.block_on(prog.report(100, "done"))?;
                Ok(json!({ "threads": max_threads.load(Ordering::SeqCst) }))
            });
            Ok(HandlerOutcome::Compute(work))
        })
    }
}

#[tokio::test]
async fn job_invisible_until_enqueuing_tx_commits() {
    let db = db_case!("job_invisible");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let ctx = user_ctx();
    let actor = ctx.actor.id;

    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    let job_id = enqueue(
        &mut tx,
        "test.echo",
        json!({ "n": 1 }),
        actor,
        EnqueueOptions::default(),
    )
    .await
    .expect("enqueue");
    assert!(
        status(db.app_pool(), job_id)
            .await
            .expect("status")
            .is_none(),
        "uncommitted job must not be visible to readers"
    );
    tx.commit().await.expect("commit");
    let st = status(db.app_pool(), job_id)
        .await
        .expect("status")
        .expect("row");
    assert_eq!(st.state, JobState::Queued);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn worker_runs_as_service_principal_with_job_source_kind() {
    let db = db_case!("job_audit");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let ctx = user_ctx();
    let actor = service_actor();
    let reg = Registry::new();
    reg.register("test.echo", OkHandler);

    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    let job_id = enqueue(
        &mut tx,
        "test.echo",
        json!({ "ok": true }),
        ctx.actor.id,
        EnqueueOptions::default(),
    )
    .await
    .expect("enqueue");
    tx.commit().await.expect("commit");

    let worker = Worker::new(reg).idle(Duration::from_millis(1));
    worker.tick(&write, actor).await.expect("tick");

    let rows: Vec<(String, String, String)> = sqlx::query_as(
        r#"
        SELECT source_kind, action, table_name
          FROM audit.event
         WHERE table_name = 'run_log'
         ORDER BY stmt_at
        "#,
    )
    .fetch_all(db.app_pool())
    .await
    .expect("audit");
    assert!(
        rows.iter()
            .any(|(sk, act, tbl)| { sk == "job" && act == "job.test.echo" && tbl == "run_log" }),
        "expected audited run_log write as job.test.echo: {rows:?}"
    );

    let st = status(db.app_pool(), job_id)
        .await
        .expect("status")
        .expect("job");
    assert_eq!(st.state, JobState::Succeeded);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn retry_then_failed_after_max_attempts() {
    let db = db_case!("job_retry");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let ctx = user_ctx();
    let actor = service_actor();
    let reg = Registry::new();
    reg.register("test.fail", FailHandler);
    let worker = Worker::new(reg)
        .backoff_base(Duration::ZERO)
        .idle(Duration::from_millis(1));

    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    let job_id = enqueue(
        &mut tx,
        "test.fail",
        json!({}),
        ctx.actor.id,
        EnqueueOptions {
            max_attempts: 2,
            ..Default::default()
        },
    )
    .await
    .expect("enqueue");
    tx.commit().await.expect("commit");

    worker.tick(&write, actor).await.expect("tick1");
    worker.tick(&write, actor).await.expect("tick2");

    let st = status(db.app_pool(), job_id)
        .await
        .expect("status")
        .expect("job");
    assert_eq!(st.state, JobState::Failed);
    assert!(st.last_error.as_ref().is_some_and(|e| e.contains("boom")));
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn crashed_worker_job_is_reclaimed() {
    let db = db_case!("job_reclaim");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let ctx = user_ctx();
    let actor = service_actor();

    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    let job_id = enqueue(
        &mut tx,
        "test.echo",
        json!({}),
        ctx.actor.id,
        EnqueueOptions::default(),
    )
    .await
    .expect("enqueue");
    tx.commit().await.expect("commit");

    query(
        r#"
        UPDATE transient.job
           SET state = 'running',
               locked_at = now() - interval '1 hour',
               locked_by = 'stale'
         WHERE id = $1
        "#,
    )
    .bind(job_id.id().as_uuid())
    .execute(db.migrate_pool())
    .await
    .expect("stale");

    let reg = Registry::new();
    reg.register("test.echo", OkHandler);
    Worker::new(reg)
        .stale_lock(Duration::from_secs(1))
        .idle(Duration::from_millis(1))
        .tick(&write, actor)
        .await
        .expect("reclaim tick");

    let st = status(db.app_pool(), job_id)
        .await
        .expect("status")
        .expect("job");
    assert_eq!(st.state, JobState::Succeeded);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn skip_locked_two_workers_no_double_run() {
    let db = db_case!("job_skip");
    migrate_and_install(&db).await;
    let write = write_pool_wide(&db, 4).await;
    let ctx = user_ctx();
    let actor = service_actor();
    let hits = Arc::new(AtomicU32::new(0));

    struct Count {
        hits: Arc<AtomicU32>,
    }
    impl JobHandler for Count {
        fn run(
            &self,
            _payload: &Value,
            _progress: Progress,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = datum_jobs::Result<HandlerOutcome>> + Send + '_>,
        > {
            let hits = self.hits.clone();
            Box::pin(async move {
                hits.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok(HandlerOutcome::Done(json!({})))
            })
        }
    }

    let reg = Registry::new();
    reg.register("test.count", Count { hits: hits.clone() });

    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    enqueue(
        &mut tx,
        "test.count",
        json!({}),
        ctx.actor.id,
        EnqueueOptions::default(),
    )
    .await
    .expect("enqueue");
    tx.commit().await.expect("commit");

    let reg = reg;
    let w1 = Worker::new(reg.clone()).idle(Duration::from_millis(5));
    let w2 = Worker::new(reg).idle(Duration::from_millis(5));
    let a = tokio::spawn({
        let write = write.clone();
        async move { w1.tick(&write, actor).await }
    });
    let b = tokio::spawn({
        let write = write.clone();
        async move { w2.tick(&write, actor).await }
    });
    let _ = tokio::join!(a, b);
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn compute_job_uses_multiple_threads_and_reports_progress() {
    let db = db_case!("job_compute");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let ctx = user_ctx();
    let actor = service_actor();
    let max_threads = Arc::new(AtomicU32::new(0));
    let reg = Registry::new();
    reg.register(
        "test.compute",
        ComputeThreads {
            max_threads: max_threads.clone(),
        },
    );

    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    let job_id = enqueue(
        &mut tx,
        "test.compute",
        json!({}),
        ctx.actor.id,
        EnqueueOptions::default(),
    )
    .await
    .expect("enqueue");
    tx.commit().await.expect("commit");

    let status_task = tokio::spawn({
        let pool = db.app_pool().clone();
        async move {
            for _ in 0..20 {
                if let Ok(Some(st)) = status(&pool, job_id).await
                    && st.progress_pct >= 50
                {
                    return st.progress_pct;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            0
        }
    });

    Worker::new(reg)
        .idle(Duration::from_millis(1))
        .tick(&write, actor)
        .await
        .expect("compute");

    let mid = status_task.await.expect("status task");
    assert!(mid >= 50, "progress should advance during compute");
    assert!(
        max_threads.load(Ordering::SeqCst) >= 2,
        "expected parallel threads"
    );
    let st = status(db.app_pool(), job_id)
        .await
        .expect("status")
        .expect("job");
    assert_eq!(st.progress_pct, 100);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn cancel_before_start_never_runs() {
    let db = db_case!("job_cancel");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let ctx = user_ctx();
    let actor = service_actor();
    let reg = Registry::new();
    reg.register("test.echo", OkHandler);

    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    let job_id = enqueue(
        &mut tx,
        "test.echo",
        json!({}),
        ctx.actor.id,
        EnqueueOptions::default(),
    )
    .await
    .expect("enqueue");
    cancel(&mut tx, job_id).await.expect("cancel");
    tx.commit().await.expect("commit");

    Worker::new(reg)
        .idle(Duration::from_millis(1))
        .tick(&write, actor)
        .await
        .expect("tick");

    let st = status(db.app_pool(), job_id)
        .await
        .expect("status")
        .expect("job");
    assert_eq!(st.state, JobState::Cancelled);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn run_log_is_append_only_and_audited() {
    let db = db_case!("job_runlog");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let ctx = user_ctx();
    let actor = service_actor();
    let reg = Registry::new();
    reg.register("test.echo", OkHandler);

    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    enqueue(
        &mut tx,
        "test.echo",
        json!({}),
        ctx.actor.id,
        EnqueueOptions::default(),
    )
    .await
    .expect("enqueue");
    tx.commit().await.expect("commit");
    Worker::new(reg)
        .idle(Duration::from_millis(1))
        .tick(&write, actor)
        .await
        .expect("tick");

    let audited: bool = query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM pg_trigger t
             WHERE t.tgrelid = 'app.run_log'::regclass
               AND t.tgname LIKE 'zz_audit%'
         )",
    )
    .fetch_one(db.app_pool())
    .await
    .expect("trigger");
    assert!(audited);

    let err = query("DELETE FROM app.run_log")
        .execute(db.app_pool())
        .await
        .expect_err("no delete");
    assert_eq!(pg_code(&err), "42501");
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn event_bridge_enqueues_job() {
    let db = db_case!("job_bridge");
    migrate_and_install(&db).await;
    let write = write_pool(&db);
    let ctx = user_ctx();
    let registry = SchemaRegistry::standard();
    let events_reg = datum_events::Registry::new();

    let mut tx = Tx::begin(&write, &ctx).await.expect("begin");
    datum_jobs::events::enable_genealogy_bridge(&mut tx, &events_reg)
        .await
        .expect("bridge");
    let event = Event::builder()
        .name("inventory.lot_received")
        .version(1)
        .payload(json!({
            "item_id": Identifier::generate().as_uuid().to_string(),
            "lot_id": Identifier::generate().as_uuid().to_string(),
            "quantity": "1 ea"
        }))
        .build_with(&registry)
        .expect("event");
    publish(&mut tx, event).await.expect("publish");
    tx.commit().await.expect("commit");

    Dispatcher::new(events_reg)
        .idle(Duration::from_millis(1))
        .tick(&write, service_actor())
        .await
        .expect("dispatch bridge");

    let n: i64 = query_scalar("SELECT count(*) FROM transient.job WHERE kind = $1")
        .bind(datum_jobs::events::bridge_job_kind())
        .fetch_one(db.app_pool())
        .await
        .expect("count");
    assert_eq!(n, 1);
    db.finish().await.expect("finish");
}

#[tokio::test]
async fn writes_go_through_tx() {
    let db = db_case!("job_tx_fence");
    migrate_and_install(&db).await;
    let before: i64 = query_scalar("SELECT count(*) FROM app.run_log")
        .fetch_one(db.app_pool())
        .await
        .expect("count");
    let err = query(
        r#"
        INSERT INTO app.run_log (id, job_id, attempt, started_at)
        VALUES (gen_random_uuid(), gen_random_uuid(), 1, now())
        "#,
    )
    .execute(db.app_pool())
    .await
    .expect_err("raw write");
    assert_eq!(pg_code(&err), "42501");
    let after: i64 = query_scalar("SELECT count(*) FROM app.run_log")
        .fetch_one(db.app_pool())
        .await
        .expect("count");
    assert_eq!(before, after);
    db.finish().await.expect("finish");
}
