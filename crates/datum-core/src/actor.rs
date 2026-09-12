//! Actor of a mutation. The ledger never takes an actor from a hook input.

use crate::id::Identifier;
use serde::{Deserialize, Serialize};

/// Kind of attributable actor. Adding a variant is a kernel change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ActorKind {
    /// A human user.
    User,
    /// A named background principal; never "unknown".
    ServicePrincipal,
}

/// Who is performing a mutation. `id` is the identity-crate identifier; `kind` is how to treat it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Actor {
    /// Identity of the actor. Stamped onto the transaction by `datum-db`, not by hooks.
    pub id: Identifier,
    /// Whether this is a user or a service principal.
    pub kind: ActorKind,
}
