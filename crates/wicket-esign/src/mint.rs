//! Mint a signature row (D-2b-3, D-2b-8). Verify, never mint, is the gate.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::query_as as sql_query_as;
use uuid::Uuid;
use wicket_core::{
    Actor, Identifier, PermissionKey, RecordRef, SignatureId, SignatureMeaning, SignatureToken,
};
use wicket_identity::rbac::has_permission;
use wicket_identity::{Principal, PrincipalStatus, reauth_signing, verify_login_secret};

use crate::error::map_tx;
use crate::hash::{content_hash, snapshot};
use crate::projection::project;
use crate::session::{
    SessionPolicy, close_sessions_for, device_changed, is_continuous, load_open, open_session,
    touch_session,
};
use crate::{Error, InstanceTriple, Result};

/// Inputs to [`mint`].
#[derive(Debug, Clone)]
pub struct MintRequest {
    /// Identification components presented (`["code","secret"]` or `["secret"]`).
    pub components: Vec<String>,
    /// Identification code (username). Required when `components` includes `code`.
    pub code: Option<String>,
    /// Signing secret. Never the login secret.
    pub secret: String,
    /// Printed meaning (`Released`, `Approved`, …).
    pub meaning: SignatureMeaning,
    /// Reason text; required when `meaning_policy.requires_reason`.
    pub reason: Option<String>,
    /// Record the signer is committing to.
    pub record: RecordRef,
    /// Display/manifestation document type.
    pub doc_type: String,
    /// Business-record JSON; a registered projection is applied before hashing.
    pub projection: Value,
    /// `sm.instance` triple read by the caller in this transaction.
    pub instance: InstanceTriple,
    /// Permission the consuming edge will require (snapshotted if currently held).
    pub permission: PermissionKey,
    /// Signer's IANA zone (11.50(a)(2)).
    pub signed_at_zone: String,
    /// Session relaxation keys (SPEC-profiles key 4 sub-keys).
    pub policy: SessionPolicy,
    /// Authenticated principal (session). Snapshots printed name and username.
    pub principal: Principal,
    /// Login session id, if any.
    pub login_session_id: Option<Uuid>,
    /// Device fingerprint.
    pub device_fingerprint: Option<String>,
    /// Source IP.
    pub source_ip: Option<String>,
    /// Boot epoch; a change closes the signing session.
    pub boot_epoch: String,
    /// `signing_password` or `idp_step_up`.
    pub credential_kind: String,
}

/// A minted signature row (no secrets).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    /// Signature id (the nonce).
    pub id: SignatureId,
    /// Signer principal id.
    pub signer_id: Identifier,
    /// Printed name snapshot.
    pub printed_name: String,
    /// Username snapshot.
    pub username: String,
    /// Meaning snapshot.
    pub meaning: SignatureMeaning,
    /// Reason snapshot.
    pub reason: Option<String>,
    /// Server time of signing (UTC).
    pub signed_at: DateTime<Utc>,
    /// Signer's IANA zone.
    pub signed_at_zone: String,
    /// Record reference.
    pub record: RecordRef,
    /// Display document type.
    pub doc_type: String,
    /// SHA-256 of the canonical snapshot.
    pub record_content_hash: [u8; 32],
    /// Canonical snapshot that was hashed.
    pub record_snapshot: Value,
    /// Effective permission keys held at mint.
    pub permission_snapshot: Vec<String>,
    /// Credential kind.
    pub credential_kind: String,
    /// Components used (`["code","secret"]` or `["secret"]`).
    pub components_used: Vec<String>,
    /// Expiry (`signed_at + max_window_secs`).
    pub expires_at: DateTime<Utc>,
}

impl Signature {
    /// Token the transition presents to the gate.
    pub fn token(&self, signer: Actor) -> SignatureToken {
        SignatureToken {
            signature: self.id,
            signer,
            meaning: self.meaning.clone(),
            record: self.record.clone(),
            record_content_hash: self.record_content_hash,
        }
    }
}

/// Mint a signature row bound to the exact content of the record version.
pub async fn mint(tx: &mut wicket_db::Tx<'_>, req: &MintRequest) -> Result<Signature> {
    if req.principal.status != PrincipalStatus::Active {
        return Err(Error::Validation {
            field: Some("identification.code".into()),
            message: "signer is inactive".into(),
        });
    }
    let user = req.principal.id;
    let two =
        req.components.iter().any(|c| c == "code") && req.components.iter().any(|c| c == "secret");
    let secret_only = req.components.len() == 1 && req.components[0] == "secret";
    if !two && !secret_only {
        return Err(Error::Validation {
            field: Some("identification".into()),
            message: "signing requires components [code, secret] or [secret]".into(),
        });
    }

    let open = load_open(tx, user).await?;
    let now = Utc::now();
    if let Some(ref s) = open {
        if device_changed(
            s,
            req.device_fingerprint.as_deref(),
            req.source_ip.as_deref(),
            &req.boot_epoch,
        ) {
            close_sessions_for(tx, user, "device_change").await?;
        } else if let Some(login) = req.login_session_id
            && s.login_session_id.is_some()
            && s.login_session_id != Some(login)
        {
            close_sessions_for(tx, user, "login_session_change").await?;
        }
    }
    let open = load_open(tx, user).await?;
    let continuous = open
        .as_ref()
        .is_some_and(|s| req.policy.is_on() && is_continuous(s, now, &req.policy));

    if secret_only && !continuous {
        return Err(Error::Validation {
            field: Some("identification.code".into()),
            message: "two identification components are required".into(),
        });
    }
    if two {
        let code = req.code.as_deref().unwrap_or("");
        if !code.eq_ignore_ascii_case(&req.principal.username) {
            return Err(Error::Validation {
                field: Some("identification.code".into()),
                message: "identification code does not match the signer".into(),
            });
        }
    }

    match reauth_signing(tx, user, &req.secret).await {
        Ok(()) => {}
        Err(wicket_identity::Error::InvalidCredentials) => {
            let _ = close_sessions_for(tx, user, "failed_signing").await;
            if verify_login_secret(tx, user, &req.secret).await? {
                return Err(Error::Validation {
                    field: Some("identification.secret".into()),
                    message: "login secret is not a signing component".into(),
                });
            }
            return Err(Error::SignatureRequired {
                field: "identification.secret".into(),
            });
        }
        Err(e) => return Err(e.into()),
    }

    let policy_row: Option<(bool, String)> = tx
        .fetch_optional(
            sql_query_as(
                r#"SELECT requires_reason, permission_hint
                     FROM esign.meaning_policy WHERE meaning = $1"#,
            )
            .bind(&req.meaning.0),
        )
        .await
        .map_err(map_tx)?;
    if let Some((requires_reason, _)) = &policy_row
        && *requires_reason
        && req.reason.as_deref().unwrap_or("").trim().is_empty()
    {
        return Err(Error::Validation {
            field: Some("reason".into()),
            message: format!("meaning {} requires a reason", req.meaning.0),
        });
    }

    let actor = req.principal.actor();
    let mut permission_snapshot = Vec::new();
    if has_permission(tx, actor, &req.permission.0).await? {
        permission_snapshot.push(req.permission.0.clone());
    }
    if let Some((_, hint)) = &policy_row
        && !hint.is_empty()
        && hint != &req.permission.0
        && has_permission(tx, actor, hint).await?
    {
        permission_snapshot.push(hint.clone());
    }

    let projected = project(&req.doc_type, &req.projection);
    let record_snapshot = snapshot(&projected, &req.instance)?;
    let record_content_hash = content_hash(&record_snapshot)?;

    let session_id = if secret_only {
        let s = open.expect("continuous session");
        touch_session(tx, s.id).await?;
        Some(s.id)
    } else {
        let opened = open_session(
            tx,
            user,
            req.login_session_id,
            req.device_fingerprint.as_deref(),
            &req.boot_epoch,
            req.source_ip.as_deref(),
        )
        .await?;
        Some(opened.id)
    };

    let id = SignatureId::generate();
    let credential_kind = if req.credential_kind.is_empty() {
        "signing_password"
    } else {
        req.credential_kind.as_str()
    };
    let row: (DateTime<Utc>, DateTime<Utc>) = tx
        .fetch_one(
            sql_query_as(
                r#"INSERT INTO esign.signature (
                       signature_id, signer_id, signer_printed_name, signer_username,
                       meaning, reason, signed_at, signed_at_zone,
                       record_table, record_id, record_version, doc_type,
                       record_content_hash, record_snapshot, permission_snapshot,
                       credential_kind, components_used, signing_session_id,
                       login_session_id, source_device, source_ip, expires_at
                   ) VALUES (
                       $1, $2, $3, $4,
                       $5, $6, now(), $7,
                       $8, $9, $10, $11,
                       $12, $13, $14,
                       $15, $16, $17,
                       $18, $19, CAST($20 AS inet),
                       now() + make_interval(secs => $21)
                   )
                   RETURNING signed_at, expires_at"#,
            )
            .bind(id.as_uuid())
            .bind(user.as_uuid())
            .bind(&req.principal.display_name)
            .bind(&req.principal.username)
            .bind(&req.meaning.0)
            .bind(&req.reason)
            .bind(&req.signed_at_zone)
            .bind(&req.record.table)
            .bind(req.record.id.as_uuid())
            .bind(req.record.version)
            .bind(&req.doc_type)
            .bind(record_content_hash.as_slice())
            .bind(&record_snapshot)
            .bind(&permission_snapshot)
            .bind(credential_kind)
            .bind(&req.components)
            .bind(session_id)
            .bind(req.login_session_id)
            .bind(&req.device_fingerprint)
            .bind(&req.source_ip)
            .bind(req.policy.max_window_secs),
        )
        .await
        .map_err(map_tx)?;

    Ok(Signature {
        id,
        signer_id: req.principal.id.0,
        printed_name: req.principal.display_name.clone(),
        username: req.principal.username.clone(),
        meaning: req.meaning.clone(),
        reason: req.reason.clone(),
        signed_at: row.0,
        signed_at_zone: req.signed_at_zone.clone(),
        record: req.record.clone(),
        doc_type: req.doc_type.clone(),
        record_content_hash,
        record_snapshot,
        permission_snapshot,
        credential_kind: credential_kind.to_owned(),
        components_used: req.components.clone(),
        expires_at: row.1,
    })
}

/// Write a security event on a fresh transaction after a rolled-back mint or gate refusal.
pub async fn log_refusal(
    pool: &wicket_db::WritePool,
    actor: Actor,
    action: &str,
    reason: &str,
    detail: Value,
) -> Result<Uuid> {
    let mut ctx = wicket_db::WriteContext::new(actor, "esign.refuse", "api");
    ctx.reason = Some(reason.to_owned());
    let mut tx = wicket_db::Tx::begin(pool, &ctx).await?;
    let row: (Uuid,) = tx
        .fetch_one(
            sql_query_as(
                r#"SELECT audit.log_event(
                       $1, $2, $3, $4, $5, $6, CAST($7 AS jsonb)
                   )"#,
            )
            .bind("security.esign.refusal")
            .bind(action)
            .bind(reason)
            .bind("")
            .bind("")
            .bind("")
            .bind(&detail),
        )
        .await
        .map_err(map_tx)?;
    tx.commit().await.map_err(map_tx)?;
    Ok(row.0)
}
