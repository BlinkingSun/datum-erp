//! At-least-once delivery, dead letter, skip locked, service principal.

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use sqlx::{query, query_as, query_scalar};
use wicket_core::Identifier;
use wicket_db::Tx;
use wicket_events::{Dispatcher, Error, Event, EventHandler, HandlerFuture, Registry, publish};
use wicket_test::db_case;

use common::{
    delivery_attempts, migrate_and_install, ping_event, service_actor, user_ctx, write_pool,
    write_pool_wide,
};

struct FailOnce {
    hits: Arc<AtomicU32>,
}

impl EventHandler for FailOnce {
    fn handle<'a, 'p: 'a>(&'a self, _tx: &'a mut Tx<'p>, _event: &'a Event) -> HandlerFuture<'a> {
        let hits = self.hits.clone();
        Box::pin(async move {
            let n = hits.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                return Err(Error::Invariant("fail after side effect".into()));
            }
            Ok(())
        })
    }
}

struct AlwaysFail {
    hits: Arc<AtomicU32>,
}

impl EventHandler for AlwaysFail {
    fn handle<'a, 'p: 'a>(&'a self, _tx: &'a mut Tx<'p>, _event: &'a Event) -> HandlerFuture<'a> {
        let hits = self.hits.clone();
        Box::pin(async move {
            hits.fetch_add(1, Ordering::SeqCst);
            Err(Error::Invariant("always".into()))
        })
    }
}

struct ProbeInsert;

impl EventHandler for ProbeInsert {
    fn handle<'a, 'p: 'a>(&'a self, tx: &'a mut Tx<'p>, event: &'a Event) -> HandlerFuture<'a> {
        let event_id = event.id.as_uuid();
        Box::pin(async move {
            tx.execute(query("INSERT INTO app.events_probe (id, n) VALUES ($1, 1)").bind(event_id))
                .await?;
            Ok(())
        })
    }
}

struct SlowCount {
    hits: Arc<AtomicU32>,
}

impl EventHandler for SlowCount {
    fn handle<'a, 'p: 'a>(&'a self, _tx: &'a mut Tx<'p>, _event: &'a Event) -> HandlerFuture<'a> {
        let hits = self.hits.clone();
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(250)).await;
            hits.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }
}

async fn publish_one(pool: &wicket_db::WritePool) -> Identifier {
    let ctx = user_ctx();
    let mut tx = Tx::begin(pool, &ctx).await.expect("begin");
    let event = ping_event().await;
    let id = event.id;
    publish(&mut tx, event).await.expect("publish");
    tx.commit().await.expect("commit");
    id
}

#[tokio::test]
async fn delivery_is_at_least_once() {
    let db = db_case!("evt_at_least");
    migrate_and_install(&db).await;
    let pool = write_pool(&db);
    let hits = Arc::new(AtomicU32::new(0));
    let registry = Registry::new();
    registry.subscribe("test.ping", "probe", FailOnce { hits: hits.clone() });
    let dispatcher = Dispatcher::new(registry)
        .max_attempts(5)
        .backoff_base(Duration::ZERO);

    let id = publish_one(&pool).await;
    dispatcher.tick(&pool, service_actor()).await.expect("tick");

    assert_eq!(
        hits.load(Ordering::SeqCst),
        2,
        "handler side effect runs on the failed attempt and the retry"
    );
    let row = delivery_attempts(db.app_pool(), id, "probe")
        .await
        .expect("delivery row");
    assert_eq!(row.0, 2, "attempts must be 2");
    assert!(row.1.is_some(), "second attempt delivers");

    db.finish().await.expect("finish");
}

#[tokio::test]
async fn handler_runs_as_service_principal() {
    let db = db_case!("evt_svc");
    migrate_and_install(&db).await;
    let pool = write_pool(&db);

    query("CREATE TABLE app.events_probe (id uuid PRIMARY KEY, n int NOT NULL)")
        .execute(db.migrate_pool())
        .await
        .expect("probe table");

    let service = service_actor();
    let service_id = service.id;
    let registry = Registry::new();
    registry.subscribe("test.ping", "probe", ProbeInsert);
    let dispatcher = Dispatcher::new(registry)
        .max_attempts(3)
        .backoff_base(Duration::ZERO);

    let _ = publish_one(&pool).await;
    dispatcher.tick(&pool, service).await.expect("tick");

    let rows: Vec<(uuid::Uuid, String, String)> = query_as(
        r#"
        SELECT actor_id, actor_kind, source_kind
          FROM audit.event
         WHERE table_name = 'events_probe' AND op = 'INSERT'
        "#,
    )
    .fetch_all(db.app_pool())
    .await
    .expect("audit rows");
    assert!(!rows.is_empty(), "handler insert must produce an audit row");
    for (actor_id, actor_kind, source_kind) in &rows {
        assert_eq!(*actor_id, service_id.as_uuid());
        assert_eq!(actor_kind, "service");
        assert_eq!(source_kind, "job");
    }

    db.finish().await.expect("finish");
}

#[tokio::test]
async fn dead_letter_after_n_attempts() {
    let db = db_case!("evt_dead");
    migrate_and_install(&db).await;
    let pool = write_pool(&db);
    let hits = Arc::new(AtomicU32::new(0));
    let registry = Registry::new();
    registry.subscribe("test.ping", "probe", AlwaysFail { hits: hits.clone() });
    let n = 3;
    let dispatcher = Dispatcher::new(registry)
        .max_attempts(n)
        .backoff_base(Duration::ZERO);

    let id = publish_one(&pool).await;
    dispatcher.tick(&pool, service_actor()).await.expect("tick");

    assert_eq!(hits.load(Ordering::SeqCst), n as u32);
    let row = delivery_attempts(db.app_pool(), id, "probe")
        .await
        .expect("delivery row");
    assert_eq!(row.0, n);
    assert!(row.1.is_none(), "dead letter is not delivered");

    let extra = dispatcher
        .tick(&pool, service_actor())
        .await
        .expect("tick after dead letter");
    assert_eq!(extra, 0);
    assert_eq!(hits.load(Ordering::SeqCst), n as u32);

    db.finish().await.expect("finish");
}

#[tokio::test]
async fn skip_locked_allows_two_dispatchers() {
    let db = db_case!("evt_skip");
    migrate_and_install(&db).await;
    let pool = write_pool_wide(&db, 8).await;

    let hits = Arc::new(AtomicU32::new(0));
    let registry = Registry::new();
    registry.subscribe("test.ping", "probe", SlowCount { hits: hits.clone() });
    let dispatcher = Dispatcher::new(registry)
        .max_attempts(3)
        .backoff_base(Duration::ZERO);

    let id = publish_one(&pool).await;
    let a = dispatcher.clone();
    let b = dispatcher.clone();
    let pool_a = pool.clone();
    let pool_b = pool.clone();
    let actor_a = service_actor();
    let actor_b = service_actor();
    let (ra, rb) = tokio::join!(a.tick(&pool_a, actor_a), b.tick(&pool_b, actor_b));
    let na = ra.expect("dispatcher a");
    let nb = rb.expect("dispatcher b");
    assert_eq!(na + nb, 1, "exactly one dispatcher claims the row");
    assert_eq!(hits.load(Ordering::SeqCst), 1, "no double delivery");

    let delivered: Option<chrono::DateTime<chrono::Utc>> = query_scalar(
        "SELECT delivered_at FROM transient.delivery WHERE event_id = $1 AND subscriber = 'probe'",
    )
    .bind(id.as_uuid())
    .fetch_one(db.app_pool())
    .await
    .expect("delivered_at");
    assert!(delivered.is_some());

    db.finish().await.expect("finish");
}
