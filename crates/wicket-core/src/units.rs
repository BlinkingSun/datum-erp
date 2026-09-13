//! Dimension markers and the unforgeable [`UnitRef`] bridge.

use crate::quantity::QuantityError;
use serde::{Deserialize, Serialize};

/// Opaque catalog identifier. Core never interprets the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct UnitId(pub i64);

/// ISO 4217 numeric currency code; the catalog row lives in the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CurrencyId(pub i32);

/// The sealed kernel dimension set. Adding a variant is a kernel change and a
/// migration, never a customization. Modules cannot extend it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DimensionKind {
    /// Discrete count (each, dozen, …).
    Count,
    /// Linear measure.
    Length,
    /// Mass.
    Mass,
    /// Duration. Labor-hours may or may not live here; `TimeDim` exists either way.
    Time,
    /// Volume.
    Volume,
    /// Area.
    Area,
}

mod private {
    /// Seals [`super::Dimension`] so only this crate can add dimension types.
    pub trait Sealed {}
}

/// Compile-time witness for exactly one [`DimensionKind`].
pub trait Dimension: private::Sealed + Copy + core::fmt::Debug + 'static {
    /// The runtime tag that matches this witness.
    const KIND: DimensionKind;
}

macro_rules! dimension {
    ($t:ident => $k:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $t;
        impl private::Sealed for $t {}
        impl Dimension for $t {
            const KIND: DimensionKind = DimensionKind::$k;
        }
    };
}

dimension!(CountDim => Count, "Compile-time witness for [`DimensionKind::Count`].");
dimension!(LengthDim => Length, "Compile-time witness for [`DimensionKind::Length`].");
dimension!(MassDim => Mass, "Compile-time witness for [`DimensionKind::Mass`].");
dimension!(TimeDim => Time, "Compile-time witness for [`DimensionKind::Time`].");
dimension!(VolumeDim => Volume, "Compile-time witness for [`DimensionKind::Volume`].");
dimension!(AreaDim => Area, "Compile-time witness for [`DimensionKind::Area`].");

/// A `UnitId` that has been proven to belong to dimension `D`.
///
/// The only constructor is [`UnitRef::checked`]. A [`crate::Quantity<D>`] cannot be
/// built from a bare [`UnitId`], so "unit does not belong to its dimension" is
/// unrepresentable once construction has succeeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnitRef<D: Dimension> {
    id: UnitId,
    _d: core::marker::PhantomData<D>,
}

impl<D: Dimension> UnitRef<D> {
    /// `catalog_kind` is what the caller read from the unit master. Core performs the
    /// comparison; a caller cannot skip it, only lie about its own data.
    ///
    /// ```
    /// use wicket_core::{CountDim, DimensionKind, UnitId, UnitRef};
    /// let unit = UnitRef::<CountDim>::checked(UnitId(1), DimensionKind::Count)
    ///     .expect("catalog said count");
    /// assert_eq!(unit.kind(), DimensionKind::Count);
    /// assert!(UnitRef::<CountDim>::checked(UnitId(2), DimensionKind::Mass).is_err());
    /// ```
    pub fn checked(id: UnitId, catalog_kind: DimensionKind) -> Result<Self, QuantityError> {
        if catalog_kind == D::KIND {
            Ok(Self {
                id,
                _d: core::marker::PhantomData,
            })
        } else {
            Err(QuantityError::DimensionMismatch {
                unit: id,
                expected: D::KIND,
                actual: catalog_kind,
            })
        }
    }

    /// Catalog identifier this witness carries.
    pub fn id(self) -> UnitId {
        self.id
    }

    /// Dimension this witness was proven for.
    pub fn kind(self) -> DimensionKind {
        D::KIND
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quantity::QuantityError;

    #[test]
    fn unit_ref_checked_dimension_mismatch() {
        let err = UnitRef::<CountDim>::checked(UnitId(7), DimensionKind::Length).unwrap_err();
        assert_eq!(
            err,
            QuantityError::DimensionMismatch {
                unit: UnitId(7),
                expected: DimensionKind::Count,
                actual: DimensionKind::Length,
            }
        );
    }
}
