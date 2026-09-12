//! Persist catalog rows, spawn instances, and run the transition executor.

use chrono::{DateTime, Utc};
use datum_core::{
    PostingError, PostingSink, RecordRef, SignatureError, SignatureGate, SignatureToken,
};
use datum_db::{Tx, WriteContext};
use sqlx::{query as sql_query, query_as as sql_query_as};
use uuid::Uuid;

use crate::decl::{
    DocRef, Edge, Instance, Machine, MachineId, SignatureDeclaration, State, action_for,
};
use crate::engine::{Engine, HookPhase, HookView};
use crate::{Error, Result};

impl Engine {
    /// Mirror registered machines into `sm.machine` / `sm.state` / `sm.edge`.
    /// Idempotent on machine id. The composition root calls this at startup.
    /// Refuses until [`Engine::freeze`] (SPEC: hook order computed once at startup).
    pub async fn persist(&self, tx: &mut Tx<'_>) -> Result<()> {
        self.ensure_frozen()?;
        for m in &self.machines {
            persist_machine(tx, m).await?;
        }
        Ok(())
    }

    /// Insert `sm.instance` in `initial`. The state column is only changed later by
    /// [`Engine::transition`].
    /// Refuses until [`Engine::freeze`] (SPEC: hook order computed once at startup).
    pub async fn spawn(&self, tx: &mut Tx<'_>, doc: &DocRef, initial: &str) -> Result<Instance> {
        self.ensure_frozen()?;
        let machine = self.machine_for(&doc.doc_type)?;
        if !machine.states.iter().any(|s| s.0 == initial) {
            return Err(Error::InvalidState {
                expected: initial.to_owned(),
                actual: String::new(),
            });
        }
        tx.execute(
            sql_query(
                r#"INSERT INTO sm.instance
                       (doc_type, doc_id, machine_id, state, version, entered_at)
                   VALUES ($1, $2, $3, $4, 1, now())"#,
            )
            .bind(&doc.doc_type)
            .bind(doc.doc_id.as_uuid())
            .bind(machine.id.as_uuid())
            .bind(initial),
        )
        .await?;
        load_instance(tx, doc).await
    }

    /// Transition executor (CONTRACT §6.2 rules 1 and 8; §6.3 executor obligation).
    ///
    /// Does not commit: the caller's [`Tx`] does. `sink` is the one posting sink for
    /// this transaction; hooks receive `&mut dyn PostingSink` and cannot `finalize`.
    /// The executor `finalize`s exactly once after every hook, including when a
    /// before-hook or after-hook returns `Err` (finalize first; the hook error is
    /// still surfaced). Refuses until [`Engine::freeze`].
    #[allow(clippy::too_many_arguments)] // SPEC executor ABI: tx, sink, doc, edge, token, gate, ctx
    pub async fn transition(
        &self,
        tx: &mut Tx<'_>,
        mut sink: Box<dyn PostingSink>,
        doc: &DocRef,
        edge_name: &str,
        token: Option<&SignatureToken>,
        gate: &dyn SignatureGate,
        ctx: &WriteContext,
    ) -> Result<Instance> {
        self.ensure_frozen()?;
        let edge = self.edge_for(&doc.doc_type, edge_name)?.clone();
        let expected = action_for(&doc.doc_type, edge_name);
        let bound = tx.setting("datum.action").await?;
        if bound != expected {
            return Err(Error::ActionMismatch {
                expected,
                actual: bound,
            });
        }

        let instance = load_instance(tx, doc).await?;
        if instance.state.0 != edge.from.0 {
            return Err(Error::InvalidState {
                expected: edge.from.0.clone(),
                actual: instance.state.0,
            });
        }

        // (a) permission
        let allowed =
            datum_identity::rbac::has_permission(tx, ctx.actor, &edge.permission.0).await?;
        if !allowed {
            return Err(Error::PermissionDenied {
                permission: edge.permission.0.clone(),
            });
        }

        // (b) signature verify BEFORE any mutation
        verify_signature(&edge, token, gate, doc, &instance)?;

        let view = HookView {
            doc_type: doc.doc_type.clone(),
            doc_id: doc.doc_id,
            edge: edge.name.clone(),
            from: edge.from.0.clone(),
            to: edge.to.0.clone(),
            version: instance.version,
            module_id: String::new(),
        };

        let hook_err = if edge.hooks_allowed {
            // (c) before_transition, topological order, one sink, time budget.
            // Do not `?` here: CONTRACT §6.2 rule 1 requires exactly one
            // `sink.finalize()` after every hook run, including a before-hook Err.
            self.run_hooks(HookPhase::Before, &view, sink.as_mut())
                .err()
        } else {
            None
        };
        if let Some(e) = hook_err {
            let _ = finalize_sink(sink);
            return Err(e);
        }

        // (d) mutate sm.instance — the only writer of the state column
        let updated = mutate_instance(tx, doc, &edge, instance.version).await?;

        let after_err = if edge.hooks_allowed {
            // (e) after_transition: same order, no veto power
            self.run_hooks(HookPhase::After, &view, sink.as_mut()).err()
        } else {
            None
        };

        // (f) exactly one finalize after every hook, including before-hook and
        // after-hook Err (CONTRACT §6.2 rule 1). Finalize first; the hook error
        // is still surfaced.
        let finalize_err = finalize_sink(sink).err();
        if let Some(e) = after_err {
            return Err(e);
        }
        if let Some(e) = finalize_err {
            return Err(e);
        }
        Ok(updated)
    }
}

/// CONTRACT §6.2: `pub struct NoPostings;   // contribute -> Err(NoSink); finalize -> Err(NoSink)`
///
/// Rule 1: exactly one `finalize` after every hook. NoPostings is "for tests and
/// for transitions that must not post"; `Err(NoSink)` is that defined outcome
/// (not a dropped sink) and maps to the transition's `Ok(())`.
fn finalize_sink(sink: Box<dyn PostingSink>) -> Result<()> {
    match sink.finalize() {
        Ok(()) | Err(PostingError::NoSink) => Ok(()),
        Err(e) => Err(Error::Posting(e)),
    }
}

fn verify_signature(
    edge: &Edge,
    token: Option<&SignatureToken>,
    gate: &dyn SignatureGate,
    doc: &DocRef,
    instance: &Instance,
) -> Result<()> {
    let SignatureDeclaration::Required(req) = &edge.signature else {
        return Ok(());
    };
    let Some(token) = token else {
        return Err(SignatureError::Invalid("missing token".into()).into());
    };
    let record = RecordRef {
        table: "sm.instance".into(),
        id: doc.doc_id,
        version: instance.version,
    };
    gate.verify(token, req, &record)?;
    Ok(())
}

async fn persist_machine(tx: &mut Tx<'_>, m: &Machine) -> Result<()> {
    tx.execute(
        sql_query(
            r#"INSERT INTO sm.machine (id, doc_type, regulated)
               VALUES ($1, $2, $3)
               ON CONFLICT (id) DO UPDATE
                 SET doc_type = EXCLUDED.doc_type,
                     regulated = EXCLUDED.regulated"#,
        )
        .bind(m.id.as_uuid())
        .bind(&m.doc_type)
        .bind(m.regulated),
    )
    .await?;
    for s in &m.states {
        tx.execute(
            sql_query(
                r#"INSERT INTO sm.state (machine_id, name)
                   VALUES ($1, $2)
                   ON CONFLICT (machine_id, name) DO NOTHING"#,
            )
            .bind(m.id.as_uuid())
            .bind(&s.0),
        )
        .await?;
    }
    for e in &m.edges {
        let (kind, meaning, sig_perm, reason) = match &e.signature {
            SignatureDeclaration::Required(req) => (
                "required",
                Some(req.meaning.0.as_str()),
                Some(req.permission.0.as_str()),
                None,
            ),
            SignatureDeclaration::NotRequired { reason } => {
                ("not_required", None, None, Some(*reason))
            }
        };
        tx.execute(
            sql_query(
                r#"INSERT INTO sm.edge (
                       machine_id, name, from_state, to_state, permission,
                       signature_kind, meaning, sig_permission, not_required_reason,
                       hooks_allowed
                   ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
                   ON CONFLICT (machine_id, name) DO UPDATE SET
                       from_state = EXCLUDED.from_state,
                       to_state = EXCLUDED.to_state,
                       permission = EXCLUDED.permission,
                       signature_kind = EXCLUDED.signature_kind,
                       meaning = EXCLUDED.meaning,
                       sig_permission = EXCLUDED.sig_permission,
                       not_required_reason = EXCLUDED.not_required_reason,
                       hooks_allowed = EXCLUDED.hooks_allowed"#,
            )
            .bind(m.id.as_uuid())
            .bind(&e.name)
            .bind(&e.from.0)
            .bind(&e.to.0)
            .bind(&e.permission.0)
            .bind(kind)
            .bind(meaning)
            .bind(sig_perm)
            .bind(reason)
            .bind(e.hooks_allowed),
        )
        .await?;
    }
    Ok(())
}

async fn load_instance(tx: &mut Tx<'_>, doc: &DocRef) -> Result<Instance> {
    let row: Option<(Uuid, String, i64, DateTime<Utc>)> = tx
        .fetch_optional(
            sql_query_as(
                r#"SELECT machine_id, state, version, entered_at
                     FROM sm.instance
                    WHERE doc_type = $1 AND doc_id = $2"#,
            )
            .bind(&doc.doc_type)
            .bind(doc.doc_id.as_uuid()),
        )
        .await?;
    let Some((machine_id, state, version, entered_at)) = row else {
        return Err(Error::InstanceNotFound {
            doc_type: doc.doc_type.clone(),
            doc_id: doc.doc_id.to_string(),
        });
    };
    Ok(Instance {
        doc_type: doc.doc_type.clone(),
        doc_id: doc.doc_id,
        machine_id: MachineId(datum_core::Identifier::from_uuid(machine_id)),
        state: State(state),
        version,
        entered_at,
    })
}

async fn mutate_instance(
    tx: &mut Tx<'_>,
    doc: &DocRef,
    edge: &Edge,
    version: i64,
) -> Result<Instance> {
    let row: Option<(Uuid, String, i64, DateTime<Utc>)> = tx
        .fetch_optional(
            sql_query_as(
                r#"UPDATE sm.instance
                      SET state = $1,
                          version = version + 1,
                          entered_at = now()
                    WHERE doc_type = $2
                      AND doc_id = $3
                      AND state = $4
                      AND version = $5
                RETURNING machine_id, state, version, entered_at"#,
            )
            .bind(&edge.to.0)
            .bind(&doc.doc_type)
            .bind(doc.doc_id.as_uuid())
            .bind(&edge.from.0)
            .bind(version),
        )
        .await?;
    let Some((machine_id, state, version, entered_at)) = row else {
        return Err(Error::InvalidState {
            expected: edge.from.0.clone(),
            actual: "lost-update-or-missing".into(),
        });
    };
    Ok(Instance {
        doc_type: doc.doc_type.clone(),
        doc_id: doc.doc_id,
        machine_id: MachineId(datum_core::Identifier::from_uuid(machine_id)),
        state: State(state),
        version,
        entered_at,
    })
}
