//! Order-domain seed helpers shared across `tests/orders.rs`. Each
//! `#[sqlx::test]` test gets a fresh DB, so collisions aren't possible
//! and we never need to randomise names beyond "unique within the
//! test's scope."
//!
//! All helpers panic on failure — these are seed helpers for tests,
//! not production code; a seed failure is a bug in the test, not a
//! case to handle.

#![allow(dead_code)]

use chrono::{NaiveDate, TimeZone, Utc};
use db::queries::orders::{self, InsertOrderData, OrderRow};
use sqlx::PgPool;
use uuid::Uuid;

/// Default serving / shipment date pair used by `seed_order` when the
/// caller doesn't care. Mirrors the TS `BASE_SERVING` / `BASE_SHIPMENT`.
pub const BASE_SERVING: NaiveDate = match NaiveDate::from_ymd_opt(2025, 6, 15) {
    Some(d) => d,
    None => unreachable!(),
};
pub const BASE_SHIPMENT: NaiveDate = match NaiveDate::from_ymd_opt(2025, 6, 10) {
    Some(d) => d,
    None => unreachable!(),
};

/// Insert a company with the given unique name, return its id.
pub async fn seed_company(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>("INSERT INTO company (name) VALUES ($1) RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await
        .expect("seed company")
}

/// Insert a recipe with the given unique name, return its id.
pub async fn seed_recipe(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO recipe (name, active) VALUES ($1, TRUE) RETURNING id",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("seed recipe")
}

/// Produce a defaulted `InsertOrderData` tied to the given company —
/// mirrors the TS `seedOrder` helper's shape.
pub fn default_insert(company_id: Uuid) -> InsertOrderData {
    InsertOrderData {
        company_id,
        recipe_id: None,
        created_by: None,
        order_source: "admin_manual".to_string(),
        status: None,
        meal_category: None,
        amount: 10.0,
        portion_size: None,
        day: None,
        serving_date: BASE_SERVING,
        shipment_date: BASE_SHIPMENT,
        frozen: None,
        chilled: None,
        recurring: None,
        client: None,
    }
}

/// Convenience: seed a fresh company + default order in one shot.
pub async fn seed_order(pool: &PgPool, name_hint: &str) -> OrderRow {
    let cid = seed_company(pool, name_hint).await;
    orders::create(pool, default_insert(cid))
        .await
        .expect("seed order")
}

/// Suppress `clippy::unused` on chrono import above.
#[allow(dead_code)]
fn _assert_chrono() {
    let _ = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0);
}
