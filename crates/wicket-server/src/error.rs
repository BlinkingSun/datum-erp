//! Crate error type.

use axum::http::StatusCode;
use wicket_core::SignatureError;

/// Crate result alias.
pub type Result<T> = core::result::Result<T, Error>;

/// Failures from boot, HTTP, identity, and module calls.
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
    Db(#[from] wicket_db::Error),
    /// Audit error.
    #[error(transparent)]
    Audit(#[from] wicket_audit::Error),
    /// Identity error.
    #[error(transparent)]
    Identity(#[from] wicket_identity::Error),
    /// Composition root.
    #[error(transparent)]
    Module(#[from] wicket_module::Error),
    /// Items module.
    #[error(transparent)]
    Items(#[from] wicket_mod_items::Error),
    /// Locations module.
    #[error(transparent)]
    Locations(#[from] wicket_mod_locations::Error),
    /// Lots module.
    #[error(transparent)]
    Lots(#[from] wicket_mod_lots::Error),
    /// Inventory module.
    #[error(transparent)]
    Inventory(#[from] wicket_mod_inventory::Error),
    /// Production module.
    #[error(transparent)]
    Production(#[from] wicket_mod_production_min::Error),
    /// Genealogy module.
    #[error(transparent)]
    Genealogy(#[from] wicket_mod_genealogy::Error),
    /// Custom fields.
    #[error(transparent)]
    Customfields(#[from] wicket_customfields::Error),
    /// Controlled documents.
    #[error(transparent)]
    Documents(#[from] wicket_documents::Error),
    /// Print / render.
    #[error(transparent)]
    Print(#[from] wicket_print::Error),
    /// Ledger error.
    #[error(transparent)]
    Ledger(#[from] wicket_ledger::Error),
    /// State machine.
    #[error(transparent)]
    Statemachine(#[from] wicket_statemachine::Error),
    /// JSON.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Configuration / CLI.
    #[error("{0}")]
    Config(String),
    /// HTTP envelope error with a docs/10 code.
    #[error("{message}")]
    Http {
        /// Machine-stable token.
        code: &'static str,
        /// Human message.
        message: String,
        /// Field path.
        field: Option<String>,
        /// HTTP status.
        status: StatusCode,
    },
}

impl Error {
    /// Build a docs/10 envelope error.
    pub fn http(
        code: &'static str,
        message: impl Into<String>,
        field: Option<&str>,
        status: StatusCode,
    ) -> Self {
        Self::Http {
            code,
            message: message.into(),
            field: field.map(str::to_owned),
            status,
        }
    }

    /// 400 VALIDATION.
    pub fn validation(message: impl Into<String>, field: Option<&str>) -> Self {
        Self::http("VALIDATION", message, field, StatusCode::BAD_REQUEST)
    }

    /// 401 UNAUTHENTICATED.
    pub fn unauthenticated(message: impl Into<String>) -> Self {
        Self::http("UNAUTHENTICATED", message, None, StatusCode::UNAUTHORIZED)
    }

    /// 403 FORBIDDEN.
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::http("FORBIDDEN", message, None, StatusCode::FORBIDDEN)
    }

    /// 404 NOT_FOUND.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::http("NOT_FOUND", message, None, StatusCode::NOT_FOUND)
    }

    /// 409 CONFLICT.
    pub fn conflict(message: impl Into<String>, field: Option<&str>) -> Self {
        Self::http("CONFLICT", message, field, StatusCode::CONFLICT)
    }

    fn from_signature(err: &SignatureError) -> Option<(&'static str, StatusCode, String)> {
        match err {
            SignatureError::NoProvider => Some((
                "SIGNATURE_NO_PROVIDER",
                StatusCode::CONFLICT,
                "This transition requires a signature and no signature provider is bound.".into(),
            )),
            SignatureError::Consumed | SignatureError::HashMismatch => {
                Some(("CONFLICT", StatusCode::CONFLICT, err.to_string()))
            }
            SignatureError::Invalid(msg) if msg == "missing token" => Some((
                "SIGNATURE_REQUIRED",
                StatusCode::UNAUTHORIZED,
                err.to_string(),
            )),
            SignatureError::SignerNotPermitted => {
                Some(("SIGNATURE_REQUIRED", StatusCode::FORBIDDEN, err.to_string()))
            }
            // D-2b-5 Invalid (dummy / expired / no such signature / signer mismatch)
            // and MeaningMismatch / RecordMismatch: esign is bound, so this is
            // not 409 SIGNATURE_NO_PROVIDER.
            _ => Some(("SIGNATURE_REQUIRED", StatusCode::FORBIDDEN, err.to_string())),
        }
    }

    fn from_sm(
        err: &wicket_statemachine::Error,
    ) -> Option<(&'static str, StatusCode, Option<&'static str>, String)> {
        match err {
            wicket_statemachine::Error::Signature(sig) => {
                let (c, s, m) = Self::from_signature(sig)?;
                Some((c, s, None, m))
            }
            wicket_statemachine::Error::PermissionDenied { .. } => {
                Some(("FORBIDDEN", StatusCode::FORBIDDEN, None, err.to_string()))
            }
            wicket_statemachine::Error::ActionMismatch { .. } => Some((
                "VALIDATION",
                StatusCode::BAD_REQUEST,
                Some("action"),
                err.to_string(),
            )),
            _ => None,
        }
    }

    /// Map to the docs/10 `(code, status, field, message)`.
    pub fn envelope(&self) -> (&'static str, StatusCode, Option<&str>, String) {
        match self {
            Self::Http {
                code,
                message,
                field,
                status,
            } => (*code, *status, field.as_deref(), message.clone()),
            Self::Identity(wicket_identity::Error::InvalidCredentials)
            | Self::Identity(wicket_identity::Error::Inactive)
            | Self::Identity(wicket_identity::Error::Lockout { .. }) => (
                "UNAUTHENTICATED",
                StatusCode::UNAUTHORIZED,
                None,
                self.to_string(),
            ),
            Self::Identity(wicket_identity::Error::NotFound)
            | Self::Items(wicket_mod_items::Error::NotFound(_))
            | Self::Locations(wicket_mod_locations::Error::NotFound(_))
            | Self::Lots(wicket_mod_lots::Error::NotFound)
            | Self::Inventory(wicket_mod_inventory::Error::NotFound)
            | Self::Production(wicket_mod_production_min::Error::NotFound)
            | Self::Genealogy(wicket_mod_genealogy::Error::NotFound) => {
                ("NOT_FOUND", StatusCode::NOT_FOUND, None, self.to_string())
            }
            Self::Lots(wicket_mod_lots::Error::InvalidIdentifier(_)) => (
                "VALIDATION",
                StatusCode::BAD_REQUEST,
                Some("identifier"),
                self.to_string(),
            ),
            Self::Items(wicket_mod_items::Error::InvalidNumber) => (
                "VALIDATION",
                StatusCode::BAD_REQUEST,
                Some("number"),
                self.to_string(),
            ),
            Self::Inventory(wicket_mod_inventory::Error::IdempotencyConflict) => (
                "IDEMPOTENCY_CONFLICT",
                StatusCode::CONFLICT,
                None,
                self.to_string(),
            ),
            Self::Inventory(wicket_mod_inventory::Error::Ledger(
                wicket_ledger::Error::AlreadyReversed,
            ))
            | Self::Ledger(wicket_ledger::Error::AlreadyReversed) => {
                ("CONFLICT", StatusCode::CONFLICT, None, self.to_string())
            }
            Self::Items(wicket_mod_items::Error::VersionConflict)
            | Self::Inventory(wicket_mod_inventory::Error::VersionConflict)
            | Self::Production(wicket_mod_production_min::Error::VersionConflict) => (
                "CONFLICT",
                StatusCode::CONFLICT,
                Some("version"),
                self.to_string(),
            ),
            Self::Locations(wicket_mod_locations::Error::Conflict(_)) => (
                "CONFLICT",
                StatusCode::CONFLICT,
                Some("version"),
                self.to_string(),
            ),
            Self::Statemachine(sm) => Self::from_sm(sm).unwrap_or((
                "INTERNAL",
                StatusCode::INTERNAL_SERVER_ERROR,
                None,
                self.to_string(),
            )),
            Self::Items(wicket_mod_items::Error::Statemachine(sm))
            | Self::Lots(wicket_mod_lots::Error::Statemachine(sm))
            | Self::Inventory(wicket_mod_inventory::Error::Statemachine(sm))
            | Self::Production(wicket_mod_production_min::Error::Statemachine(sm))
            | Self::Module(wicket_module::Error::Statemachine(sm))
            | Self::Lots(wicket_mod_lots::Error::Module(wicket_module::Error::Statemachine(sm)))
            | Self::Items(wicket_mod_items::Error::Module(wicket_module::Error::Statemachine(
                sm,
            )))
            | Self::Inventory(wicket_mod_inventory::Error::Module(
                wicket_module::Error::Statemachine(sm),
            ))
            | Self::Inventory(wicket_mod_inventory::Error::Lots(wicket_mod_lots::Error::Module(
                wicket_module::Error::Statemachine(sm),
            )))
            | Self::Inventory(wicket_mod_inventory::Error::Lots(
                wicket_mod_lots::Error::Statemachine(sm),
            ))
            | Self::Production(wicket_mod_production_min::Error::Module(
                wicket_module::Error::Statemachine(sm),
            )) => Self::from_sm(sm).unwrap_or((
                "INTERNAL",
                StatusCode::INTERNAL_SERVER_ERROR,
                None,
                self.to_string(),
            )),
            Self::Inventory(e) => {
                let code = wicket_mod_inventory::error_code(e);
                let status = StatusCode::from_u16(wicket_mod_inventory::http_status(e))
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                (code, status, None, e.to_string())
            }
            Self::Config(_) => (
                "VALIDATION",
                StatusCode::BAD_REQUEST,
                None,
                self.to_string(),
            ),
            Self::Customfields(e) => crate::handlers::customfields::envelope_arm(e),
            Self::Documents(e) => crate::handlers::documents::envelope_arm(e),
            Self::Print(e) => crate::handlers::print::envelope_arm(e),
            _ => (
                "INTERNAL",
                StatusCode::INTERNAL_SERVER_ERROR,
                None,
                self.to_string(),
            ),
        }
    }
}

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        Error::Db(err.into())
    }
}

impl From<wicket_esign::Error> for Error {
    fn from(err: wicket_esign::Error) -> Self {
        match err {
            wicket_esign::Error::SignatureRequired { field } => Self::http(
                "SIGNATURE_REQUIRED",
                format!("signature required: {field}"),
                Some(&field),
                StatusCode::UNAUTHORIZED,
            ),
            wicket_esign::Error::Validation { field, message } => {
                Self::validation(message, field.as_deref())
            }
            wicket_esign::Error::Conflict { message } => Self::conflict(message, None),
            wicket_esign::Error::NotFound => Self::not_found("signature not found"),
            wicket_esign::Error::Signature(sig) => {
                let fallback = sig.to_string();
                let (code, status, message) = Self::from_signature(&sig).unwrap_or((
                    "INTERNAL",
                    StatusCode::INTERNAL_SERVER_ERROR,
                    fallback,
                ));
                Self::http(code, message, None, status)
            }
            wicket_esign::Error::Identity(e) => Self::Identity(e),
            wicket_esign::Error::Db(e) => Self::Db(e),
            wicket_esign::Error::Core(e) => Self::Core(e),
            other => Self::Config(other.to_string()),
        }
    }
}
