//! Hooks. Conversion and event emission happen around `Kernel::transition`,
//! never inside a hook (SPEC-common kernel seams). Posting intents are
//! contributed by the caller through one `GroupBuilder` per transaction.

use datum_module::KernelBuilder;

use crate::error::Result;

/// No in-hook conversion or emission. Registration is the extension point.
pub fn register(_builder: &mut KernelBuilder) -> Result<()> {
    Ok(())
}
