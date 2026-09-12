//! In-process subscriber registry, built by the composition root.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};

use crate::error::Result;
use crate::event::Event;

/// Future returned by an [`EventHandler`].
pub type HandlerFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

/// Subscriber invoked inside the dispatcher's own `Tx::begin`.
///
/// Handlers **must be idempotent**. Delivery is at-least-once: a crash after side
/// effects and before the delivery row is committed causes a re-delivery.
pub trait EventHandler: Send + Sync {
    /// Handle one event inside `tx`.
    fn handle<'a, 'p: 'a>(
        &'a self,
        tx: &'a mut datum_db::Tx<'p>,
        event: &'a Event,
    ) -> HandlerFuture<'a>;
}

type HandlerMap = HashMap<(String, String), Arc<dyn EventHandler>>;

/// In-process map of `(event name, subscriber) -> handler`.
#[derive(Clone, Default)]
pub struct Registry {
    inner: Arc<RwLock<HandlerMap>>,
}

impl Registry {
    /// Empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `handler` for `(name, subscriber)`. Last registration wins.
    pub fn subscribe(
        &self,
        name: impl Into<String>,
        subscriber: impl Into<String>,
        handler: impl EventHandler + 'static,
    ) {
        let mut guard = match self.inner.write() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.insert((name.into(), subscriber.into()), Arc::new(handler));
    }

    pub(crate) fn handler(&self, name: &str, subscriber: &str) -> Option<Arc<dyn EventHandler>> {
        let guard = match self.inner.read() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard
            .get(&(name.to_string(), subscriber.to_string()))
            .cloned()
    }

    pub(crate) fn entries(&self) -> Vec<(String, String)> {
        let guard = match self.inner.read() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.keys().cloned().collect()
    }
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let n = self.entries().len();
        f.debug_struct("Registry")
            .field("subscriptions", &n)
            .finish()
    }
}

/// Persist an enabled subscription row inside `tx`.
pub async fn enable_subscription(
    tx: &mut datum_db::Tx<'_>,
    name: &str,
    subscriber: &str,
) -> Result<()> {
    use crate::sql::query;
    tx.execute(
        query(
            r#"
            INSERT INTO app.subscription (subscriber, name, enabled)
            VALUES ($1, $2, true)
            ON CONFLICT (subscriber, name) DO NOTHING
            "#,
        )
        .bind(subscriber)
        .bind(name),
    )
    .await?;
    Ok(())
}
