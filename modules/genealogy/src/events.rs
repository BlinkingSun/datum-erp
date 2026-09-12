//! Event subscriber. Subscriptions are declared in `module.toml` and registered
//! through `KernelBuilder::apply_manifest` / `Kernel::register_events_subscription`.

use datum_db::Tx;
use datum_events::{Event, EventHandler, HandlerFuture};

use crate::store::invalidate_cache;

/// Idempotent cache invalidator for inventory / production events.
#[derive(Debug, Default, Clone, Copy)]
pub struct CacheInvalidate;

impl EventHandler for CacheInvalidate {
    fn handle<'a, 'p: 'a>(&'a self, tx: &'a mut Tx<'p>, event: &'a Event) -> HandlerFuture<'a> {
        Box::pin(async move {
            let _ = event;
            invalidate_cache(tx)
                .await
                .map_err(|e| datum_events::Error::Invariant(e.to_string()))?;
            Ok(())
        })
    }
}
