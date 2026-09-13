//! Start (`production.issue`) registers a hook that contributes inventory issue
//! postings on the transition's bound [`PostingSink`] (R-2s-7).

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use datum_core::Identifier;
use datum_mod_inventory::{WipIssuePlan, contribute_wip_issue};
use datum_module::KernelBuilder;
use datum_statemachine::Veto;

use crate::domain::DOC_TYPE;
use crate::error::Result;

static ISSUE_PLANS: LazyLock<Mutex<HashMap<Identifier, WipIssuePlan>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Stash a plan for the next `issue` transition on this work order.
pub(crate) fn stash_issue_plan(work_order: Identifier, plan: WipIssuePlan) {
    ISSUE_PLANS
        .lock()
        .expect("issue plan lock")
        .insert(work_order, plan);
}

/// Drop any stashed plan after the transition completes.
pub(crate) fn clear_issue_plan(work_order: Identifier) {
    ISSUE_PLANS
        .lock()
        .expect("issue plan lock")
        .remove(&work_order);
}

/// Register the start hook on `production.issue`.
pub fn register(builder: &mut KernelBuilder) -> Result<()> {
    builder.register_hook("mod-production-min", DOC_TYPE, "issue", |view, sink| {
        let plan = ISSUE_PLANS
            .lock()
            .expect("issue plan lock")
            .get(&view.doc_id)
            .cloned();
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
