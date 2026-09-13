//! Kernel lot identifiers (`^[0-9A-Z-]{1,20}$`).

use wicket_db::Tx;

use crate::error::{Error, Result};
use crate::format::{literals, max_len, parse_template, reset_from_template};
use crate::{SequenceId, define, next_number};

/// Single validator every module calls for an externally supplied lot identifier.
///
/// Supplier lots go to a cross-reference; they never become the kernel id.
pub fn validate(id: &str) -> Result<()> {
    if id.is_empty() || id.len() > 20 {
        return Err(Error::InvalidIdentifier(id.to_owned()));
    }
    if !id
        .bytes()
        .all(|b| b.is_ascii_digit() || b.is_ascii_uppercase() || b == b'-')
    {
        return Err(Error::InvalidIdentifier(id.to_owned()));
    }
    Ok(())
}

/// Refuse a template that could exceed 20 characters or emit a non-kernel character.
pub fn validate_template(template: &str) -> Result<()> {
    parse_template(template)?;
    if max_len(template)? > 20 {
        return Err(Error::InvalidTemplate(format!(
            "template {template:?} can exceed 20 characters"
        )));
    }
    let lit = literals(template)?;
    if let Some(bad) = lit
        .chars()
        .find(|c| !(c.is_ascii_digit() || c.is_ascii_uppercase() || *c == '-'))
    {
        return Err(Error::InvalidTemplate(format!(
            "template {template:?} contains {bad:?}"
        )));
    }
    Ok(())
}

/// Allocate a lot identifier from `template` inside the caller's transaction.
pub async fn generate(tx: &mut Tx<'_>, template: &str) -> Result<String> {
    validate_template(template)?;
    let reset = reset_from_template(template)?;
    let doc_type = format!("lot:{template}");
    define(tx, &doc_type, template, reset).await?;
    let id = next_number(tx, SequenceId::new(doc_type, reset)).await?;
    validate(&id)?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lot_id_charset_and_length_enforced() {
        validate("LOT-BAR-24-4412").expect("canonical lot");
        assert!(matches!(
            validate("lot-bar-24-4412"),
            Err(Error::InvalidIdentifier(_))
        ));
        assert!(matches!(
            validate("LOT BAR"),
            Err(Error::InvalidIdentifier(_))
        ));
        assert!(matches!(
            validate("ABCDEFGHIJKLMNOPQRSTU"),
            Err(Error::InvalidIdentifier(_))
        ));
        assert!(matches!(validate(""), Err(Error::InvalidIdentifier(_))));

        validate_template("LOT-BAR-{yy}-{0000}").expect("fits");
        assert!(matches!(
            validate_template("lot-bar-{yy}-{0000}"),
            Err(Error::InvalidTemplate(_))
        ));
        assert!(matches!(
            validate_template("LOT BAR-{0000}"),
            Err(Error::InvalidTemplate(_))
        ));
        assert!(matches!(
            validate_template("ABCDEFGHIJKLMNOPQRSTU"),
            Err(Error::InvalidTemplate(_))
        ));
    }
}
