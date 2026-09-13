//! Registered record projections: modules declare what is signed.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

type ProjectionFn = fn(&Value) -> Value;

fn registry() -> &'static Mutex<BTreeMap<String, ProjectionFn>> {
    static REG: OnceLock<Mutex<BTreeMap<String, ProjectionFn>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// Identity projection: the whole JSON value is signed. Default for tests.
pub fn identity_projection(value: &Value) -> Value {
    value.clone()
}

/// Register a projection for `doc_type`. Replaces a previous registration.
pub fn register_projection(doc_type: &str, project: fn(&Value) -> Value) {
    let mut guard = match registry().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    guard.insert(doc_type.to_owned(), project);
}

/// Apply the registered projection, or [`identity_projection`] if none is registered.
pub fn project(doc_type: &str, value: &Value) -> Value {
    let guard = match registry().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    match guard.get(doc_type) {
        Some(f) => f(value),
        None => identity_projection(value),
    }
}
