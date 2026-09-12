//! Hooks. Genealogy is read-only: it never vetoes a transition.

use datum_module::KernelBuilder;

use crate::error::Result;

/// No in-hook conversion or emission. Registration is the extension point.
pub fn register(_builder: &mut KernelBuilder) -> Result<()> {
    Ok(())
}
