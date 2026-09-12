//! Job handler registry and compute outcomes.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};

use serde_json::Value;

use crate::error::Result;
use crate::progress::Progress;

/// Result of synchronous handler work, or a compute bundle for the worker thread pool.
pub enum HandlerOutcome {
    /// Finished inside the handler call.
    Done(Value),
    /// CPU work to run off the async runtime; DB is opened only for the result write.
    Compute(ComputeWork),
}

/// Blocking compute closure.
pub type ComputeWork = Box<dyn FnOnce(Progress) -> Result<Value> + Send>;

/// Handler invoked by the worker after a claim.
pub trait JobHandler: Send + Sync {
    /// Run the job. May return [`HandlerOutcome::Compute`].
    fn run(
        &self,
        payload: &Value,
        progress: Progress,
    ) -> Pin<Box<dyn Future<Output = Result<HandlerOutcome>> + Send + '_>>;
}

type HandlerMap = HashMap<String, Arc<dyn JobHandler>>;

/// In-process map of job kind → handler.
#[derive(Clone, Default)]
pub struct Registry {
    inner: Arc<RwLock<HandlerMap>>,
}

impl Registry {
    /// Empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `handler` for `kind`. Last registration wins.
    pub fn register(&self, kind: impl Into<String>, handler: impl JobHandler + 'static) {
        let mut guard = match self.inner.write() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.insert(kind.into(), Arc::new(handler));
    }

    pub(crate) fn get(&self, kind: &str) -> Option<Arc<dyn JobHandler>> {
        let guard = match self.inner.read() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.get(kind).cloned()
    }
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let n = match self.inner.read() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        };
        f.debug_struct("Registry").field("kinds", &n).finish()
    }
}
