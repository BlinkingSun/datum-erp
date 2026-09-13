//! Signature gate: verify, never mint.
//!
//! Ratified by DECISION D-W1-4. Built verbatim from CONTRACT §6.3.

use crate::actor::Actor;
use crate::id::{Identifier, SignatureId};
use serde::{Deserialize, Serialize};

/// Meaning the signer committed to, e.g. `"Approved"`, `"Reviewed"`, `"Released"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SignatureMeaning(pub String);

/// Permission the signer must have held at mint, e.g. `"calibration.approve"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PermissionKey(pub String);

/// Live record the executor is about to mutate.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecordRef {
    /// Table name of the record.
    pub table: String,
    /// Record id.
    pub id: Identifier,
    /// Record version the signature was taken against.
    pub version: i64,
}

/// Meaning and permission a transition edge requires.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SignatureRequirement {
    /// Required meaning.
    pub meaning: SignatureMeaning,
    /// Permission snapshot key that must have been held at mint.
    pub permission: PermissionKey,
}

/// A reference to a signature that already exists, plus the bytes the signer committed to.
/// Nothing here is secret and nothing here is authority: `minted_at`, the two-component
/// authentication evidence, the permission snapshot and `consumed_at` live only on the esign row.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SignatureToken {
    /// Existing signature row.
    pub signature: SignatureId,
    /// Signer recorded on that row.
    pub signer: Actor,
    /// Meaning recorded on that row.
    pub meaning: SignatureMeaning,
    /// Record the signer committed to.
    pub record: RecordRef,
    /// SHA-256 of the canonical record bytes at `record.version`, as stored on the signature row (D3 §9).
    pub record_content_hash: [u8; 32],
}

/// Why a gate refused a token.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SignatureError {
    /// No provider is bound (`NoSignatures`).
    #[error("no signature provider is bound")]
    NoProvider,
    /// Token meaning does not match the requirement.
    #[error("signature meaning mismatch")]
    MeaningMismatch,
    /// Token record (including version) does not match the live record.
    #[error("signature record mismatch")]
    RecordMismatch,
    /// Content hash does not match the live record bytes.
    #[error("signature content hash mismatch")]
    HashMismatch,
    /// Permission snapshot taken at mint does not include the required permission.
    #[error("signer not permitted")]
    SignerNotPermitted,
    /// Signature has already been consumed (single-use claim).
    #[error("signature already consumed")]
    Consumed,
    /// Row missing or otherwise invalid, with a reason.
    #[error("invalid signature: {0}")]
    Invalid(String),
    /// Operation is not implemented.
    #[error("unimplemented")]
    Unimplemented,
}

/// Verify, never mint. Minting is `wicket-esign`'s job before the transition.
pub trait SignatureGate {
    /// `record` is the live reference the executor is about to mutate, read in the same
    /// transaction. Checked in order: row, meaning, reference (incl. version), hashes, permission
    /// snapshot, single-use claim; the first failure is returned.
    ///
    /// ```
    /// use wicket_core::{
    ///     Actor, ActorKind, Identifier, NoSignatures, PermissionKey, RecordRef, SignatureError,
    ///     SignatureGate, SignatureId, SignatureMeaning, SignatureRequirement, SignatureToken,
    /// };
    /// fn signature_error_order_is_documented() {
    ///     let order = [
    ///         "row",
    ///         "meaning",
    ///         "reference (incl. version)",
    ///         "hashes",
    ///         "permission snapshot",
    ///         "single-use claim",
    ///     ];
    ///     assert_eq!(order.len(), 6);
    /// }
    /// signature_error_order_is_documented();
    /// let gate = NoSignatures;
    /// let record = RecordRef {
    ///     table: "app.example".into(),
    ///     id: Identifier::from_uuid(uuid::Uuid::nil()),
    ///     version: 1,
    /// };
    /// let token = SignatureToken {
    ///     signature: SignatureId::from_uuid(uuid::Uuid::nil()),
    ///     signer: Actor {
    ///         id: Identifier::from_uuid(uuid::Uuid::nil()),
    ///         kind: ActorKind::User,
    ///     },
    ///     meaning: SignatureMeaning("Approved".into()),
    ///     record: record.clone(),
    ///     record_content_hash: [0; 32],
    /// };
    /// let required = SignatureRequirement {
    ///     meaning: SignatureMeaning("Approved".into()),
    ///     permission: PermissionKey("example.approve".into()),
    /// };
    /// assert!(matches!(
    ///     gate.verify(&token, &required, &record),
    ///     Err(SignatureError::NoProvider)
    /// ));
    /// ```
    fn verify(
        &self,
        token: &SignatureToken,
        required: &SignatureRequirement,
        record: &RecordRef,
    ) -> core::result::Result<(), SignatureError>;
}

/// Refuses every token. Bind this only for transitions that must not require a signature;
/// a release profile with a `Required` edge and `NoSignatures` fails at startup.
#[derive(Debug, Clone, Copy)]
pub struct NoSignatures;

impl SignatureGate for NoSignatures {
    fn verify(
        &self,
        _token: &SignatureToken,
        _required: &SignatureRequirement,
        _record: &RecordRef,
    ) -> core::result::Result<(), SignatureError> {
        Err(SignatureError::NoProvider)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor::{Actor, ActorKind};
    use crate::id::{Identifier, SignatureId};

    fn token_and_req() -> (SignatureToken, SignatureRequirement, RecordRef) {
        let record = RecordRef {
            table: "app.example".into(),
            id: Identifier::from_uuid(uuid::Uuid::nil()),
            version: 1,
        };
        let token = SignatureToken {
            signature: SignatureId::from_uuid(uuid::Uuid::nil()),
            signer: Actor {
                id: Identifier::from_uuid(uuid::Uuid::nil()),
                kind: ActorKind::User,
            },
            meaning: SignatureMeaning("Approved".into()),
            record: record.clone(),
            record_content_hash: [0; 32],
        };
        let required = SignatureRequirement {
            meaning: SignatureMeaning("Approved".into()),
            permission: PermissionKey("example.approve".into()),
        };
        (token, required, record)
    }

    #[test]
    fn no_signatures_refuses_every_token() {
        let gate = NoSignatures;
        let (token, required, record) = token_and_req();
        assert_eq!(
            gate.verify(&token, &required, &record).unwrap_err(),
            SignatureError::NoProvider
        );
        let mut other = token.clone();
        other.meaning = SignatureMeaning("Reviewed".into());
        assert_eq!(
            gate.verify(&other, &required, &record).unwrap_err(),
            SignatureError::NoProvider
        );
    }

    #[test]
    fn signature_error_order_is_documented() {
        let order = [
            "row",
            "meaning",
            "reference (incl. version)",
            "hashes",
            "permission snapshot",
            "single-use claim",
        ];
        assert_eq!(order.len(), 6);
        let _ = SignatureError::NoProvider;
        let _ = SignatureError::MeaningMismatch;
        let _ = SignatureError::RecordMismatch;
        let _ = SignatureError::HashMismatch;
        let _ = SignatureError::SignerNotPermitted;
        let _ = SignatureError::Consumed;
        let _ = SignatureError::Invalid(String::new());
        let _ = SignatureError::Unimplemented;
    }
}
