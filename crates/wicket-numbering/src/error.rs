//! Crate error type.

/// Crate result alias.
pub type Result<T> = core::result::Result<T, Error>;

/// Failures from numbering, lot, and serial allocation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Not implemented.
    #[error("unimplemented")]
    Unimplemented,
    /// Core error.
    #[error(transparent)]
    Core(#[from] wicket_core::Error),
    /// Database error.
    #[error(transparent)]
    Db(wicket_db::Error),
    /// `{0000}`-style padding overflowed; the value is never truncated.
    #[error("padding overflow allocated={allocated} width={width}")]
    PaddingOverflow {
        /// Value that did not fit.
        allocated: i64,
        /// Number of zeros in the token.
        width: usize,
    },
    /// Format or lot/serial template is illegal.
    #[error("invalid template: {0}")]
    InvalidTemplate(String),
    /// Identifier failed `^[0-9A-Z-]{{1,20}}$`.
    #[error("invalid identifier: {0}")]
    InvalidIdentifier(String),
    /// `next_number` on a `doc_type` that was never [`super::define`]d.
    #[error("unknown sequence: {0}")]
    UnknownSequence(String),
    /// `reset_policy` was not `never`, `yearly`, or `monthly`.
    #[error("invalid reset policy: {0}")]
    InvalidResetPolicy(String),
    /// Failed to encode a bind argument.
    #[error("bind: {0}")]
    Bind(String),
}

impl From<wicket_db::Error> for Error {
    fn from(err: wicket_db::Error) -> Self {
        map_db(err)
    }
}

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        Error::from(wicket_db::Error::from(err))
    }
}

fn map_db(err: wicket_db::Error) -> Error {
    if let wicket_db::Error::Sqlx(sqlx_err) = &err {
        let msg = sqlx_err.to_string();
        if let Some(ov) = parse_overflow(&msg) {
            return Error::PaddingOverflow {
                allocated: ov.0,
                width: ov.1,
            };
        }
        if let Some(rest) = msg_after(&msg, "numbering: unknown sequence ") {
            return Error::UnknownSequence(rest);
        }
        if let Some(rest) = msg_after(&msg, "numbering: invalid reset_policy ") {
            return Error::InvalidResetPolicy(rest);
        }
        if msg.contains("numbering: invalid template")
            || msg.contains("numbering: empty format")
            || msg.contains("numbering: leftover token")
            || msg.contains("already defined with a different format")
        {
            return Error::InvalidTemplate(msg);
        }
        if msg.contains("numbering: empty doc_type") {
            return Error::UnknownSequence(msg);
        }
    }
    Error::Db(err)
}

fn parse_overflow(msg: &str) -> Option<(i64, usize)> {
    let idx = msg.find("numbering: padding overflow allocated=")?;
    let rest = &msg[idx + "numbering: padding overflow allocated=".len()..];
    let mut parts = rest.split(" width=");
    let allocated = parts
        .next()?
        .split(|c: char| !c.is_ascii_digit() && c != '-')
        .next()?
        .parse()
        .ok()?;
    let width = parts
        .next()?
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;
    Some((allocated, width))
}

fn msg_after(msg: &str, prefix: &str) -> Option<String> {
    let idx = msg.find(prefix)?;
    let rest = &msg[idx + prefix.len()..];
    let token: String = rest
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '"')
        .collect();
    if token.is_empty() { None } else { Some(token) }
}
