//! In-memory registry: machines, hooks, module order, startup guard, manifest.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

use wicket_core::PostingSink;

use crate::decl::{Edge, Machine, ManifestEdge};
use crate::{Error, Result};

/// Before or after the instance mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookPhase {
    /// Runs after permission and signature checks, before `sm.instance` is mutated. May veto.
    Before,
    /// Runs after the mutation. Must not veto.
    After,
}

/// Structured veto from a before-transition hook (`docs/03` §3.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Veto {
    /// Registering module id.
    pub module: String,
    /// Why the hook refused the transition.
    pub reason: String,
}

/// Snapshot handed to a hook. Synchronous; no network; may contribute to the one sink.
#[derive(Debug, Clone)]
pub struct HookView {
    /// Document type.
    pub doc_type: String,
    /// Document id.
    pub doc_id: wicket_core::Identifier,
    /// Edge name.
    pub edge: String,
    /// State before the transition.
    pub from: String,
    /// State after the transition.
    pub to: String,
    /// Instance version the executor read (pre-mutate).
    pub version: i64,
    /// Module this invocation is running as.
    pub module_id: String,
}

/// One node of the module dependency graph supplied by the composition root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleNode {
    /// Module id. Ties in the topological order are broken by this string.
    pub id: String,
    /// Modules this one depends on (they run first).
    pub depends_on: Vec<String>,
}

type Handler =
    Arc<dyn Fn(&HookView, &mut dyn PostingSink) -> core::result::Result<(), Veto> + Send + Sync>;

pub(crate) struct RegisteredHook {
    pub(crate) module_id: String,
    pub(crate) doc_type: String,
    pub(crate) edge_name: String,
    pub(crate) phase: HookPhase,
    pub(crate) budget_ms: u64,
    pub(crate) handler: Handler,
}

/// Composition-root handle: declarations, hooks, and the executor's lookup tables.
pub struct Engine {
    pub(crate) machines: Vec<Machine>,
    pub(crate) hooks: Vec<RegisteredHook>,
    pub(crate) modules: Vec<ModuleNode>,
    pub(crate) order: Vec<String>,
    pub(crate) frozen: bool,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    /// Empty engine. The composition root registers machines and hooks, then
    /// supplies the module graph (or calls [`Engine::freeze`]).
    pub fn new() -> Self {
        Self {
            machines: Vec::new(),
            hooks: Vec::new(),
            modules: Vec::new(),
            order: Vec::new(),
            frozen: false,
        }
    }

    fn ensure_open(&self) -> Result<()> {
        if self.frozen {
            Err(Error::Frozen)
        } else {
            Ok(())
        }
    }

    /// `persist` / `spawn` / `transition` require a frozen registry: hook order
    /// is computed once at startup (SPEC deliverable 4).
    pub(crate) fn ensure_frozen(&self) -> Result<()> {
        if self.frozen {
            Ok(())
        } else {
            Err(Error::NotFrozen)
        }
    }

    /// Replace the module dependency graph and recompute hook order.
    pub fn set_module_graph(&mut self, modules: Vec<ModuleNode>) -> Result<()> {
        self.ensure_open()?;
        self.order = topological_order(&modules)?;
        self.modules = modules;
        Ok(())
    }

    /// Register a machine. A regulated machine with any undeclared edge is refused.
    pub fn register_machine(&mut self, machine: Machine) -> Result<()> {
        self.ensure_open()?;
        if self.machines.iter().any(|m| m.doc_type == machine.doc_type) {
            return Err(Error::Duplicate(machine.doc_type));
        }
        self.machines.push(machine);
        Ok(())
    }

    /// Register a hook. Order is dependency-topological over the graph, ties by module id.
    pub fn register_hook<F>(
        &mut self,
        module_id: impl Into<String>,
        doc_type: impl Into<String>,
        edge_name: impl Into<String>,
        phase: HookPhase,
        budget_ms: u64,
        handler: F,
    ) -> Result<()>
    where
        F: Fn(&HookView, &mut dyn PostingSink) -> core::result::Result<(), Veto>
            + Send
            + Sync
            + 'static,
    {
        self.ensure_open()?;
        let module_id = module_id.into();
        let doc_type = doc_type.into();
        let edge_name = edge_name.into();
        if !self.modules.is_empty() && !self.order.iter().any(|id| id == &module_id) {
            return Err(Error::UnknownModule(module_id));
        }
        let machine = self
            .machines
            .iter()
            .find(|m| m.doc_type == doc_type)
            .ok_or_else(|| Error::UnknownMachine(doc_type.clone()))?;
        let edge = machine
            .edges
            .iter()
            .find(|e| e.name == edge_name)
            .ok_or_else(|| Error::UnknownEdge {
                doc_type: doc_type.clone(),
                edge: edge_name.clone(),
            })?;
        if !edge.hooks_allowed {
            return Err(Error::HooksNotAllowed {
                doc_type,
                edge: edge_name,
            });
        }
        self.hooks.push(RegisteredHook {
            module_id,
            doc_type,
            edge_name,
            phase,
            budget_ms,
            handler: Arc::new(handler),
        });
        Ok(())
    }

    /// Freeze registration and pin hook order (composition-root startup).
    ///
    /// SPEC deliverable 4: hook order is computed once at startup from the
    /// module dependency graph. After freeze, registration is [`Error::Frozen`].
    /// [`Engine::persist`], [`Engine::spawn`], and [`Engine::transition`] refuse
    /// until frozen ([`Error::NotFrozen`]).
    pub fn freeze(&mut self) -> Result<()> {
        self.ensure_open()?;
        if self.order.is_empty() && !self.modules.is_empty() {
            self.order = topological_order(&self.modules)?;
        }
        if self.order.is_empty() {
            let ids: BTreeSet<String> = self.hooks.iter().map(|h| h.module_id.clone()).collect();
            self.order = ids.into_iter().collect();
        }
        self.frozen = true;
        Ok(())
    }

    /// Hook order for `(doc_type, edge)`: topological, ties by module id.
    pub fn hook_order(&self, doc_type: &str, edge: &str) -> Vec<String> {
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for id in &self.order {
            if self
                .hooks
                .iter()
                .any(|h| h.module_id == *id && h.doc_type == doc_type && h.edge_name == edge)
                && seen.insert(id.clone())
            {
                out.push(id.clone());
            }
        }
        let mut extra: Vec<String> = self
            .hooks
            .iter()
            .filter(|h| h.doc_type == doc_type && h.edge_name == edge)
            .map(|h| h.module_id.clone())
            .filter(|id| seen.insert(id.clone()))
            .collect();
        extra.sort();
        out.extend(extra);
        out
    }

    /// Every registered edge with its declaration, both kinds, reasons included.
    pub fn edges_for_manifest(&self) -> Vec<ManifestEdge> {
        let mut out = Vec::new();
        for m in &self.machines {
            for e in &m.edges {
                out.push(ManifestEdge::from_edge(&m.doc_type, e));
            }
        }
        out.sort_by(|a, b| (&a.doc_type, &a.edge).cmp(&(&b.doc_type, &b.edge)));
        out
    }

    /// Startup guard (D-W1-4 (c), D-W1-5 key 4).
    pub fn check_gate_binding(&self, gate_is_noop: bool, release: bool) -> Result<()> {
        check_gate_binding(&self.machines, gate_is_noop, release)
    }

    pub(crate) fn machine_for(&self, doc_type: &str) -> Result<&Machine> {
        self.machines
            .iter()
            .find(|m| m.doc_type == doc_type)
            .ok_or_else(|| Error::UnknownMachine(doc_type.to_owned()))
    }

    pub(crate) fn edge_for(&self, doc_type: &str, edge_name: &str) -> Result<&Edge> {
        let machine = self.machine_for(doc_type)?;
        machine
            .edges
            .iter()
            .find(|e| e.name == edge_name)
            .ok_or_else(|| Error::UnknownEdge {
                doc_type: doc_type.to_owned(),
                edge: edge_name.to_owned(),
            })
    }

    pub(crate) fn run_hooks(
        &self,
        phase: HookPhase,
        view: &HookView,
        sink: &mut dyn PostingSink,
    ) -> Result<()> {
        let order = self.hook_order(&view.doc_type, &view.edge);
        for module_id in order {
            for hook in &self.hooks {
                if hook.phase != phase
                    || hook.module_id != module_id
                    || hook.doc_type != view.doc_type
                    || hook.edge_name != view.edge
                {
                    continue;
                }
                let mut view = view.clone();
                view.module_id = module_id.clone();
                let start = Instant::now();
                let outcome = (hook.handler)(&view, sink);
                let elapsed = start.elapsed();
                if elapsed > Duration::from_millis(hook.budget_ms) {
                    return Err(Error::HookBudgetExceeded {
                        module: module_id,
                        budget_ms: hook.budget_ms,
                    });
                }
                match outcome {
                    Ok(()) => {}
                    Err(veto) if phase == HookPhase::After => {
                        return Err(Error::AfterHookCannotVeto {
                            module: veto.module,
                        });
                    }
                    Err(veto) => {
                        return Err(Error::Veto {
                            module: veto.module,
                            reason: veto.reason,
                        });
                    }
                }
            }
        }
        Ok(())
    }
}

/// Fails at startup when `release && gate_is_noop && any Required edge`.
pub fn check_gate_binding(machines: &[Machine], gate_is_noop: bool, release: bool) -> Result<()> {
    if !(release && gate_is_noop) {
        return Ok(());
    }
    for m in machines {
        for e in &m.edges {
            if matches!(e.signature, crate::decl::SignatureDeclaration::Required(_)) {
                return Err(Error::StartupGate {
                    doc_type: m.doc_type.clone(),
                    edge: e.name.clone(),
                });
            }
        }
    }
    Ok(())
}

fn topological_order(nodes: &[ModuleNode]) -> Result<Vec<String>> {
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    let mut indeg: HashMap<String, usize> = HashMap::new();
    for n in nodes {
        indeg.entry(n.id.clone()).or_insert(0);
        children.entry(n.id.clone()).or_default();
        for dep in &n.depends_on {
            indeg.entry(dep.clone()).or_insert(0);
            children.entry(dep.clone()).or_default();
            *indeg.entry(n.id.clone()).or_insert(0) += 1;
            children.entry(dep.clone()).or_default().push(n.id.clone());
        }
    }
    let mut ready: BTreeSet<String> = indeg
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(k, _)| k.clone())
        .collect();
    let mut out = Vec::new();
    while let Some(id) = ready.iter().next().cloned() {
        ready.remove(&id);
        out.push(id.clone());
        if let Some(chs) = children.get(&id) {
            for ch in chs {
                if let Some(d) = indeg.get_mut(ch) {
                    *d = d.saturating_sub(1);
                    if *d == 0 {
                        ready.insert(ch.clone());
                    }
                }
            }
        }
    }
    if out.len() != indeg.len() {
        return Err(Error::DependencyCycle);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topo_diamond_ties_by_id() {
        let nodes = vec![
            ModuleNode {
                id: "mod-a".into(),
                depends_on: vec![],
            },
            ModuleNode {
                id: "mod-c".into(),
                depends_on: vec!["mod-a".into()],
            },
            ModuleNode {
                id: "mod-b".into(),
                depends_on: vec!["mod-a".into()],
            },
            ModuleNode {
                id: "mod-d".into(),
                depends_on: vec!["mod-b".into(), "mod-c".into()],
            },
        ];
        let order = topological_order(&nodes).expect("acyclic");
        assert_eq!(
            order,
            vec![
                "mod-a".to_string(),
                "mod-b".to_string(),
                "mod-c".to_string(),
                "mod-d".to_string()
            ]
        );
    }

    #[test]
    fn topo_v_ties_by_id() {
        let nodes = vec![
            ModuleNode {
                id: "mod-a".into(),
                depends_on: vec![],
            },
            ModuleNode {
                id: "mod-c".into(),
                depends_on: vec!["mod-a".into()],
            },
            ModuleNode {
                id: "mod-b".into(),
                depends_on: vec!["mod-a".into()],
            },
        ];
        let order = topological_order(&nodes).expect("acyclic");
        assert_eq!(
            order,
            vec![
                "mod-a".to_string(),
                "mod-b".to_string(),
                "mod-c".to_string()
            ]
        );
    }
}
