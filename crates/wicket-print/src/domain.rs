//! Domain types for deterministic rendering.

use serde::{Deserialize, Serialize};
use wicket_core::Identifier;
use wicket_documents::BlobHash;

/// Built-in template identifiers (versioned in `print.template`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TemplateId(pub String);

impl TemplateId {
    /// Controlled document revision layout.
    pub const DOCUMENT_REVISION: &'static str = "document_revision";
    /// Generic key/value record.
    pub const GENERIC_RECORD: &'static str = "generic_record";
    /// Work-order traveler skeleton.
    pub const WORK_ORDER_TRAVELER: &'static str = "work_order_traveler";

    /// Construct from a name.
    pub fn new(name: &str) -> Self {
        Self(name.to_owned())
    }
}

/// Output format for a rendition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Format {
    /// Deterministic HTML (no external assets).
    Html,
    /// Deterministic PDF (pinned generator, fixed metadata).
    Pdf,
}

impl Format {
    /// Wire / storage label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Pdf => "pdf",
        }
    }

    /// Parse from storage label.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "html" => Some(Self::Html),
            "pdf" => Some(Self::Pdf),
            _ => None,
        }
    }
}

/// Latest effective template row (list seam; no body).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateSummary {
    /// Template id (`document_revision`, `generic_record`, `work_order_traveler`).
    pub template_id: String,
    /// Integer version of the latest effective row.
    pub version: i32,
    /// Semantic version stamped on that row.
    pub semantic_version: String,
    /// SHA-256 of the template body, lowercase hex.
    pub body_hash: String,
}

/// A finished rendition (bytes + stamped versions).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    /// Final bytes (HTML or PDF).
    pub bytes: Vec<u8>,
    /// SHA-256 of `bytes`.
    pub output_hash: [u8; 32],
    /// Template row version used.
    pub template_version: i32,
    /// Crate version that produced the bytes.
    pub renderer_version: String,
}

/// One row from `print.render_log`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderLogRow {
    /// Render id.
    pub render_id: Identifier,
    /// Record table.
    pub record_table: String,
    /// Record id.
    pub record_id: Identifier,
    /// Record version.
    pub record_version: i64,
    /// Content hash of the record version rendered.
    pub record_content_hash: [u8; 32],
    /// Template id.
    pub template_id: String,
    /// Template version.
    pub template_version: i32,
    /// Renderer version stamp.
    pub renderer_version: String,
    /// Output format label.
    pub output_format: String,
    /// Output bytes hash.
    pub output_hash: [u8; 32],
    /// Archived blob hash when present.
    pub blob_hash: Option<BlobHash>,
}
