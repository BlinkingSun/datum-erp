//! Bridge from [`datum_events`] subscribers into the job queue.

use datum_db::Tx;
use datum_events::{Event, EventHandler, HandlerFuture, Result as EventResult};
use serde_json::json;

use crate::queue::{EnqueueOptions, enqueue};

const SUBSCRIBER: &str = "datum-jobs";
const LOT_RECEIVED: &str = "inventory.lot_received";
const GENEALOGY_REFRESH: &str = "genealogy.refresh";

/// Subscriber that enqueues `genealogy.refresh` when `inventory.lot_received` fires.
#[derive(Debug, Default, Clone, Copy)]
pub struct GenealogyRefreshBridge;

impl EventHandler for GenealogyRefreshBridge {
    fn handle<'a, 'p: 'a>(&'a self, tx: &'a mut Tx<'p>, event: &'a Event) -> HandlerFuture<'a> {
        Box::pin(async move {
            if event.name != LOT_RECEIVED {
                return Ok(());
            }
            let payload = json!({
                "source_event_id": event.id.as_uuid().to_string(),
                "payload": event.payload,
            });
            enqueue(
                tx,
                GENEALOGY_REFRESH,
                payload,
                event.actor_id,
                EnqueueOptions::default(),
            )
            .await
            .map_err(|e| datum_events::Error::Invariant(e.to_string()))?;
            Ok(())
        })
    }
}

/// Register the bridge with an events [`datum_events::Registry`] and enable the subscription row.
pub async fn enable_genealogy_bridge(
    tx: &mut Tx<'_>,
    registry: &datum_events::Registry,
) -> EventResult<()> {
    registry.subscribe(LOT_RECEIVED, SUBSCRIBER, GenealogyRefreshBridge);
    datum_events::enable_subscription(tx, LOT_RECEIVED, SUBSCRIBER).await?;
    Ok(())
}

/// Expose subscriber id for tests.
pub fn bridge_subscriber() -> &'static str {
    SUBSCRIBER
}

/// Target job kind for the bridge.
pub fn bridge_job_kind() -> &'static str {
    GENEALOGY_REFRESH
}
