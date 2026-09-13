//! Kernel serial identifiers (`^[0-9A-Z-]{1,20}$`).

use wicket_db::Tx;

use crate::error::Result;
use crate::format::reset_from_template;
use crate::lot;
use crate::{SequenceId, define, next_number};

/// Validator for an externally supplied serial identifier.
pub fn validate(id: &str) -> Result<()> {
    lot::validate(id)
}

/// Refuse a template that could exceed 20 characters or emit a non-kernel character.
pub fn validate_template(template: &str) -> Result<()> {
    lot::validate_template(template)
}

/// Allocate a serial identifier from `template` inside the caller's transaction.
pub async fn generate(tx: &mut Tx<'_>, template: &str) -> Result<String> {
    validate_template(template)?;
    let reset = reset_from_template(template)?;
    let doc_type = format!("serial:{template}");
    define(tx, &doc_type, template, reset).await?;
    let id = next_number(tx, SequenceId::new(doc_type, reset)).await?;
    validate(&id)?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    #[test]
    fn serial_reuses_lot_charset() {
        validate("SN-450-000134").expect("canonical serial");
        assert!(matches!(
            validate("sn-450-000134"),
            Err(Error::InvalidIdentifier(_))
        ));
    }
}
