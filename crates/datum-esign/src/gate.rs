//! Per-transaction prepared gate (D-2b-4, D-2b-5).

use chrono::{DateTime, Utc};
use datum_core::{
    NoSignatures, RecordRef, SignatureError, SignatureGate, SignatureId, SignatureRequirement,
    SignatureToken,
};
use datum_identity::PrincipalStatus;
use serde_json::Value;
use sqlx::query_as as sql_query_as;
use uuid::Uuid;

use crate::error::map_tx;
use crate::hash::{content_hash, snapshot};
use crate::projection::project;
use crate::{Error, InstanceTriple, Result};

/// Live document the composition root read in the transition's transaction.
#[derive(Debug, Clone)]
pub struct LiveDoc {
    /// Live record reference the executor is about to mutate.
    pub record: RecordRef,
    /// Display document type (projection key).
    pub doc_type: String,
    /// Live business-record JSON.
    pub projection: Value,
    /// Live `sm.instance` triple.
    pub instance: InstanceTriple,
    /// Live signer status from [`datum_identity::load_principal`] (no `Tx` API).
    pub signer_status: PrincipalStatus,
}

type SigRow = (
    Uuid,
    Uuid,
    String,
    DateTime<Utc>,
    String,
    Uuid,
    i64,
    Vec<u8>,
    Value,
    Vec<String>,
    Option<DateTime<Utc>>,
    Option<Uuid>,
);

/// In-memory gate produced by [`prepare`]. Claims the row in `tx`.
#[derive(Debug, Clone)]
pub struct PreparedGate {
    first: Option<SignatureError>,
    claim_ok: bool,
    meaning: String,
    record_table: String,
    record_id: Uuid,
    record_version: i64,
    row_hash: [u8; 32],
    live_hash: [u8; 32],
    token_hash: [u8; 32],
    permission_snapshot: Vec<String>,
}

impl SignatureGate for PreparedGate {
    fn verify(
        &self,
        token: &SignatureToken,
        required: &SignatureRequirement,
        record: &RecordRef,
    ) -> core::result::Result<(), SignatureError> {
        // D-2b-5 frozen order: row, meaning, reference, hashes, permission, claim.
        if let Some(err) = &self.first {
            return Err(err.clone());
        }
        if self.meaning != required.meaning.0 || token.meaning.0 != required.meaning.0 {
            return Err(SignatureError::MeaningMismatch);
        }
        if self.record_table != record.table
            || self.record_id != record.id.as_uuid()
            || self.record_version != record.version
            || token.record.table != record.table
            || token.record.id != record.id
            || token.record.version != record.version
        {
            return Err(SignatureError::RecordMismatch);
        }
        if self.token_hash != self.row_hash || self.live_hash != self.row_hash {
            return Err(SignatureError::HashMismatch);
        }
        if !self
            .permission_snapshot
            .iter()
            .any(|k| k == &required.permission.0)
        {
            return Err(SignatureError::SignerNotPermitted);
        }
        if !self.claim_ok {
            return Err(SignatureError::Consumed);
        }
        Ok(())
    }
}

/// A bound gate: `NoSignatures` or a prepared esign gate.
#[derive(Debug, Clone)]
pub enum BoundGate {
    /// No provider.
    NoSignatures(NoSignatures),
    /// Prepared claim.
    Prepared(Box<PreparedGate>),
}

impl SignatureGate for BoundGate {
    fn verify(
        &self,
        token: &SignatureToken,
        required: &SignatureRequirement,
        record: &RecordRef,
    ) -> core::result::Result<(), SignatureError> {
        match self {
            BoundGate::NoSignatures(g) => g.verify(token, required, record),
            BoundGate::Prepared(g) => g.verify(token, required, record),
        }
    }
}

/// Factory the composition root binds per profile (`GateBinding`).
#[derive(Debug, Clone, Copy)]
pub struct GateFactory {
    kind: FactoryKind,
}

#[derive(Debug, Clone, Copy)]
enum FactoryKind {
    NoSignatures,
    DatumEsign,
}

impl GateFactory {
    /// `NoSignatures` binding (plain-shop, tests).
    pub fn no_signatures() -> Self {
        Self {
            kind: FactoryKind::NoSignatures,
        }
    }

    /// `datum-esign` binding (regulated-device from 2b.1).
    pub fn datum_esign() -> Self {
        Self {
            kind: FactoryKind::DatumEsign,
        }
    }

    /// Prepare a gate inside the transition's transaction.
    pub async fn prepare(
        &self,
        tx: &mut datum_db::Tx<'_>,
        token: &SignatureToken,
        doc: &LiveDoc,
    ) -> Result<BoundGate> {
        match self.kind {
            FactoryKind::NoSignatures => Ok(BoundGate::NoSignatures(NoSignatures)),
            FactoryKind::DatumEsign => {
                let gate = prepare(tx, token, doc).await?;
                Ok(BoundGate::Prepared(Box::new(gate)))
            }
        }
    }
}

/// Lock the row `FOR UPDATE`, claim it, recompute the live hash.
///
/// Signer activity is [`LiveDoc::signer_status`]: [`datum_identity::load_principal`]
/// takes `&Pool`, not `&mut Tx`, and a second pool connection deadlocks a
/// `max_connections=2` claim.
pub async fn prepare(
    tx: &mut datum_db::Tx<'_>,
    token: &SignatureToken,
    doc: &LiveDoc,
) -> Result<PreparedGate> {
    let row: Option<SigRow> = tx
        .fetch_optional(
            sql_query_as(
                r#"SELECT signature_id, signer_id, meaning, expires_at,
                          record_table, record_id, record_version,
                          record_content_hash, record_snapshot, permission_snapshot,
                          consumed_at, superseded_by
                     FROM esign.signature
                    WHERE signature_id = $1
                    FOR UPDATE"#,
            )
            .bind(token.signature.as_uuid()),
        )
        .await
        .map_err(map_tx)?;

    let Some(row) = row else {
        return Ok(PreparedGate {
            first: Some(SignatureError::Invalid("no such signature".into())),
            claim_ok: false,
            meaning: String::new(),
            record_table: String::new(),
            record_id: Uuid::nil(),
            record_version: 0,
            row_hash: [0; 32],
            live_hash: [0; 32],
            token_hash: token.record_content_hash,
            permission_snapshot: Vec::new(),
        });
    };

    let expires_at = row.3;
    let signer_id = row.1;
    let mut first = None;
    if expires_at <= Utc::now() {
        first = Some(SignatureError::Invalid("expired".into()));
    }
    if first.is_none() && doc.signer_status != PrincipalStatus::Active {
        first = Some(SignatureError::Invalid("signer inactive".into()));
    }
    if first.is_none() && token.signer.id.as_uuid() != signer_id {
        first = Some(SignatureError::Invalid("signer mismatch".into()));
    }

    let claimed: Option<(DateTime<Utc>,)> = tx
        .fetch_optional(
            sql_query_as(
                r#"UPDATE esign.signature
                      SET consumed_at = now(),
                          consumed_xid = pg_current_xact_id()
                    WHERE signature_id = $1 AND consumed_at IS NULL
                    RETURNING consumed_at"#,
            )
            .bind(token.signature.as_uuid()),
        )
        .await
        .map_err(map_tx)?;
    let claim_ok = claimed.is_some();

    let mut row_hash = [0u8; 32];
    if row.7.len() == 32 {
        row_hash.copy_from_slice(&row.7);
    } else if first.is_none() {
        first = Some(SignatureError::HashMismatch);
    }

    let projected = project(&doc.doc_type, &doc.projection);
    let live_snapshot = snapshot(&projected, &doc.instance)?;
    let live_hash = content_hash(&live_snapshot)?;

    Ok(PreparedGate {
        first,
        claim_ok,
        meaning: row.2,
        record_table: row.4,
        record_id: row.5,
        record_version: row.6,
        row_hash,
        live_hash,
        token_hash: token.record_content_hash,
        permission_snapshot: row.9,
    })
}

/// Mark `old` as superseded by `new`. Monotone insert into `esign.supersession`
/// (D-2b-1 does not grant `UPDATE (superseded_by)`).
pub async fn supersede(
    tx: &mut datum_db::Tx<'_>,
    old: SignatureId,
    new: SignatureId,
) -> Result<()> {
    let inserted: (bool,) = tx
        .fetch_one(
            sql_query_as("SELECT esign.supersede_signature($1, $2)")
                .bind(old.as_uuid())
                .bind(new.as_uuid()),
        )
        .await
        .map_err(map_tx)?;
    if !inserted.0 {
        return Err(Error::NotFound);
    }
    Ok(())
}
