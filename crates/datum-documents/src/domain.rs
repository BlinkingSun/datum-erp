//! Domain types for controlled documents.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use datum_core::Identifier;

/// Machine `doc_type` registered by [`crate::document_machine`].
pub const DOC_TYPE: &str = "document";

/// Permission keys exported for `datum-module`.
pub const PERMISSIONS: &[&str] = &[
    "documents.view",
    "documents.edit",
    "documents.approve",
    "documents.release",
];

/// Event name: a revision row was inserted.
pub const EVENT_REVISION_CREATED: &str = "documents.revision_created";
/// Event name: a revision became Effective.
pub const EVENT_EFFECTIVE: &str = "documents.effective";

/// Document identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DocumentId(pub Identifier);

impl DocumentId {
    /// Mint a new id.
    pub fn generate() -> Self {
        Self(Identifier::generate())
    }

    /// Inner uuid.
    pub fn as_uuid(self) -> Uuid {
        self.0.as_uuid()
    }
}

/// Revision identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RevisionId(pub Identifier);

impl RevisionId {
    /// Mint a new id.
    pub fn generate() -> Self {
        Self(Identifier::generate())
    }

    /// Inner uuid.
    pub fn as_uuid(self) -> Uuid {
        self.0.as_uuid()
    }
}

/// Attachment identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AttachmentId(pub Identifier);

impl AttachmentId {
    /// Mint a new id.
    pub fn generate() -> Self {
        Self(Identifier::generate())
    }

    /// Inner uuid.
    pub fn as_uuid(self) -> Uuid {
        self.0.as_uuid()
    }
}

/// Link identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LinkId(pub Identifier);

impl LinkId {
    /// Mint a new id.
    pub fn generate() -> Self {
        Self(Identifier::generate())
    }

    /// Inner uuid.
    pub fn as_uuid(self) -> Uuid {
        self.0.as_uuid()
    }
}

/// SHA-256 digest used as the blob primary key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlobHash(pub [u8; 32]);

impl BlobHash {
    /// Bytes.
    pub fn as_bytes(self) -> [u8; 32] {
        self.0
    }

    /// Lowercase hex.
    pub fn to_hex(self) -> String {
        datum_audit::sha256::hex(&self.0)
    }

    /// Parse a 32-byte slice.
    pub fn from_slice(bytes: &[u8]) -> Option<Self> {
        let arr: [u8; 32] = bytes.try_into().ok()?;
        Some(Self(arr))
    }
}

/// Effectivity / expiry precision (invariant 12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DatePrecision {
    /// Day.
    Day,
    /// Month (stored as first of month when a date is derived).
    Month,
    /// Year (stored as 1 January when a date is derived).
    Year,
}

impl DatePrecision {
    /// SQL / wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Month => "month",
            Self::Year => "year",
        }
    }

    pub(crate) fn parse(s: &str) -> Option<Self> {
        match s {
            "day" => Some(Self::Day),
            "month" => Some(Self::Month),
            "year" => Some(Self::Year),
            _ => None,
        }
    }
}

/// Workflow status stored on document and revision (mirrored from the machine).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Status {
    /// Working copy.
    Draft,
    /// Submitted for review.
    InReview,
    /// Approved, not yet effective.
    Approved,
    /// Live.
    Effective,
    /// Replaced by a successor.
    Superseded,
    /// Withdrawn from use.
    Obsolete,
    /// Cancelled; the number is kept.
    Void,
}

impl Status {
    /// SQL / wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "Draft",
            Self::InReview => "InReview",
            Self::Approved => "Approved",
            Self::Effective => "Effective",
            Self::Superseded => "Superseded",
            Self::Obsolete => "Obsolete",
            Self::Void => "Void",
        }
    }

    pub(crate) fn parse(s: &str) -> Option<Self> {
        match s {
            "Draft" => Some(Self::Draft),
            "InReview" => Some(Self::InReview),
            "Approved" => Some(Self::Approved),
            "Effective" => Some(Self::Effective),
            "Superseded" => Some(Self::Superseded),
            "Obsolete" => Some(Self::Obsolete),
            "Void" => Some(Self::Void),
            _ => None,
        }
    }
}

/// Content manifest carried on a revision, plus optional effectivity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Opaque content list (filenames, hashes, notes). Never interpreted here.
    pub content: Value,
    /// Inclusive start of effectivity.
    pub effective_from: Option<DateTime<Utc>>,
    /// Exclusive end of effectivity (`NULL` = open).
    pub effective_until: Option<DateTime<Utc>>,
    /// Precision of `effective_from` (required when `effective_from` is set).
    pub from_precision: Option<DatePrecision>,
    /// Precision of `effective_until`.
    pub until_precision: Option<DatePrecision>,
}

impl Manifest {
    /// Content-only manifest with no effectivity window.
    pub fn content(content: Value) -> Self {
        Self {
            content,
            effective_from: None,
            effective_until: None,
            from_precision: None,
            until_precision: None,
        }
    }
}

/// Document master row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    /// Id.
    pub id: DocumentId,
    /// Document type / numbering sequence (`SOP`, `WI`, …).
    pub kind: String,
    /// Gap-free number allocated late inside the create transaction.
    pub number: String,
    /// Title.
    pub title: String,
    /// Workflow status (only the machine writes this column).
    pub status: Status,
    /// Retention class, stamped onto every revision.
    pub retention_class: String,
    /// When true, Obsolete, Superseded, and retention actions are refused.
    pub legal_hold: bool,
}

/// Revision row (version chain via `supersedes_revision_id`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revision {
    /// Id.
    pub id: RevisionId,
    /// Parent document.
    pub document_id: DocumentId,
    /// Human revision label (`A`, `B`, `1`).
    pub label: String,
    /// Predecessor in the version chain.
    pub supersedes: Option<RevisionId>,
    /// Content manifest.
    pub manifest: Manifest,
    /// Workflow status. The only in-place update on this row.
    pub status: Status,
    /// Retention class copied from the document at insert.
    pub retention_class: String,
}

/// Payload contract exported for `datum-module` event registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventSchemaDecl {
    /// Event name.
    pub name: &'static str,
    /// Schema version.
    pub version: i16,
    /// Required field names.
    pub fields: &'static [&'static str],
}

/// Event schemas this crate declares.
pub const EVENT_SCHEMAS: &[EventSchemaDecl] = &[
    EventSchemaDecl {
        name: EVENT_REVISION_CREATED,
        version: 1,
        fields: &["document_id", "revision_id", "label"],
    },
    EventSchemaDecl {
        name: EVENT_EFFECTIVE,
        version: 1,
        fields: &["document_id", "revision_id", "effective_from"],
    },
];
