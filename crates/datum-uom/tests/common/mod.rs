#![allow(dead_code)]

use datum_core::{Actor, ActorKind, Identifier, ItemId, UnitId};
use datum_db::WriteContext;

pub fn write_ctx(action: &str) -> WriteContext {
    let mut ctx = WriteContext::new(
        Actor {
            id: Identifier::generate(),
            kind: ActorKind::User,
        },
        action,
        "ui",
    );
    ctx.actor_display = Some("test".into());
    ctx.reason = Some("integration test".into());
    ctx
}

pub async fn migrate(db: &datum_test::TestDb) {
    datum_db::migrate::run(
        db.migrate_pool(),
        &[
            ("datum-db", &datum_db::MIGRATOR),
            ("datum-audit", &datum_audit::MIGRATOR),
            ("datum-uom", &datum_uom::MIGRATOR),
        ],
    )
    .await
    .expect("migrate on template clone");
}

pub async fn insert_item_stock(
    tx: &mut datum_db::Tx<'_>,
    item: ItemId,
    stock_unit: UnitId,
    scale: i16,
) {
    tx.execute(
        sqlx::query(
            "INSERT INTO uom.item_stock (item_id, stock_unit_id, stock_scale)
             VALUES ($1, $2, $3)",
        )
        .bind(item.as_uuid())
        .bind(stock_unit.0)
        .bind(scale),
    )
    .await
    .expect("item_stock");
}

pub fn pg_code(err: &datum_db::Error) -> String {
    match err {
        datum_db::Error::Sqlx(e) => e
            .as_database_error()
            .and_then(|d| d.code().map(|c| c.into_owned()))
            .unwrap_or_else(|| format!("{e}")),
        other => other.to_string(),
    }
}
