//! Hooks. None registered by this module; the transition executor is used so
//! later modules (training veto, calibration veto) can hook without changing
//! this crate. Conversion and event emission happen around `Kernel::transition`,
//! never inside a hook.

use datum_module::KernelBuilder;

use crate::error::Result;

/// No in-hook conversion or emission. Registration is the extension point.
pub fn register(_builder: &mut KernelBuilder) -> Result<()> {
    Ok(())
}
