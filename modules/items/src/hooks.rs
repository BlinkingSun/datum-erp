//! Validation hooks. Items registers none in Wave 2s.1.

use datum_module::KernelBuilder;

/// No-op: the items module does not register hooks against other modules.
pub fn register_hooks(_builder: &mut KernelBuilder) {}
