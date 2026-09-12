//! Persistence. Every mutation runs inside [`datum_db::Tx`].

use chrono::{DateTime, NaiveDate, Utc};
use datum_core::{Actor, AnyQuantity, DimensionKind, Identifier, ItemId, LotId, SerialId, UnitId};
use datum_db::{Tx, WriteContext};
use datum_module::Kernel;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::domain::{
    CreateLot, DEFAULT_LOT_TEMPLATE, DEFAULT_SERIAL_TEMPLATE, Expiry, ExpiryPrecision, Lot,
    LotStatus, Package, PackageId, PackageLevel, Serial, StatusHistory, StatusTarget, UdiTarget,
    validate_identifier,
};
use crate::error::{Error, Result};
use crate::states::{doc_ref_lot, doc_ref_serial, edge_for_transition};
use crate::{events, stamps};

/// Create a lot. Generates a number from a template or validates a supplied one.
///
/// Expiry precision is mandatory with a date. Month precision stores the first of
/// the month. This module never posts inventory.
pub async fn create_lot(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    ctx: &WriteContext,
    spec: CreateLot,
) -> Result<Lot> {
    let number = match spec.number {
        Some(n) => {
            validate_identifier(&n)?;
            n
        }
        None => {
            let template = spec.template.as_deref().unwrap_or(DEFAULT_LOT_TEMPLATE);
            datum_numbering::lot::generate(tx, template).await?
        }
    };
    let (app_version, config_version) = stamps(tx).await?;
    let id = LotId::generate();
    let expiry_date = spec.expiry.map(|e| e.date);
    let expiry_precision = spec.expiry.map(|e| e.precision.as_str());
    tx.execute(
        sqlx::query(
            r#"INSERT INTO lots.lot (
                   id, item_id, number, supplier_lot, heat_or_source_ref,
                   received_at, expiry_date, expiry_precision, cert_ref, status,
                   udi_device_identifier, version, application_version, configuration_version
               ) VALUES (
                   $1, $2, $3, $4, $5,
                   now(), $6, $7, $8, $9,
                   NULL, 1, $10, $11
               )"#,
        )
        .bind(id.as_uuid())
        .bind(spec.item.as_uuid())
        .bind(&number)
        .bind(spec.supplier_lot.as_deref())
        .bind(spec.heat_or_source_ref.as_deref())
        .bind(expiry_date)
        .bind(expiry_precision)
        .bind(spec.cert_ref.as_deref())
        .bind(LotStatus::Quarantine.as_str())
        .bind(&app_version)
        .bind(&config_version),
    )
    .await?;
    kernel
        .spawn(tx, &doc_ref_lot(id), LotStatus::Quarantine.as_str())
        .await?;
    let event = events::lot_created(id, spec.item, &number)?;
    datum_events::publish(tx, event).await?;
    if spec.status != LotStatus::Quarantine {
        let _ = (kernel, ctx);
        return Err(Error::InvalidTransition {
            edge: "create".into(),
            status: spec.status.as_str().into(),
        });
    }
    load_lot_in(tx, id).await
}

/// Allocate `n` serials as units within `lot` (invariant 10).
pub async fn create_serials(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    lot: LotId,
    n: u32,
    template: Option<&str>,
) -> Result<Vec<Serial>> {
    let _ = load_lot_in(tx, lot).await?;
    let template = template.unwrap_or(DEFAULT_SERIAL_TEMPLATE);
    let (app_version, config_version) = stamps(tx).await?;
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let number = datum_numbering::serial::generate(tx, template).await?;
        let id = SerialId::generate();
        tx.execute(
            sqlx::query(
                r#"INSERT INTO lots.serial (
                       id, lot_id, number, status, udi_production_identifier,
                       version, application_version, configuration_version
                   ) VALUES ($1, $2, $3, $4, NULL, 1, $5, $6)"#,
            )
            .bind(id.as_uuid())
            .bind(lot.as_uuid())
            .bind(&number)
            .bind(LotStatus::Quarantine.as_str())
            .bind(&app_version)
            .bind(&config_version),
        )
        .await?;
        kernel
            .spawn(tx, &doc_ref_serial(id), LotStatus::Quarantine.as_str())
            .await?;
        out.push(Serial {
            id,
            lot,
            number,
            status: LotStatus::Quarantine,
            udi_production_identifier: None,
            version: 1,
        });
    }
    if !out.is_empty() {
        let ids: Vec<SerialId> = out.iter().map(|s| s.id).collect();
        let event = events::serials_created(lot, &ids)?;
        datum_events::publish(tx, event).await?;
    }
    Ok(out)
}

/// Record a status change through the registered lot state machine. The caller
/// posts the inventory movement; this module never posts.
pub async fn set_status(
    tx: &mut Tx<'_>,
    kernel: &Kernel,
    actor: Actor,
    target: StatusTarget,
    status: LotStatus,
    reason: &str,
) -> Result<StatusHistory> {
    let (from, doc) = match target {
        StatusTarget::Lot(id) => {
            let lot = load_lot_in(tx, id).await?;
            (lot.status, doc_ref_lot(id))
        }
        StatusTarget::Serial(id) => {
            let serial = load_serial_in(tx, id).await?;
            (serial.status, doc_ref_serial(id))
        }
    };
    if from == status {
        return Err(Error::InvalidTransition {
            edge: "noop".into(),
            status: from.as_str().into(),
        });
    }
    let edge = edge_for_transition(from, status).ok_or_else(|| Error::InvalidTransition {
        edge: format!("{}_to_{}", from.as_str(), status.as_str()),
        status: from.as_str().into(),
    })?;
    let mut ctx = kernel.transition_context(actor, &doc, edge);
    if ctx.config_version.is_none() {
        ctx.config_version = Some(kernel.profile.spec_version.clone());
    }
    kernel.transition(tx, &doc, edge, None, &ctx).await?;

    let (app_version, config_version) = stamps(tx).await?;
    let hid = Identifier::generate();
    match target {
        StatusTarget::Lot(id) => {
            tx.execute(
                sqlx::query(
                    r#"UPDATE lots.lot
                          SET status = $2, version = version + 1
                        WHERE id = $1"#,
                )
                .bind(id.as_uuid())
                .bind(status.as_str()),
            )
            .await?;
            tx.execute(
                sqlx::query(
                    r#"INSERT INTO lots.status_history (
                           id, lot_id, serial_id, from_status, to_status, reason,
                           recorded_at, application_version, configuration_version
                       ) VALUES ($1, $2, NULL, $3, $4, $5, now(), $6, $7)"#,
                )
                .bind(hid.as_uuid())
                .bind(id.as_uuid())
                .bind(from.as_str())
                .bind(status.as_str())
                .bind(reason)
                .bind(&app_version)
                .bind(&config_version),
            )
            .await?;
            let event = events::status_changed(Some(id), None, Some(from), status, reason)?;
            datum_events::publish(tx, event).await?;
            Ok(StatusHistory {
                id: hid,
                lot: Some(id),
                serial: None,
                from: Some(from),
                to: status,
                reason: reason.to_owned(),
            })
        }
        StatusTarget::Serial(id) => {
            tx.execute(
                sqlx::query(
                    r#"UPDATE lots.serial
                          SET status = $2, version = version + 1
                        WHERE id = $1"#,
                )
                .bind(id.as_uuid())
                .bind(status.as_str()),
            )
            .await?;
            tx.execute(
                sqlx::query(
                    r#"INSERT INTO lots.status_history (
                           id, lot_id, serial_id, from_status, to_status, reason,
                           recorded_at, application_version, configuration_version
                       ) VALUES ($1, NULL, $2, $3, $4, $5, now(), $6, $7)"#,
                )
                .bind(hid.as_uuid())
                .bind(id.as_uuid())
                .bind(from.as_str())
                .bind(status.as_str())
                .bind(reason)
                .bind(&app_version)
                .bind(&config_version),
            )
            .await?;
            let event = events::status_changed(None, Some(id), Some(from), status, reason)?;
            datum_events::publish(tx, event).await?;
            Ok(StatusHistory {
                id: hid,
                lot: None,
                serial: Some(id),
                from: Some(from),
                to: status,
                reason: reason.to_owned(),
            })
        }
    }
}

/// Create one package node (invariant 11). Parent, if any, must belong to `lot`.
pub async fn create_package(
    tx: &mut Tx<'_>,
    lot: LotId,
    parent: Option<PackageId>,
    level: PackageLevel,
    contained: AnyQuantity,
    label_ref: Option<&str>,
) -> Result<Package> {
    let _ = load_lot_in(tx, lot).await?;
    if let Some(parent_id) = parent {
        let parent_lot: Option<(Uuid,)> = tx
            .fetch_optional(
                sqlx::query_as("SELECT lot_id FROM lots.package WHERE id = $1")
                    .bind(parent_id.as_uuid()),
            )
            .await?;
        let Some((pl,)) = parent_lot else {
            return Err(Error::InvalidPackageParent);
        };
        if pl != lot.as_uuid() {
            return Err(Error::InvalidPackageParent);
        }
    }
    let (app_version, config_version) = stamps(tx).await?;
    let id = PackageId::generate();
    tx.execute(
        sqlx::query(
            r#"INSERT INTO lots.package (
                   id, lot_id, parent_id, level,
                   contained_amount, contained_unit, contained_dimension, label_ref,
                   application_version, configuration_version
               ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
        )
        .bind(id.as_uuid())
        .bind(lot.as_uuid())
        .bind(parent.map(PackageId::as_uuid))
        .bind(level.as_str())
        .bind(contained.amount)
        .bind(contained.unit.0)
        .bind(dimension_str(contained.dimension))
        .bind(label_ref)
        .bind(&app_version)
        .bind(&config_version),
    )
    .await?;
    Ok(Package {
        id,
        lot,
        parent,
        level,
        contained,
        label_ref: label_ref.map(str::to_owned),
    })
}

/// Package hierarchy for `lot` (parent links included).
pub async fn package_hierarchy(tx: &mut Tx<'_>, lot: LotId) -> Result<Vec<Package>> {
    let _ = load_lot_in(tx, lot).await?;
    let rows: Vec<PackageRow> = tx
        .fetch_all(
            sqlx::query_as(
                r#"SELECT id, lot_id, parent_id, level,
                          contained_amount, contained_unit, contained_dimension, label_ref
                     FROM lots.package
                    WHERE lot_id = $1
                    ORDER BY id"#,
            )
            .bind(lot.as_uuid()),
        )
        .await?;
    rows.into_iter().map(package_from_row).collect()
}

/// Write the UDI attachment columns. Nothing else.
pub async fn attach_udi(
    tx: &mut Tx<'_>,
    target: UdiTarget,
    di: Option<&str>,
    pi: Option<&str>,
) -> Result<()> {
    match target {
        UdiTarget::Lot(id) => {
            let n = tx
                .execute(
                    sqlx::query(
                        r#"UPDATE lots.lot
                              SET udi_device_identifier = $2, version = version + 1
                            WHERE id = $1"#,
                    )
                    .bind(id.as_uuid())
                    .bind(di),
                )
                .await?;
            if n.rows_affected() == 0 {
                return Err(Error::NotFound);
            }
            let _ = pi;
            Ok(())
        }
        UdiTarget::Serial(id) => {
            let n = tx
                .execute(
                    sqlx::query(
                        r#"UPDATE lots.serial
                              SET udi_production_identifier = $2, version = version + 1
                            WHERE id = $1"#,
                    )
                    .bind(id.as_uuid())
                    .bind(pi),
                )
                .await?;
            if n.rows_affected() == 0 {
                return Err(Error::NotFound);
            }
            let _ = di;
            Ok(())
        }
    }
}

/// Resolve a kernel lot number to its [`LotId`].
pub async fn resolve(tx: &mut Tx<'_>, number: &str) -> Result<LotId> {
    let row: Option<(Uuid,)> = tx
        .fetch_optional(sqlx::query_as("SELECT id FROM lots.lot WHERE number = $1").bind(number))
        .await?;
    match row {
        Some((id,)) => Ok(LotId::from_uuid(id)),
        None => Err(Error::UnknownNumber(number.to_owned())),
    }
}

/// Genealogy keys: the lot and every serial within it.
pub async fn trace_keys(tx: &mut Tx<'_>, lot: LotId) -> Result<(LotId, Vec<SerialId>)> {
    let _ = load_lot_in(tx, lot).await?;
    let rows: Vec<(Uuid,)> = tx
        .fetch_all(
            sqlx::query_as("SELECT id FROM lots.serial WHERE lot_id = $1 ORDER BY number")
                .bind(lot.as_uuid()),
        )
        .await?;
    Ok((
        lot,
        rows.into_iter()
            .map(|(id,)| SerialId::from_uuid(id))
            .collect(),
    ))
}

/// Load a lot by id.
pub async fn load_lot(tx: &mut Tx<'_>, id: LotId) -> Result<Lot> {
    load_lot_in(tx, id).await
}

/// Load a serial by id.
pub async fn load_serial(tx: &mut Tx<'_>, id: SerialId) -> Result<Serial> {
    load_serial_in(tx, id).await
}

/// Cursor-paginated serials for `lot` (id ascending).
pub async fn list_serials(
    tx: &mut Tx<'_>,
    lot: LotId,
    limit: i64,
    cursor: Option<SerialId>,
) -> Result<(Vec<Serial>, Option<SerialId>, bool)> {
    let _ = load_lot_in(tx, lot).await?;
    if !(1..=200).contains(&limit) {
        return Err(Error::InvalidLimit);
    }
    let fetch = limit + 1;
    let rows: Vec<SerialRow> = match cursor {
        Some(c) => {
            tx.fetch_all(
                sqlx::query_as(
                    r#"SELECT id, lot_id, number, status, udi_production_identifier, version
                         FROM lots.serial
                        WHERE lot_id = $1 AND id > $2
                        ORDER BY id
                        LIMIT $3"#,
                )
                .bind(lot.as_uuid())
                .bind(c.as_uuid())
                .bind(fetch),
            )
            .await?
        }
        None => {
            tx.fetch_all(
                sqlx::query_as(
                    r#"SELECT id, lot_id, number, status, udi_production_identifier, version
                         FROM lots.serial
                        WHERE lot_id = $1
                        ORDER BY id
                        LIMIT $2"#,
                )
                .bind(lot.as_uuid())
                .bind(fetch),
            )
            .await?
        }
    };
    let has_more = rows.len() as i64 > limit;
    let serials: Vec<Serial> = rows
        .into_iter()
        .take(limit as usize)
        .map(serial_from_row)
        .collect::<Result<Vec<_>>>()?;
    let next = if has_more {
        serials.last().map(|s| s.id)
    } else {
        None
    };
    Ok((serials, next, has_more))
}

/// Cursor-paginated packages for `lot` (id ascending).
pub async fn list_packages(
    tx: &mut Tx<'_>,
    lot: LotId,
    limit: i64,
    cursor: Option<PackageId>,
) -> Result<(Vec<Package>, Option<PackageId>, bool)> {
    let _ = load_lot_in(tx, lot).await?;
    if !(1..=200).contains(&limit) {
        return Err(Error::InvalidLimit);
    }
    let fetch = limit + 1;
    let rows: Vec<PackageRow> = match cursor {
        Some(c) => {
            tx.fetch_all(
                sqlx::query_as(
                    r#"SELECT id, lot_id, parent_id, level,
                              contained_amount, contained_unit, contained_dimension, label_ref
                         FROM lots.package
                        WHERE lot_id = $1 AND id > $2
                        ORDER BY id
                        LIMIT $3"#,
                )
                .bind(lot.as_uuid())
                .bind(c.as_uuid())
                .bind(fetch),
            )
            .await?
        }
        None => {
            tx.fetch_all(
                sqlx::query_as(
                    r#"SELECT id, lot_id, parent_id, level,
                              contained_amount, contained_unit, contained_dimension, label_ref
                         FROM lots.package
                        WHERE lot_id = $1
                        ORDER BY id
                        LIMIT $2"#,
                )
                .bind(lot.as_uuid())
                .bind(fetch),
            )
            .await?
        }
    };
    let has_more = rows.len() as i64 > limit;
    let packages: Vec<Package> = rows
        .into_iter()
        .take(limit as usize)
        .map(package_from_row)
        .collect::<Result<Vec<_>>>()?;
    let next = if has_more {
        packages.last().map(|p| p.id)
    } else {
        None
    };
    Ok((packages, next, has_more))
}

/// Cursor-paginated lot list (id ascending). `cursor` is the last seen id.
pub async fn list_lots(
    tx: &mut Tx<'_>,
    limit: i64,
    cursor: Option<LotId>,
) -> Result<(Vec<Lot>, Option<LotId>, bool)> {
    if !(1..=200).contains(&limit) {
        return Err(Error::InvalidLimit);
    }
    let fetch = limit + 1;
    let rows: Vec<LotRow> = match cursor {
        Some(c) => {
            tx.fetch_all(
                sqlx::query_as(
                    r#"SELECT id, item_id, number, supplier_lot, heat_or_source_ref,
                              received_at, expiry_date, expiry_precision, cert_ref, status,
                              udi_device_identifier, version
                         FROM lots.lot
                        WHERE id > $1
                        ORDER BY id
                        LIMIT $2"#,
                )
                .bind(c.as_uuid())
                .bind(fetch),
            )
            .await?
        }
        None => {
            tx.fetch_all(
                sqlx::query_as(
                    r#"SELECT id, item_id, number, supplier_lot, heat_or_source_ref,
                              received_at, expiry_date, expiry_precision, cert_ref, status,
                              udi_device_identifier, version
                         FROM lots.lot
                        ORDER BY id
                        LIMIT $1"#,
                )
                .bind(fetch),
            )
            .await?
        }
    };
    let has_more = rows.len() as i64 > limit;
    let lots: Vec<Lot> = rows
        .into_iter()
        .take(limit as usize)
        .map(lot_from_row)
        .collect::<Result<Vec<_>>>()?;
    let next = if has_more {
        lots.last().map(|l| l.id)
    } else {
        None
    };
    Ok((lots, next, has_more))
}

type PackageRow = (
    Uuid,
    Uuid,
    Option<Uuid>,
    String,
    Decimal,
    i64,
    String,
    Option<String>,
);

type SerialRow = (Uuid, Uuid, String, String, Option<String>, i64);

type LotRow = (
    Uuid,
    Uuid,
    String,
    Option<String>,
    Option<String>,
    DateTime<Utc>,
    Option<NaiveDate>,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
    i64,
);

async fn load_lot_in(tx: &mut Tx<'_>, id: LotId) -> Result<Lot> {
    let row: Option<LotRow> = tx
        .fetch_optional(
            sqlx::query_as(
                r#"SELECT id, item_id, number, supplier_lot, heat_or_source_ref,
                          received_at, expiry_date, expiry_precision, cert_ref, status,
                          udi_device_identifier, version
                     FROM lots.lot
                    WHERE id = $1"#,
            )
            .bind(id.as_uuid()),
        )
        .await?;
    match row {
        Some(r) => lot_from_row(r),
        None => Err(Error::NotFound),
    }
}

async fn load_serial_in(tx: &mut Tx<'_>, id: SerialId) -> Result<Serial> {
    let row: Option<SerialRow> = tx
        .fetch_optional(
            sqlx::query_as(
                r#"SELECT id, lot_id, number, status, udi_production_identifier, version
                     FROM lots.serial
                    WHERE id = $1"#,
            )
            .bind(id.as_uuid()),
        )
        .await?;
    match row {
        Some(r) => serial_from_row(r),
        None => Err(Error::NotFound),
    }
}

fn lot_from_row(r: LotRow) -> Result<Lot> {
    let (
        id,
        item,
        number,
        supplier_lot,
        heat,
        received_at,
        expiry_date,
        expiry_precision,
        cert_ref,
        status,
        udi,
        version,
    ) = r;
    Ok(Lot {
        id: LotId::from_uuid(id),
        item: ItemId::from_uuid(item),
        number,
        supplier_lot,
        heat_or_source_ref: heat,
        received_at,
        expiry: expiry_from_parts(expiry_date, expiry_precision)?,
        cert_ref,
        status: LotStatus::parse(&status)?,
        udi_device_identifier: udi,
        version,
    })
}

fn serial_from_row(r: SerialRow) -> Result<Serial> {
    let (id, lot, number, status, udi, version) = r;
    Ok(Serial {
        id: SerialId::from_uuid(id),
        lot: LotId::from_uuid(lot),
        number,
        status: LotStatus::parse(&status)?,
        udi_production_identifier: udi,
        version,
    })
}

fn package_from_row(r: PackageRow) -> Result<Package> {
    let (id, lot, parent, level, amount, unit, dimension, label_ref) = r;
    Ok(Package {
        id: PackageId::from_uuid(id),
        lot: LotId::from_uuid(lot),
        parent: parent.map(PackageId::from_uuid),
        level: PackageLevel::parse(&level)?,
        contained: AnyQuantity {
            amount,
            unit: UnitId(unit),
            dimension: parse_dimension(&dimension)?,
        },
        label_ref,
    })
}

fn expiry_from_parts(date: Option<NaiveDate>, precision: Option<String>) -> Result<Option<Expiry>> {
    match (date, precision) {
        (None, None) => Ok(None),
        (Some(date), Some(p)) => Ok(Some(Expiry {
            date,
            precision: ExpiryPrecision::parse(&p)?,
        })),
        (Some(_), None) => Err(Error::ExpiryPrecisionRequired),
        (None, Some(_)) => Err(Error::ExpiryDateRequired),
    }
}

fn dimension_str(d: DimensionKind) -> &'static str {
    match d {
        DimensionKind::Count => "Count",
        DimensionKind::Length => "Length",
        DimensionKind::Mass => "Mass",
        DimensionKind::Time => "Time",
        DimensionKind::Volume => "Volume",
        DimensionKind::Area => "Area",
        _ => "Count",
    }
}

fn parse_dimension(s: &str) -> Result<DimensionKind> {
    match s {
        "Count" => Ok(DimensionKind::Count),
        "Length" => Ok(DimensionKind::Length),
        "Mass" => Ok(DimensionKind::Mass),
        "Time" => Ok(DimensionKind::Time),
        "Volume" => Ok(DimensionKind::Volume),
        "Area" => Ok(DimensionKind::Area),
        other => Err(Error::Core(datum_core::Error::Invariant(format!(
            "unknown dimension {other}"
        )))),
    }
}
