//! Approval machine: `Draft → InReview → Approved → Effective → Superseded | Obsolete`.

use wicket_core::{PermissionKey, SignatureMeaning, SignatureRequirement};
use wicket_statemachine::{EdgeBuilder, Machine};

use crate::domain::DOC_TYPE;
use crate::error::{Error, Result};

/// Build the document machine for an installation profile.
///
/// Under `regulated-device`, `approve` and `make_effective` are `Required`
/// (meanings `Approved` and `Responsible`). Under `plain-shop` every edge is
/// `NotRequired`. The composition root registers the returned [`Machine`].
pub fn document_machine(profile: &str) -> Result<Machine> {
    let regulated = match profile {
        "regulated-device" => true,
        "plain-shop" => false,
        other => return Err(Error::UnknownProfile(other.to_owned())),
    };

    let submit = EdgeBuilder::new("Draft", "InReview", "submit", "documents.edit")
        .not_required("editorial submit is not a quality decision");
    let approve = EdgeBuilder::new("InReview", "Approved", "approve", "documents.approve");
    let approve = if regulated {
        approve.required(SignatureRequirement {
            meaning: SignatureMeaning("Approved".into()),
            permission: PermissionKey("documents.approve".into()),
        })
    } else {
        approve.not_required("plain-shop; no signature on approve")
    };
    let make_effective = EdgeBuilder::new(
        "Approved",
        "Effective",
        "make_effective",
        "documents.release",
    );
    let make_effective = if regulated {
        make_effective.required(SignatureRequirement {
            meaning: SignatureMeaning("Responsible".into()),
            permission: PermissionKey("documents.release".into()),
        })
    } else {
        make_effective.not_required("plain-shop; no signature on make_effective")
    };
    let revise = EdgeBuilder::new("Effective", "Draft", "revise", "documents.edit")
        .not_required("opening a successor revision is editorial");
    let supersede = EdgeBuilder::new("Effective", "Superseded", "supersede", "documents.release")
        .not_required("supersession is recorded by the successor, not a quality approval");
    let obsolete = EdgeBuilder::new("Effective", "Obsolete", "obsolete", "documents.release")
        .not_required("obsolescence is a retention action, not an approval");
    let void = EdgeBuilder::new("Draft", "Void", "void", "documents.edit")
        .not_required("void keeps the number; it is not a quality approval");

    Machine::builder(DOC_TYPE)
        .regulated(regulated)
        .state("Draft")
        .state("InReview")
        .state("Approved")
        .state("Effective")
        .state("Superseded")
        .state("Obsolete")
        .state("Void")
        .edge(submit)
        .edge(approve)
        .edge(make_effective)
        .edge(revise)
        .edge(supersede)
        .edge(obsolete)
        .edge(void)
        .build()
        .map_err(Error::from)
}
