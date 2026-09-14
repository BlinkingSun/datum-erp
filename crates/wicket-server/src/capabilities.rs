//! One capability table (ADR 0010 / T-24).
//!
//! Assembled from kernel operations plus the **mounted** subset of first-party
//! `module.toml` routes. Unmounted module routes (item list, inventory issues,
//! …) stay off this table until T-30. Handlers stay hand-written; this module
//! names what is mounted.

/// Kind of capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityKind {
    /// Ordinary HTTP operation.
    Http,
    /// State-machine edge (`If-Match`, optional signature).
    Transition,
}

/// One row of the capability table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capability {
    /// OpenAPI `operationId` and handler bind key.
    pub id: &'static str,
    /// Kind.
    pub kind: CapabilityKind,
    /// HTTP method (`GET`, `POST`, `PUT`, `PATCH`, `DELETE`).
    pub method: &'static str,
    /// Path (`/api/v1/items/{id}`).
    pub path: &'static str,
    /// Manifest permission; empty means unauthenticated.
    pub permission: &'static str,
    /// First-party module id when this row is a mounted `[[routes]]` entry.
    pub module: Option<&'static str>,
    /// State-machine edge name when [`CapabilityKind::Transition`].
    pub edge: Option<&'static str>,
}

const fn kernel(
    id: &'static str,
    kind: CapabilityKind,
    method: &'static str,
    path: &'static str,
    permission: &'static str,
    edge: Option<&'static str>,
) -> Capability {
    Capability {
        id,
        kind,
        method,
        path,
        permission,
        module: None,
        edge,
    }
}

const fn module(
    id: &'static str,
    kind: CapabilityKind,
    method: &'static str,
    path: &'static str,
    permission: &'static str,
    module: &'static str,
    edge: Option<&'static str>,
) -> Capability {
    Capability {
        id,
        kind,
        method,
        path,
        permission,
        module: Some(module),
        edge,
    }
}

/// Kernel (and kernel-crate) operations. Not declared in `modules/*/module.toml`.
pub const KERNEL: &[Capability] = &[
    kernel("health", CapabilityKind::Http, "GET", "/health", "", None),
    kernel(
        "getOpenApi",
        CapabilityKind::Http,
        "GET",
        "/api/v1/openapi.json",
        "",
        None,
    ),
    kernel(
        "getValidationManifest",
        CapabilityKind::Http,
        "GET",
        "/api/v1/iq/manifest",
        "validation.manifest.read",
        None,
    ),
    kernel(
        "exportAudit",
        CapabilityKind::Http,
        "GET",
        "/api/v1/audit",
        "audit.export",
        None,
    ),
    kernel(
        "getNavigation",
        CapabilityKind::Http,
        "GET",
        "/api/v1/navigation",
        "identity.session",
        None,
    ),
    kernel(
        "login",
        CapabilityKind::Http,
        "POST",
        "/api/v1/identity/login",
        "",
        None,
    ),
    kernel(
        "logout",
        CapabilityKind::Http,
        "POST",
        "/api/v1/identity/logout",
        "identity.session",
        None,
    ),
    kernel(
        "approveCalibration",
        CapabilityKind::Transition,
        "POST",
        "/api/v1/calibration/certificates/{id}/approve",
        "calibration.approve",
        Some("approve"),
    ),
    kernel(
        "esignChallenge",
        CapabilityKind::Http,
        "POST",
        "/api/v1/esign/challenges",
        "identity.session",
        None,
    ),
    kernel(
        "esignMint",
        CapabilityKind::Http,
        "POST",
        "/api/v1/esign/signatures",
        "identity.session",
        None,
    ),
    kernel(
        "getEsignSignature",
        CapabilityKind::Http,
        "GET",
        "/api/v1/esign/signatures/{id}",
        "identity.session",
        None,
    ),
    kernel(
        "getEsignBundle",
        CapabilityKind::Http,
        "GET",
        "/api/v1/esign/signatures/{id}/bundle",
        "esign.bundle.read",
        None,
    ),
    kernel(
        "defineCustomField",
        CapabilityKind::Http,
        "POST",
        "/api/v1/customfields/definitions",
        "customfields.define",
        None,
    ),
    kernel(
        "listCustomFieldDefinitions",
        CapabilityKind::Http,
        "GET",
        "/api/v1/customfields/definitions",
        "customfields.view",
        None,
    ),
    kernel(
        "retireCustomField",
        CapabilityKind::Transition,
        "POST",
        "/api/v1/customfields/definitions/{id}/retire",
        "customfields.retire",
        Some("retire"),
    ),
    kernel(
        "setItemCustomFields",
        CapabilityKind::Http,
        "PUT",
        "/api/v1/items/{id}/custom-fields",
        "customfields.set",
        None,
    ),
    kernel(
        "getItemCustomFields",
        CapabilityKind::Http,
        "GET",
        "/api/v1/items/{id}/custom-fields",
        "customfields.view",
        None,
    ),
    kernel(
        "createDocument",
        CapabilityKind::Http,
        "POST",
        "/api/v1/documents",
        "documents.edit",
        None,
    ),
    kernel(
        "getDocument",
        CapabilityKind::Http,
        "GET",
        "/api/v1/documents/{id}",
        "documents.view",
        None,
    ),
    kernel(
        "createDocumentRevision",
        CapabilityKind::Http,
        "POST",
        "/api/v1/documents/{id}/revisions",
        "documents.edit",
        None,
    ),
    kernel(
        "submitDocument",
        CapabilityKind::Transition,
        "POST",
        "/api/v1/documents/{id}/submit",
        "documents.edit",
        Some("submit"),
    ),
    kernel(
        "approveDocument",
        CapabilityKind::Transition,
        "POST",
        "/api/v1/documents/{id}/approve",
        "documents.approve",
        Some("approve"),
    ),
    kernel(
        "listPrintTemplates",
        CapabilityKind::Http,
        "GET",
        "/api/v1/print/templates",
        "print.templates",
        None,
    ),
    kernel(
        "renderPrint",
        CapabilityKind::Http,
        "POST",
        "/api/v1/print/render",
        "print.render",
        None,
    ),
    kernel(
        "archivePrint",
        CapabilityKind::Http,
        "POST",
        "/api/v1/print/archive",
        "print.archive",
        None,
    ),
    kernel(
        "releaseFromQuarantine",
        CapabilityKind::Transition,
        "POST",
        "/api/v1/inventory/releases",
        "lots.release",
        Some("release"),
    ),
    kernel(
        "reverseIssue",
        CapabilityKind::Http,
        "POST",
        "/api/v1/inventory/reversals",
        "inventory.adjust",
        None,
    ),
];

/// Mounted first-party `[[routes]]`. Path+method exists in that module's toml.
pub const MODULE: &[Capability] = &[
    module(
        "createItem",
        CapabilityKind::Http,
        "POST",
        "/api/v1/items",
        "items.edit",
        "mod-items",
        None,
    ),
    module(
        "getItem",
        CapabilityKind::Http,
        "GET",
        "/api/v1/items/{id}",
        "items.view",
        "mod-items",
        None,
    ),
    module(
        "releaseItem",
        CapabilityKind::Transition,
        "POST",
        "/api/v1/items/{id}/release",
        "items.release",
        "mod-items",
        Some("release"),
    ),
    module(
        "createLocation",
        CapabilityKind::Http,
        "POST",
        "/api/v1/locations",
        "locations.edit",
        "mod-locations",
        None,
    ),
    module(
        "getLocation",
        CapabilityKind::Http,
        "GET",
        "/api/v1/locations/{id}",
        "locations.view",
        "mod-locations",
        None,
    ),
    module(
        "createLot",
        CapabilityKind::Http,
        "POST",
        "/api/v1/lots",
        "lots.edit",
        "mod-lots",
        None,
    ),
    module(
        "getLot",
        CapabilityKind::Http,
        "GET",
        "/api/v1/lots/{id}",
        "lots.view",
        "mod-lots",
        None,
    ),
    module(
        "setLotStatus",
        CapabilityKind::Transition,
        "POST",
        "/api/v1/lots/{id}/status",
        "lots.release",
        "mod-lots",
        Some("release"),
    ),
    module(
        "listPackages",
        CapabilityKind::Http,
        "GET",
        "/api/v1/lots/{id}/packages",
        "lots.view",
        "mod-lots",
        None,
    ),
    module(
        "createPackage",
        CapabilityKind::Http,
        "POST",
        "/api/v1/lots/{id}/packages",
        "lots.edit",
        "mod-lots",
        None,
    ),
    module(
        "listSerials",
        CapabilityKind::Http,
        "GET",
        "/api/v1/lots/{id}/serials",
        "lots.view",
        "mod-lots",
        None,
    ),
    module(
        "createReceipt",
        CapabilityKind::Http,
        "POST",
        "/api/v1/inventory/receipts",
        "inventory.receive",
        "mod-inventory",
        None,
    ),
    module(
        "createCount",
        CapabilityKind::Http,
        "POST",
        "/api/v1/inventory/counts",
        "inventory.count",
        "mod-inventory",
        None,
    ),
    module(
        "getOnHand",
        CapabilityKind::Http,
        "GET",
        "/api/v1/inventory/on-hand",
        "inventory.view",
        "mod-inventory",
        None,
    ),
    module(
        "createWorkOrder",
        CapabilityKind::Http,
        "POST",
        "/api/v1/work-orders",
        "production.create",
        "mod-production-min",
        None,
    ),
    module(
        "getWorkOrder",
        CapabilityKind::Http,
        "GET",
        "/api/v1/work-orders/{id}",
        "production.view",
        "mod-production-min",
        None,
    ),
    module(
        "releaseWorkOrder",
        CapabilityKind::Transition,
        "POST",
        "/api/v1/work-orders/{id}/release",
        "production.release",
        "mod-production-min",
        Some("release"),
    ),
    module(
        "issueWorkOrder",
        CapabilityKind::Transition,
        "POST",
        "/api/v1/work-orders/{id}/issue",
        "production.issue",
        "mod-production-min",
        Some("issue"),
    ),
    module(
        "completeWorkOrder",
        CapabilityKind::Transition,
        "POST",
        "/api/v1/work-orders/{id}/complete",
        "production.complete",
        "mod-production-min",
        Some("complete"),
    ),
    module(
        "traceGenealogy",
        CapabilityKind::Http,
        "GET",
        "/api/v1/genealogy/trace",
        "genealogy.view",
        "mod-genealogy",
        None,
    ),
];

/// The capability table: kernel operations then mounted module routes.
pub fn table() -> impl Iterator<Item = &'static Capability> {
    KERNEL.iter().chain(MODULE.iter())
}

/// Method+path pairs in table order, sorted (T-25 / former `mounted_operations`).
pub fn operations() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = table()
        .map(|c| (c.method.to_string(), c.path.to_string()))
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use wicket_module::compiled_in;

    #[test]
    fn ids_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for cap in table() {
            assert!(seen.insert(cap.id), "duplicate capability id {}", cap.id);
        }
    }

    #[test]
    fn method_path_pairs_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for cap in table() {
            assert!(
                seen.insert((cap.method, cap.path)),
                "duplicate {} {}",
                cap.method,
                cap.path
            );
        }
    }

    #[test]
    fn module_rows_exist_in_toml() {
        let catalog = compiled_in().expect("compiled_in");
        for cap in MODULE {
            let module_id = cap.module.expect("MODULE row has module");
            let m = catalog
                .iter()
                .find(|m| m.id == module_id)
                .unwrap_or_else(|| panic!("compiled_in missing {module_id}"));
            assert!(
                m.routes
                    .iter()
                    .any(|r| r.method == cap.method && r.path == cap.path),
                "{module_id} module.toml missing {} {}",
                cap.method,
                cap.path
            );
        }
    }

    #[test]
    fn table_has_the_mounted_count() {
        assert_eq!(
            KERNEL.len() + MODULE.len(),
            47,
            "keep in lockstep with the live mount set"
        );
    }
}
