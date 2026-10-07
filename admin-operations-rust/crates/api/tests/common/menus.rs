//! Menu-domain seed helpers shared across `tests/menus.rs`.
//!
//! Menus are global master data (no company_id), so seeding is simpler
//! than persons — just name + date range. Recipe seeding is reused
//! from `common::recipes::seed_recipe`.
//!
//! Each `#[sqlx::test]` test gets a fresh DB, so unique-name
//! collisions aren't a concern — the helpers just panic on failure.

#![allow(dead_code)]

use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

/// Default start/end date pair used when the caller doesn't care.
/// Mirrors the TS `seedMenu` helper (June 2025 covers "today" for the
/// `getActiveMenuForDate` tests).
pub const BASE_START: NaiveDate = match NaiveDate::from_ymd_opt(2025, 6, 1) {
    Some(d) => d,
    None => unreachable!(),
};
pub const BASE_END: NaiveDate = match NaiveDate::from_ymd_opt(2025, 6, 30) {
    Some(d) => d,
    None => unreachable!(),
};

/// Insert an active menu with the given name and default date range.
/// Returns its id.
pub async fn seed_menu(pool: &PgPool, name: &str) -> Uuid {
    seed_menu_dates(pool, name, BASE_START, BASE_END, true).await
}

/// Insert an inactive menu with the given name and default date range.
/// Returns its id.
pub async fn seed_menu_inactive(pool: &PgPool, name: &str) -> Uuid {
    seed_menu_dates(pool, name, BASE_START, BASE_END, false).await
}

/// Insert a menu with explicit dates and active flag. Returns its id.
pub async fn seed_menu_dates(
    pool: &PgPool,
    name: &str,
    start_date: NaiveDate,
    end_date: NaiveDate,
    is_active: bool,
) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO menu (name, is_active, start_date, end_date) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(name)
    .bind(is_active)
    .bind(start_date)
    .bind(end_date)
    .fetch_one(pool)
    .await
    .expect("seed menu")
}

/// Insert a recipe. Duplicated from `common::recipes::seed_recipe` to
/// keep this file's dependency graph flat (same pattern as other
/// common/* files).
pub async fn seed_recipe(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO recipe (name, active) VALUES ($1, TRUE) RETURNING id",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("seed recipe")
}

/// Insert a menu_recipe slot directly. Returns its id. Useful for
/// read-side tests that don't want to go through the set-slot handler.
pub async fn seed_slot(
    pool: &PgPool,
    menu_id: Uuid,
    recipe_id: Uuid,
    cycle_week: i16,
    day_of_week: &str,
    meal_category: &str,
) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO menu_recipe (menu_id, recipe_id, cycle_week, day_of_week, meal_category) \
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(menu_id)
    .bind(recipe_id)
    .bind(cycle_week)
    .bind(day_of_week)
    .bind(meal_category)
    .fetch_one(pool)
    .await
    .expect("seed slot")
}
