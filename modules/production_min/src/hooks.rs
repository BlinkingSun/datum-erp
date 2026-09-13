//! Start (`production.issue`) registers a hook that contributes inventory issue
//! postings on the transition's bound [`PostingSink`] (R-2s-7).
//!
//! The plan itself is Tx-scoped in `wicket-mod-inventory` (keyed by
//! `pg_current_xact_id`), not a process-global map.

use wicket_mod_inventory::{contribute_wip_issue, take_wip_issue_plan};
use wicket_module::KernelBuilder;
use wicket_statemachine::Veto;

use crate::domain::DOC_TYPE;
use crate::error::Result;

/// Register the start hook on `production.issue`.
pub fn register(builder: &mut KernelBuilder) -> Result<()> {
    builder.register_hook("mod-production-min", DOC_TYPE, "issue", |view, sink| {
        let plan = take_wip_issue_plan(view.doc_id);
        if let Some(plan) = plan
            && !plan.lines.is_empty()
        {
            contribute_wip_issue(sink, &plan).map_err(|e| Veto {
                module: "mod-production-min".into(),
                reason: e.to_string(),
            })?;
        }
        Ok(())
    });
    Ok(())
}
