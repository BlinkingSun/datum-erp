//! Opaque uuid-v7 identifiers. Typed wrappers do not convert across kinds.

use crate::error::Error;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// Opaque uuid v7 newtype. `Copy`, `Eq`, `Hash`, `Ord`, `Serialize`, `Deserialize`, `Display`.
///
/// Serde is a hyphenated lowercase string. `Display` is the same form; [`FromStr`] parses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Identifier(Uuid);

impl Identifier {
    /// Mint a new uuid v7. Call this at the moment the identifier is allocated.
    pub fn generate() -> Self {
        Self(Uuid::now_v7())
    }

    /// Wrap an existing uuid (any version). Use [`Self::generate`] for new ids.
    pub fn from_uuid(u: Uuid) -> Self {
        Self(u)
    }

    /// Return the inner uuid.
    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.as_hyphenated())
    }
}

impl FromStr for Identifier {
    type Err = Error;

    fn from_str(s: &str) -> core::result::Result<Self, Error> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|e| Error::Invariant(e.to_string()))
    }
}

macro_rules! typed_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Identifier);

        impl $name {
            /// Mint a new uuid v7 of this identifier kind.
            pub fn generate() -> Self {
                Self(Identifier::generate())
            }

            /// Wrap an existing uuid. There is no conversion from another typed id.
            pub fn from_uuid(u: Uuid) -> Self {
                Self(Identifier::from_uuid(u))
            }

            /// Return the inner uuid.
            pub fn as_uuid(self) -> Uuid {
                self.0.as_uuid()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

typed_id!(ItemId, "Catalog item. Not a lot, serial, or location.");
typed_id!(LotId, "Lot (batch) identity. Not an item or serial.");
typed_id!(SerialId, "Serialized-unit identity. Not a lot or item.");
typed_id!(LocationId, "Stock location. Not an item.");
typed_id!(
    UserId,
    "Human user. Distinct from [`crate::Actor`]'s id space only by convention of the identity crate."
);
typed_id!(SignatureId, "Existing e-sign row. Never minted here.");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_display_fromstr_roundtrip() {
        let id = Identifier::generate();
        let s = id.to_string();
        assert_eq!(s, s.to_lowercase());
        assert!(s.contains('-'));
        let parsed: Identifier = s.parse().unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn identifier_fromstr_rejects_garbage() {
        let err = "not-a-uuid".parse::<Identifier>().unwrap_err();
        assert!(matches!(err, Error::Invariant(_)));
    }

    #[test]
    fn typed_ids_do_not_share_constructors_across_kinds() {
        let item = ItemId::generate();
        let lot = LotId::from_uuid(item.as_uuid());
        assert_eq!(item.as_uuid(), lot.as_uuid());
        // Distinct types: passing ItemId where LotId is required does not compile
        // (see compile-fail tests for the posting handle analog).
    }
}
