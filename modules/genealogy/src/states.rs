//! State machines. Genealogy is read-only: it has no document lifecycle.

/// Genealogy does not own a document type. Status never changes here
/// (R-2s-5 does not apply; there is no status column to UPDATE).
pub const DOC_TYPE: &str = "genealogy";
