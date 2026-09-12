//! Compile-time assertion that every frozen name exists with the stated signature.

#![allow(
    dead_code,
    unused_must_use,
    unused_crate_dependencies,
    clippy::no_effect,
    clippy::type_complexity
)]

use datum_core::*;
use rust_decimal::Decimal;
use std::cmp::Ordering;
use std::str::FromStr;

fn _identifier_family() {
    let _: fn() -> Identifier = Identifier::generate;
    let _: fn(uuid::Uuid) -> Identifier = Identifier::from_uuid;
    let _: fn(Identifier) -> uuid::Uuid = Identifier::as_uuid;
    let _: fn() -> ItemId = ItemId::generate;
    let _: fn(uuid::Uuid) -> ItemId = ItemId::from_uuid;
    let _: fn(ItemId) -> uuid::Uuid = ItemId::as_uuid;
    let _: fn() -> LotId = LotId::generate;
    let _: fn(uuid::Uuid) -> LotId = LotId::from_uuid;
    let _: fn(LotId) -> uuid::Uuid = LotId::as_uuid;
    let _: fn() -> SerialId = SerialId::generate;
    let _: fn(uuid::Uuid) -> SerialId = SerialId::from_uuid;
    let _: fn(SerialId) -> uuid::Uuid = SerialId::as_uuid;
    let _: fn() -> LocationId = LocationId::generate;
    let _: fn(uuid::Uuid) -> LocationId = LocationId::from_uuid;
    let _: fn(LocationId) -> uuid::Uuid = LocationId::as_uuid;
    let _: fn() -> UserId = UserId::generate;
    let _: fn(uuid::Uuid) -> UserId = UserId::from_uuid;
    let _: fn(UserId) -> uuid::Uuid = UserId::as_uuid;
    let _: fn() -> SignatureId = SignatureId::generate;
    let _: fn(uuid::Uuid) -> SignatureId = SignatureId::from_uuid;
    let _: fn(SignatureId) -> uuid::Uuid = SignatureId::as_uuid;
    let _: fn(&str) -> datum_core::Result<Identifier> = Identifier::from_str;
}

fn _units() {
    let _: fn(UnitId, DimensionKind) -> core::result::Result<UnitRef<CountDim>, QuantityError> =
        UnitRef::<CountDim>::checked;
    let _: fn(UnitRef<CountDim>) -> UnitId = UnitRef::id;
    let _: fn(UnitRef<CountDim>) -> DimensionKind = UnitRef::kind;
}

fn _quantity() {
    let _: fn(
        Decimal,
        UnitRef<CountDim>,
    ) -> core::result::Result<Quantity<CountDim>, QuantityError> = Quantity::new;
    let _: fn(UnitRef<CountDim>) -> Quantity<CountDim> = Quantity::zero;
    let _: fn(Quantity<CountDim>) -> Decimal = Quantity::amount;
    let _: fn(Quantity<CountDim>) -> UnitRef<CountDim> = Quantity::unit;
    let _: fn(Quantity<CountDim>) -> UnitId = Quantity::unit_id;
    let _: fn(Quantity<CountDim>) -> DimensionKind = Quantity::kind;
    let _: fn(Quantity<CountDim>) -> bool = Quantity::is_zero;
    let _: fn(Quantity<CountDim>) -> i8 = Quantity::signum;
    let _: fn(
        Quantity<CountDim>,
        Quantity<CountDim>,
    ) -> core::result::Result<Quantity<CountDim>, QuantityError> = Quantity::try_add;
    let _: fn(
        Quantity<CountDim>,
        Quantity<CountDim>,
    ) -> core::result::Result<Quantity<CountDim>, QuantityError> = Quantity::try_sub;
    let _: fn(
        &mut Quantity<CountDim>,
        Quantity<CountDim>,
    ) -> core::result::Result<(), QuantityError> = Quantity::try_add_assign;
    let _: fn(Quantity<CountDim>) -> Quantity<CountDim> = Quantity::negate;
    let _: fn(Quantity<CountDim>) -> Quantity<CountDim> = Quantity::abs;
    let _: fn(
        Quantity<CountDim>,
        Quantity<CountDim>,
    ) -> core::result::Result<Ordering, QuantityError> = Quantity::try_cmp;
    let _: fn(
        Quantity<CountDim>,
        Decimal,
    ) -> core::result::Result<Quantity<CountDim>, QuantityError> = Quantity::try_scale_exact;
    let _: fn(Quantity<CountDim>, Decimal, u32) -> Scaled<CountDim> = Quantity::scale;
    let _: fn(
        Quantity<CountDim>,
        Quantity<CountDim>,
    ) -> core::result::Result<Decimal, QuantityError> = Quantity::try_ratio;
    let _: fn(
        Vec<Quantity<CountDim>>,
    ) -> core::result::Result<Option<Quantity<CountDim>>, QuantityError> = Quantity::try_sum;
}

fn _any_quantity() {
    let _: fn(Quantity<CountDim>) -> AnyQuantity = From::from;
    let _: fn(AnyQuantity) -> core::result::Result<Quantity<CountDim>, QuantityError> =
        AnyQuantity::downcast;
    let _: fn(AnyQuantity, AnyQuantity) -> core::result::Result<AnyQuantity, QuantityError> =
        AnyQuantity::try_add;
    let _: fn(Vec<AnyQuantity>) -> core::result::Result<Option<AnyQuantity>, QuantityError> =
        AnyQuantity::try_sum;
}

fn _money() {
    let _: fn(Decimal, CurrencyId) -> core::result::Result<Money, MoneyError> = Money::new;
    let _: fn(CurrencyId) -> Money = Money::zero;
    let _: fn(Money) -> Decimal = Money::amount;
    let _: fn(Money) -> CurrencyId = Money::currency;
    let _: fn(Money, Money) -> core::result::Result<Money, MoneyError> = Money::try_add;
    let _: fn(Money, Money) -> core::result::Result<Money, MoneyError> = Money::try_sub;
    let _: fn(Money) -> Money = Money::negate;
    let _: fn(Money, Money) -> core::result::Result<Ordering, MoneyError> = Money::try_cmp;
    let _: fn(Vec<Money>) -> core::result::Result<Option<Money>, MoneyError> = Money::try_sum;
    let _: fn(Money, u32, Rounding) -> Settled = Money::settle;
    let _: fn(Money, &[u64], u32) -> core::result::Result<Vec<Money>, MoneyError> = Money::allocate;
    let _: fn(
        Decimal,
        CurrencyId,
        UnitRef<CountDim>,
    ) -> core::result::Result<UnitCost<CountDim>, MoneyError> = UnitCost::new;
    let _: fn(
        UnitCost<CountDim>,
        Quantity<CountDim>,
        u32,
    ) -> core::result::Result<Extended, MoneyError> = UnitCost::extend;
    let _: fn(Money) -> MoneyWire = From::from;
    let _: fn(MoneyWire) -> core::result::Result<Money, MoneyError> = TryFrom::try_from;
}

fn _residual() {
    let _: fn(Converted<CountDim>) -> core::result::Result<Quantity<CountDim>, ResidualError> =
        Converted::into_exact;
    let _: fn(Converted<CountDim>, Rounding) -> (Quantity<CountDim>, Quantity<CountDim>) =
        Converted::split;
    let _: fn(Converted<CountDim>) -> bool = Converted::has_residual;
    let _: fn(Converted<CountDim>) -> Decimal = Converted::peek_residual;
    let _: fn(Scaled<CountDim>) -> core::result::Result<Quantity<CountDim>, ResidualError> =
        Scaled::into_exact;
    let _: fn(Scaled<CountDim>, Rounding) -> (Quantity<CountDim>, Quantity<CountDim>) =
        Scaled::split;
    let _: fn(Scaled<CountDim>) -> bool = Scaled::has_residual;
    let _: fn(Scaled<CountDim>) -> Decimal = Scaled::peek_residual;
    let _: fn(Settled) -> core::result::Result<Money, ResidualError> = Settled::into_exact;
    let _: fn(Settled) -> (Money, Money) = Settled::split;
    let _: fn(Settled) -> bool = Settled::has_residual;
    let _: fn(Settled) -> Decimal = Settled::peek_residual;
    let _: fn(Extended) -> core::result::Result<Money, ResidualError> = Extended::into_exact;
    let _: fn(Extended, Rounding) -> (Money, Money) = Extended::split;
    let _: fn(Extended) -> bool = Extended::has_residual;
    let _: fn(Extended) -> Decimal = Extended::peek_residual;
}

fn _convert<C: UnitCatalog + UnitConverter>() {
    let _: fn(&C, UnitId) -> core::result::Result<DimensionKind, QuantityError> =
        UnitCatalog::dimension_of;
    let _: fn(&C, UnitId, &ConversionContext) -> core::result::Result<u32, QuantityError> =
        UnitCatalog::scale_of;
    let _: fn(&C, UnitId) -> core::result::Result<UnitRef<CountDim>, QuantityError> =
        UnitCatalog::resolve;
    let _: fn(
        &C,
        Quantity<CountDim>,
        UnitRef<CountDim>,
        &ConversionContext,
    ) -> core::result::Result<Converted<CountDim>, QuantityError> = UnitConverter::convert;
}

fn _posting_sink<S: PostingSink + ?Sized>() {
    let _: fn(&S) -> GroupKind = PostingSink::kind;
    let _: fn(&S) -> &PostingGroupHeader = PostingSink::header;
    let _: fn(&mut S, PostingIntent) -> core::result::Result<PostingHandle, PostingError> =
        PostingSink::contribute;
    let _: fn(Box<S>) -> core::result::Result<(), PostingError> = PostingSink::finalize;
}

fn _signature_gate<G: SignatureGate + ?Sized>() {
    let _: fn(
        &G,
        &SignatureToken,
        &SignatureRequirement,
        &RecordRef,
    ) -> core::result::Result<(), SignatureError> = SignatureGate::verify;
}

fn _posting_error_variants() {
    let _ = PostingError::NoSink;
    let _ = PostingError::Shape(String::new());
    let _ = PostingError::UnknownHandle(PostingHandle(0));
    let _ = PostingError::AfterFinalize;
    let _ = PostingError::EmptyGroup;
    let _ = PostingError::AllocationRequired(PostingHandle(0));
    let _ = PostingError::AllocationMismatch(PostingHandle(0));
    let _ = PostingError::IneligibleLayer {
        consuming: PostingHandle(0),
        consumed: PostingId(0),
    };
    let _ = PostingError::LineageRequired(PostingHandle(0));
    let _ = PostingError::Unfinalized;
    let _ = PostingError::Unimplemented;
}

fn _signature_error_variants() {
    let _ = SignatureError::NoProvider;
    let _ = SignatureError::MeaningMismatch;
    let _ = SignatureError::RecordMismatch;
    let _ = SignatureError::HashMismatch;
    let _ = SignatureError::SignerNotPermitted;
    let _ = SignatureError::Consumed;
    let _ = SignatureError::Invalid(String::new());
    let _ = SignatureError::Unimplemented;
}

fn _error_variants() {
    let _ = Error::Invariant(String::new());
    let _ = Error::Overflow;
    let _ = Error::Quantity(QuantityError::Overflow);
    let _ = Error::Money(MoneyError::Overflow);
    let _ = Error::Residual(ResidualError::NotExact {
        residual: Decimal::ZERO,
        unit: UnitId(0),
    });
    let _ = Error::Posting(PostingError::NoSink);
    let _ = Error::Signature(SignatureError::NoProvider);
    let _ = Error::Unimplemented;
}

fn _quantity_error_variants() {
    let _ = QuantityError::DimensionMismatch {
        unit: UnitId(0),
        expected: DimensionKind::Count,
        actual: DimensionKind::Mass,
    };
    let _ = QuantityError::UnitMismatch {
        left: UnitId(0),
        right: UnitId(1),
    };
    let _ = QuantityError::ScaleExceeded {
        found: 9,
        max: QUANTITY_MAX_SCALE,
    };
    let _ = QuantityError::Overflow;
    let _ = QuantityError::DivideByZero;
    let _ = QuantityError::Inexact;
    let _ = QuantityError::NoConversionPath {
        from: UnitId(0),
        to: UnitId(1),
        item: ItemId::from_uuid(uuid::Uuid::nil()),
    };
    let _ = QuantityError::UnknownUnit(UnitId(0));
}

fn _money_error_variants() {
    let _ = MoneyError::CurrencyMismatch {
        left: CurrencyId(1),
        right: CurrencyId(2),
    };
    let _ = MoneyError::RateUnitMismatch {
        cost_unit: UnitId(0),
        qty_unit: UnitId(1),
    };
    let _ = MoneyError::ScaleExceeded {
        found: 7,
        max: MONEY_MAX_SCALE,
    };
    let _ = MoneyError::Overflow;
    let _ = MoneyError::EmptyAllocation;
}

fn _rounding_and_constants() {
    let _ = Rounding::HalfUp;
    let _ = Rounding::HalfEven;
    let _ = Rounding::TowardZero;
    let _ = Rounding::AwayFromZero;
    let _: u32 = QUANTITY_MAX_SCALE;
    let _: u32 = MONEY_MAX_SCALE;
    let _: u32 = RATE_MAX_SCALE;
    let _: fn() -> NoPostings = || NoPostings;
    let _: fn() -> NoSignatures = || NoSignatures;
}

fn _group_enums() {
    let _ = GroupKind::Movement;
    let _ = GroupKind::Adjustment;
    let _ = GroupKind::Transformation;
    let _ = GroupKind::Valuation;
    let _ = GroupKind::Reversal;
    let _ = Boundary::Supplier;
    let _ = Boundary::Customer;
    let _ = Boundary::Scrap;
    let _ = Boundary::Adjustment;
    let _ = Boundary::Rounding;
    let _ = Boundary::Consumed;
    let _ = Boundary::Produced;
    let _ = CostElement::Material;
    let _ = CostElement::Labor;
    let _ = CostElement::Labor;
    let _ = CostElement::Burden;
    let _ = CostElement::Outside;
    let _ = ValueAccount::Inventory;
    let _ = ValueAccount::Wip;
    let _ = ValueAccount::Cogs;
    let _ = ValueAccount::ScrapExpense;
    let _ = ValueAccount::AdjustmentExpense;
    let _ = ValueAccount::ApAccrual;
    let _ = ValueAccount::LaborAbsorbed;
    let _ = ValueAccount::BurdenAbsorbed;
    let _ = ValueAccount::Ppv;
    let _ = ValueAccount::MfgVariance;
    let _ = ValueAccount::Rounding;
    let _ = ActorKind::User;
    let _ = ActorKind::ServicePrincipal;
}

struct NoCatalog;
impl UnitCatalog for NoCatalog {
    fn dimension_of(&self, _unit: UnitId) -> core::result::Result<DimensionKind, QuantityError> {
        Err(QuantityError::UnknownUnit(UnitId(0)))
    }
    fn scale_of(
        &self,
        _unit: UnitId,
        _ctx: &ConversionContext,
    ) -> core::result::Result<u32, QuantityError> {
        Err(QuantityError::UnknownUnit(UnitId(0)))
    }
}
impl UnitConverter for NoCatalog {
    fn convert<D: Dimension>(
        &self,
        _qty: Quantity<D>,
        _to: UnitRef<D>,
        _ctx: &ConversionContext,
    ) -> core::result::Result<Converted<D>, QuantityError> {
        Err(QuantityError::UnknownUnit(UnitId(0)))
    }
}

#[test]
fn api_surface_compiles() {
    _identifier_family();
    _units();
    _quantity();
    _any_quantity();
    _money();
    _residual();
    _convert::<NoCatalog>();
    _posting_sink::<NoPostings>();
    _posting_sink::<dyn PostingSink>();
    _signature_gate::<NoSignatures>();
    _signature_gate::<dyn SignatureGate>();
    _posting_error_variants();
    _signature_error_variants();
    _error_variants();
    _quantity_error_variants();
    _money_error_variants();
    _rounding_and_constants();
    _group_enums();
}
