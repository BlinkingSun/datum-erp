//! Rounding residual home (D2 §7 R4): the only path that posts `ADJUSTMENT`
//! with reason `UOM_CONVERSION_RESIDUAL`.

use crate::builder::{GroupBuilder, UOM_CONVERSION_RESIDUAL};
use crate::post::post;
use crate::registry::load_stock_item;
use crate::{Error, Result};
use wicket_core::{
    AnyQuantity, AreaDim, Boundary, ConversionContext, CostElement, CountDim, DimensionKind,
    GroupKind, Identifier, ItemId, LengthDim, LocationId, MassDim, Money, PostingGroupHeader,
    PostingIntent, PostingSink, QuantityPosting, TimeDim, ValueAccount, ValuePosting, VolumeDim,
};
use wicket_db::Tx;

/// Convert `entered` through [`wicket_uom::to_stock`] and, if a residual remains,
/// post an `ADJUSTMENT` group bounded by `residual_tolerance`.
pub async fn post_uom_conversion_residual(
    tx: &mut Tx<'_>,
    item: ItemId,
    location: LocationId,
    rounding_location: LocationId,
    entered: AnyQuantity,
    lot: Option<wicket_core::LotId>,
    inventory_amount: Money,
) -> Result<Option<Identifier>> {
    let catalog = wicket_uom::load_catalog(tx).await?;
    let ctx = ConversionContext { item, lot };
    let residual_qty = residual_of(tx, &catalog, item, entered, &ctx).await?;
    post_residual_adjustment(
        tx,
        ResidualAdj {
            item,
            location,
            rounding_location,
            residual_qty,
            entered: Some(entered),
            lot,
            inventory_amount,
        },
    )
    .await
}

/// Dust-report flush: `residual` is already in the item's stock unit.
///
/// This crate does not round `residual`. `quantity_exact_at_scale` and
/// `rounding_is_dust` are the database's gates. Used by D2 §8 case k (0.0057 FT).
pub async fn post_uom_residual_flush(
    tx: &mut Tx<'_>,
    item: ItemId,
    location: LocationId,
    rounding_location: LocationId,
    residual: AnyQuantity,
    lot: Option<wicket_core::LotId>,
    inventory_amount: Money,
) -> Result<Option<Identifier>> {
    post_residual_adjustment(
        tx,
        ResidualAdj {
            item,
            location,
            rounding_location,
            residual_qty: residual,
            entered: None,
            lot,
            inventory_amount,
        },
    )
    .await
}

struct ResidualAdj {
    item: ItemId,
    location: LocationId,
    rounding_location: LocationId,
    residual_qty: AnyQuantity,
    entered: Option<AnyQuantity>,
    lot: Option<wicket_core::LotId>,
    inventory_amount: Money,
}

async fn post_residual_adjustment(tx: &mut Tx<'_>, adj: ResidualAdj) -> Result<Option<Identifier>> {
    let ResidualAdj {
        item,
        location,
        rounding_location,
        residual_qty,
        entered,
        lot,
        inventory_amount,
    } = adj;
    let stock = load_stock_item(tx, item).await?;
    // Never round. A zero leftover posts nothing. A leftover that is not
    // already exact at `stock_scale` is left in the balance (D2 §7 R4); the
    // dust report later flushes an accumulated amount that *is* exact.
    if residual_qty.amount.is_zero() {
        return Ok(None);
    }
    let scale = u32::try_from(stock.stock_scale).unwrap_or(0);
    if residual_qty.amount.scale() > scale {
        return Ok(None);
    }
    if residual_qty.amount.abs() > stock.residual_tolerance {
        return Err(Error::from_posting(wicket_core::PostingError::Shape(
            "residual exceeds residual_tolerance; use a real reason code".into(),
        )));
    }

    let mut builder = GroupBuilder::new(
        GroupKind::Adjustment,
        PostingGroupHeader {
            source_kind: "uom_residual".into(),
            source_id: None,
            work_order_id: None,
            reason_code: Some(UOM_CONVERSION_RESIDUAL.into()),
            reverses_group_id: None,
        },
    );
    let withdrawal = QuantityPosting {
        item,
        quantity: AnyQuantity {
            amount: -residual_qty.amount.abs(),
            unit: residual_qty.unit,
            dimension: residual_qty.dimension,
        },
        location,
        boundary: None,
        lot,
        serial: None,
        entered,
    };
    let w = builder
        .contribute(PostingIntent::Quantity(withdrawal.clone()))
        .map_err(Error::from_posting)?;
    builder
        .contribute(PostingIntent::Quantity(QuantityPosting {
            item,
            quantity: AnyQuantity {
                amount: residual_qty.amount.abs(),
                unit: residual_qty.unit,
                dimension: residual_qty.dimension,
            },
            location: rounding_location,
            boundary: Some(Boundary::Rounding),
            lot,
            serial: None,
            entered,
        }))
        .map_err(Error::from_posting)?;

    let layers = crate::allocate::load_open_layers(tx, item, location).await?;
    let (edges, extra) = crate::allocate::allocate_withdrawal(w, &withdrawal, &stock, &layers)
        .map_err(Error::from_posting)?;
    let allocated = Money::try_sum(edges.iter().map(|e| e.amount))
        .map_err(wicket_core::Error::from)?
        .unwrap_or_else(|| Money::zero(inventory_amount.currency()));
    let _ = extra;
    if !allocated.amount().is_zero() {
        builder
            .contribute(PostingIntent::Value(ValuePosting {
                account: ValueAccount::Inventory,
                cost_element: CostElement::Material,
                cost_object: None,
                amount: allocated.negate(),
                values: Some(w),
            }))
            .map_err(Error::from_posting)?;
        builder
            .contribute(PostingIntent::Value(ValuePosting {
                account: ValueAccount::Rounding,
                cost_element: CostElement::Material,
                cost_object: None,
                amount: allocated,
                values: None,
            }))
            .map_err(Error::from_posting)?;
    }
    for e in edges {
        builder
            .contribute(PostingIntent::Consumption(
                wicket_core::ConsumptionPosting {
                    consuming: w,
                    consumed_posting_id: e.consumed,
                    quantity: e.quantity,
                    amount: e.amount,
                },
            ))
            .map_err(Error::from_posting)?;
    }

    Ok(Some(post(tx, builder).await?))
}

async fn residual_of(
    tx: &mut Tx<'_>,
    catalog: &wicket_uom::UomCatalog,
    item: ItemId,
    entered: AnyQuantity,
    ctx: &ConversionContext,
) -> Result<AnyQuantity> {
    match entered.dimension {
        DimensionKind::Count => {
            pack(wicket_uom::to_stock::<CountDim>(tx, catalog, item, entered, ctx).await?)
        }
        DimensionKind::Length => {
            pack(wicket_uom::to_stock::<LengthDim>(tx, catalog, item, entered, ctx).await?)
        }
        DimensionKind::Mass => {
            pack(wicket_uom::to_stock::<MassDim>(tx, catalog, item, entered, ctx).await?)
        }
        DimensionKind::Time => {
            pack(wicket_uom::to_stock::<TimeDim>(tx, catalog, item, entered, ctx).await?)
        }
        DimensionKind::Volume => {
            pack(wicket_uom::to_stock::<VolumeDim>(tx, catalog, item, entered, ctx).await?)
        }
        DimensionKind::Area => {
            pack(wicket_uom::to_stock::<AreaDim>(tx, catalog, item, entered, ctx).await?)
        }
        _ => Err(Error::from_posting(wicket_core::PostingError::Shape(
            "unknown dimension".into(),
        ))),
    }
}

fn pack<D: wicket_core::Dimension>(conv: wicket_uom::StockConversion<D>) -> Result<AnyQuantity> {
    Ok(AnyQuantity::from(conv.residual))
}
