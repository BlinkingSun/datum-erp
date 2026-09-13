//! Hooks. This module does not register in-transaction hooks in Wave 2s.1:
//! inventory posts quarantine/available movements; lots only records status.
//!
//! Registration, when needed, goes through [`wicket_module::KernelBuilder::register_hook`].

use wicket_module::KernelBuilder;

use crate::error::Result;

/// No hooks in this slice. Kept so the module anatomy matches `docs/03` §2.
pub fn register(_builder: &mut KernelBuilder) -> Result<()> {
    Ok(())
}
