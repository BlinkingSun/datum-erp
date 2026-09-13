//! Declarative state machines, uniformly audited, with a frozen hook ABI.
//!
//! # Hook ABI (module authors)
//!
//! - **Order.** Hooks registered against `(doc_type, edge)` run in dependency-topological
//!   order of the registering modules; ties break by module id (CONTRACT §6.2 rule 8).
//! - **Budget.** Each hook has a `budget_ms`. Overrun is [`Error::HookBudgetExceeded`],
//!   never a silent skip (`docs/03` §3.2).
//! - **Veto.** `before_transition` may return [`Veto { module, reason }`](Veto); the
//!   executor aborts and does not mutate. `after_transition` has no veto power
//!   ([`Error::AfterHookCannotVeto`]).
//! - **Sink.** One [`datum_core::PostingSink`] per transaction, handed to every hook as
//!   `&mut dyn PostingSink`. The executor calls `finalize` exactly once after every hook
//!   has run, including when a before-hook or after-hook returns `Err` (finalize first;
//!   the hook error is still surfaced). CONTRACT §6.2: `NoPostings` finalize is
//!   `Err(NoSink)` — a defined outcome mapped to transition `Ok(())`. The transition
//!   never commits; the caller's [`datum_db::Tx`] does.
//! - **Freeze.** Hook order is computed once at startup ([`Engine::freeze`]). Registration
//!   after freeze is [`Error::Frozen`]. `persist` / `spawn` / `transition` refuse until
//!   frozen ([`Error::NotFrozen`]). Catalog declarations are frozen after first persist:
//!   an identical re-persist is a no-op (existing machine id); a different declaration
//!   for the same `doc_type` is [`Error::MachineChanged`].
//! - **Signatures.** [`SignatureDeclaration`] lives here (no `Default`). The executor
//!   calls `gate.verify` on every `Required` edge before mutate and fails closed.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use datum_audit as _;

mod decl;
mod engine;
mod error;
mod exec;
mod query;

pub use decl::{
    DocRef, Edge, EdgeBuilder, Instance, Machine, MachineBuilder, MachineId, ManifestEdge,
    SignatureDeclaration, State, Transition, action_for, with_action,
};
pub use engine::{Engine, HookPhase, HookView, ModuleNode, Veto, check_gate_binding};
pub use error::{Error, Result};
pub use query::{
    current_state, current_state_on, instance_exists, instance_exists_on, machine_id_for,
    machine_id_for_on,
};

/// Embedded migrator (`placeholder` + `0001` + `0002_query_seam` + `0003_engine_write`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[cfg(test)]
mod tests {
    use super::*;
    use datum_core::{PermissionKey, SignatureMeaning, SignatureRequirement};
    use datum_module as _;
    use proptest::prelude::*;
    use rust_decimal as _;
    use tokio as _;
    use trybuild as _;

    #[test]
    fn unimplemented_formats() {
        assert!(!Error::Unimplemented.to_string().is_empty());
    }

    #[test]
    fn machine_changed_names_doc_type() {
        let err = Error::MachineChanged {
            doc_type: "wo".into(),
        };
        let text = err.to_string();
        assert!(text.contains("wo"), "got {text}");
        assert!(
            matches!(err, Error::MachineChanged { ref doc_type } if doc_type == "wo"),
            "got {err:?}"
        );
    }

    #[test]
    fn migrator_has_placeholder() {
        assert!(MIGRATOR.migrations.len() >= 4);
    }

    #[test]
    fn postgres_helper_is_callable() {
        let _ = datum_test::postgres_available();
    }

    #[test]
    fn regulated_machine_requires_total_declaration() {
        let err = Machine::builder("wo")
            .regulated(true)
            .state("Draft")
            .state("Released")
            .edge(EdgeBuilder::new(
                "Draft",
                "Released",
                "release",
                "wo.release",
            ))
            .build()
            .expect_err("missing declaration");
        assert!(
            matches!(
                err,
                Error::MissingSignatureDeclaration { ref edge, .. } if edge == "release"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn non_regulated_absence_means_none() {
        let m = Machine::builder("quote")
            .state("Open")
            .state("Won")
            .edge(EdgeBuilder::new("Open", "Won", "win", "quote.win"))
            .build()
            .expect("non-regulated");
        assert!(matches!(
            m.edges[0].signature,
            SignatureDeclaration::NotRequired { .. }
        ));
    }

    #[test]
    fn startup_fails_when_required_edge_meets_no_signatures_in_release() {
        let req = SignatureRequirement {
            meaning: SignatureMeaning("Released".into()),
            permission: PermissionKey("wo.release".into()),
        };
        let m = Machine::builder("wo")
            .regulated(true)
            .edge(EdgeBuilder::new("Draft", "Released", "release", "wo.release").required(req))
            .build()
            .expect("declared");
        let err = check_gate_binding(std::slice::from_ref(&m), true, true).expect_err("startup");
        assert!(matches!(err, Error::StartupGate { .. }), "got {err:?}");
        check_gate_binding(std::slice::from_ref(&m), true, false).expect("not release");
        check_gate_binding(std::slice::from_ref(&m), false, true).expect("real gate");
    }

    #[test]
    fn manifest_lists_both_kinds_with_reasons() {
        let req = SignatureRequirement {
            meaning: SignatureMeaning("Approved".into()),
            permission: PermissionKey("cal.approve".into()),
        };
        let m = Machine::builder("cal")
            .regulated(true)
            .edge(EdgeBuilder::new("Open", "Approved", "approve", "cal.approve").required(req))
            .edge(
                EdgeBuilder::new("Open", "Void", "void", "cal.void")
                    .not_required("void is not a quality decision"),
            )
            .build()
            .expect("ok");
        let mut eng = Engine::new();
        eng.register_machine(m).expect("reg");
        let list = eng.edges_for_manifest();
        assert_eq!(list.len(), 2);
        let approve = list.iter().find(|e| e.edge == "approve").unwrap();
        assert_eq!(approve.kind, "required");
        assert_eq!(approve.meaning.as_deref(), Some("Approved"));
        let void = list.iter().find(|e| e.edge == "void").unwrap();
        assert_eq!(void.kind, "not_required");
        assert_eq!(
            void.reason.as_deref(),
            Some("void is not a quality decision")
        );
    }

    proptest! {
        #[test]
        fn unimplemented_display_is_stable(_x in 0u8..4) {
            prop_assert!(!Error::Unimplemented.to_string().is_empty());
        }
    }
}
