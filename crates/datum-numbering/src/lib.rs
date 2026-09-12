//! Gap-free document numbering and kernel lot/serial identifiers.
//!
//! Document numbers are allocated from `numbering.counter` by
//! `UPDATE … RETURNING` inside the caller's [`datum_db::Tx`] (D3 §8). There is
//! no `void_number`: cancellation is the document's status (`Void`) with a
//! reason and a full audit trail. A committed number is never reused.
//!
//! Period keys (`''`, year, year-month) come from [`ResetPolicy`] applied to
//! the transaction's server `now()`, never the client clock (invariant 4).
//! Lot and serial identifiers are kernel (invariant 9): `^[0-9A-Z-]{1,20}$`.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod error;
mod format;

pub mod lot;
pub mod serial;

use datum_db::Tx;
use serde::{Deserialize, Serialize};
use sqlx::Arguments as _;
use sqlx::postgres::PgArguments;

pub use error::{Error, Result};

/// Embedded migrator (`placeholder` + `0001_numbering`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

const SQL_DEFINE: &str = "SELECT numbering.define($1, $2, $3)";
const SQL_NEXT: &str = "SELECT numbering.next_number($1, $2)";
const SQL_NOW: &str = "SELECT numbering.server_now()";

/// How [`next_number`] derives `period_key`. Never rewinds a counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ResetPolicy {
    /// `period_key` is always `''`.
    Never,
    /// `period_key` is the UTC year (`YYYY`).
    Yearly,
    /// `period_key` is the UTC year-month (`YYYY-MM`).
    Monthly,
}

impl ResetPolicy {
    fn as_str(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::Yearly => "yearly",
            Self::Monthly => "monthly",
        }
    }
}

/// Resolves `(doc_type, period_key)`: `doc_type` is stored; `period_key` is
/// derived at allocation from [`ResetPolicy`] and server `now()`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SequenceId {
    doc_type: String,
    reset: ResetPolicy,
}

impl SequenceId {
    /// Construct a sequence handle. Prefer [`define`], which also inserts the row.
    pub fn new(doc_type: impl Into<String>, reset: ResetPolicy) -> Self {
        Self {
            doc_type: doc_type.into(),
            reset,
        }
    }

    /// Document type key.
    pub fn doc_type(&self) -> &str {
        &self.doc_type
    }

    /// Reset policy used for `period_key` derivation.
    pub fn reset_policy(&self) -> ResetPolicy {
        self.reset
    }
}

/// Create the counter for `doc_type`. Does not allocate a number and never
/// rewinds `next_value` if the period row already exists.
pub async fn define(
    tx: &mut Tx<'_>,
    doc_type: impl AsRef<str>,
    format: impl AsRef<str>,
    reset_policy: ResetPolicy,
) -> Result<SequenceId> {
    let doc_type = doc_type.as_ref();
    let format = format.as_ref();
    if doc_type.is_empty() {
        return Err(Error::UnknownSequence(doc_type.to_owned()));
    }
    format::parse_template(format)?;
    let _ = format::render(format, 1, 2000, 1)?;
    let _ = exec_out(tx, SQL_DEFINE, &[doc_type, format, reset_policy.as_str()]).await?;
    Ok(SequenceId::new(doc_type, reset_policy))
}

/// Allocate the next formatted number in `tx` (D3 §8).
pub async fn next_number(tx: &mut Tx<'_>, sequence: SequenceId) -> Result<String> {
    exec_out(
        tx,
        SQL_NEXT,
        &[sequence.doc_type(), sequence.reset_policy().as_str()],
    )
    .await
}

/// Transaction-local server `now()` (UTC text). Period keys use this, not the client clock.
pub async fn server_now(tx: &mut Tx<'_>) -> Result<String> {
    exec_out(tx, SQL_NOW, &[]).await
}

async fn exec_out(tx: &mut Tx<'_>, sql: &'static str, args: &[&str]) -> Result<String> {
    let mut bind = PgArguments::default();
    for value in args {
        bind.add(*value).map_err(|e| Error::Bind(e.to_string()))?;
    }
    tx.execute((sql, Some(bind))).await?;
    let out = tx.setting("numbering.out").await?;
    if out.is_empty() {
        return Err(Error::Bind("numbering.out was empty".into()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use datum_audit as _;
    use proptest::prelude::*;
    use tokio as _;

    #[test]
    fn unimplemented_formats() {
        assert!(!Error::Unimplemented.to_string().is_empty());
    }

    #[test]
    fn migrator_has_placeholder() {
        assert!(!MIGRATOR.migrations.is_empty());
        assert!(
            MIGRATOR
                .iter()
                .any(|m| m.description.contains("numbering") || m.version == 1)
        );
    }

    #[test]
    fn postgres_helper_is_callable() {
        let _ = datum_test::postgres_available();
    }

    #[test]
    fn reset_policy_labels_are_stable() {
        assert_eq!(ResetPolicy::Never.as_str(), "never");
        assert_eq!(ResetPolicy::Yearly.as_str(), "yearly");
        assert_eq!(ResetPolicy::Monthly.as_str(), "monthly");
    }

    proptest! {
        #[test]
        fn unimplemented_display_is_stable(_x in 0u8..4) {
            prop_assert!(!Error::Unimplemented.to_string().is_empty());
        }
    }
}
