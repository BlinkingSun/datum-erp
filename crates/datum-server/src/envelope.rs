//! docs/10 error envelope and list envelope.

use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::error::Error;

/// docs/10 §2.6 error body.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorBody {
    /// Envelope.
    pub error: ErrorFields,
}

/// Fields of [`ErrorBody`].
#[derive(Debug, Clone, Serialize)]
pub struct ErrorFields {
    /// Machine-stable token.
    pub code: String,
    /// Human message.
    pub message: String,
    /// Field path, if any.
    pub field: Option<String>,
    /// Request id.
    pub request_id: String,
}

/// List envelope (docs/10 §2.3).
#[derive(Debug, Clone, Serialize)]
pub struct ListBody<T: Serialize> {
    /// Page.
    pub data: Vec<T>,
    /// Opaque next cursor.
    pub next_cursor: Option<String>,
    /// Whether another page exists.
    pub has_more: bool,
}

/// Request id stored in extensions.
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

impl RequestId {
    /// Inner id.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (code, status, field, message) = self.envelope();
        let request_id = "00000000-0000-0000-0000-000000000000".to_string();
        let body = ErrorBody {
            error: ErrorFields {
                code: code.to_string(),
                message,
                field: field.map(str::to_owned),
                request_id: request_id.clone(),
            },
        };
        let mut resp = (status, axum::Json(body)).into_response();
        if let Ok(v) = HeaderValue::from_str(&request_id) {
            resp.headers_mut().insert("x-request-id", v);
        }
        if status == StatusCode::UNAUTHORIZED {
            resp.headers_mut().insert(
                "www-authenticate",
                HeaderValue::from_static("Bearer, Cookie"),
            );
        }
        resp
    }
}

/// Attach `request_id` to an [`Error`] response.
pub fn error_response(err: Error, request_id: &str) -> Response {
    let (code, status, field, message) = err.envelope();
    let body = ErrorBody {
        error: ErrorFields {
            code: code.to_string(),
            message,
            field: field.map(str::to_owned),
            request_id: request_id.to_string(),
        },
    };
    let mut resp = (status, axum::Json(body)).into_response();
    if let Ok(v) = HeaderValue::from_str(request_id) {
        resp.headers_mut().insert("x-request-id", v);
    }
    resp
}
