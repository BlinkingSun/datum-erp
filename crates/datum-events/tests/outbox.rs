//! Publish visibility: uncommitted and rolled-back rows are invisible.

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use datum_db::Tx;
use datum_events::{Dispatcher, Event, EventHandler, HandlerFuture, Registry, publish};
use datum_test::db_case;

use common::{count_events, migrate_and_install, ping_event, service_actor, user_ctx, write_pool};

struct Counting {
    hits: Arc<AtomicU32>,
}

impl EventHandler for Counting {
    fn handle<'a, 'p: 'a>(&'a self, _tx: &'a mut Tx<'p>, _event: &'a Event) -> HandlerFuture<'a> {
        let hits = self.hits.clone();
        Box::pin(async move {
            hits.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }
}

#[tokio::test]
async fn event_is_invisible_until_commit() {
    let db = db_case!("evt_invisible");
    migrate_and_install(&db).await;
    let pool = write_pool(&db);
    let hits = Arc::new(AtomicU32::new(0));
    let registry = Registry::new();
    registry.subscribe("test.ping", "probe", Counting { hits: hits.clone() });
    let dispatcher = Dispatcher::new(registry)
        .max_attempts(3)
        .backoff_base(Duration::ZERO);

    let ctx = user_ctx();
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let event = ping_event().await;
    publish(&mut tx, event).await.expect("publish");

    let n = count_events(db.app_pool()).await;
    assert_eq!(n, 0, "uncommitted publish must be invisible");
    let attempted = dispatcher
        .tick(&pool, service_actor())
        .await
        .expect("tick before commit");
    assert_eq!(attempted, 0);
    assert_eq!(hits.load(Ordering::SeqCst), 0);

    tx.commit().await.expect("commit");
    let n = count_events(db.app_pool()).await;
    assert_eq!(n, 1, "committed publish must be visible");
    let attempted = dispatcher
        .tick(&pool, service_actor())
        .await
        .expect("tick after commit");
    assert_eq!(attempted, 1);
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    db.finish().await.expect("finish");
}

#[tokio::test]
async fn rolled_back_transaction_publishes_nothing() {
    let db = db_case!("evt_rollback");
    migrate_and_install(&db).await;
    let pool = write_pool(&db);
    let hits = Arc::new(AtomicU32::new(0));
    let registry = Registry::new();
    registry.subscribe("test.ping", "probe", Counting { hits: hits.clone() });
    let dispatcher = Dispatcher::new(registry)
        .max_attempts(3)
        .backoff_base(Duration::ZERO);

    let ctx = user_ctx();
    let mut tx = Tx::begin(&pool, &ctx).await.expect("begin");
    let event = ping_event().await;
    publish(&mut tx, event).await.expect("publish");
    tx.rollback().await.expect("rollback");

    let n = count_events(db.app_pool()).await;
    assert_eq!(n, 0);
    let attempted = dispatcher.tick(&pool, service_actor()).await.expect("tick");
    assert_eq!(attempted, 0);
    assert_eq!(hits.load(Ordering::SeqCst), 0);

    db.finish().await.expect("finish");
}
