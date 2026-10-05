//! Ingredient-domain seed helpers shared across `tests/ingredients.rs`.
//! Mirror of `tests/common/recipes.rs` and `tests/common/companies.rs`.
//! Each `#[sqlx::test]` test gets a fresh DB, so unique-name collisions
//! aren't a concern — the helpers just panic on failure.

#![allow(dead_code)]

use sqlx::PgPool;
use uuid::Uuid;

/// Insert an active ingredient, return its id.
pub async fn seed_ingredient(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO ingredient (name, active) VALUES ($1, TRUE) RETURNING id",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("seed ingredient")
}

/// Insert an inactive ingredient directly. Useful for tests that assert
/// the `include_inactive` flag's behavior without first calling the
/// deactivate endpoint.
pub async fn seed_ingredient_inactive(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO ingredient (name, active) VALUES ($1, FALSE) RETURNING id",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("seed inactive ingredient")
}

/// Insert an active ingredient with an initial `cost_per_unit`. The
/// NUMERIC(10,4) column is driven by a float literal via `::float8::numeric`
/// to avoid pulling in `rust_decimal` on the test side.
pub async fn seed_ingredient_with_cost(pool: &PgPool, name: &str, cost: f64) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO ingredient (name, active, cost_per_unit) \
         VALUES ($1, TRUE, $2::float8::numeric) RETURNING id",
    )
    .bind(name)
    .bind(cost)
    .fetch_one(pool)
    .await
    .expect("seed ingredient with cost")
}

/// Insert an allergen, return its id. Re-exports an inlined copy rather
/// than importing from `common::recipes` to keep each test file's
/// dependency graph flat — the body is three lines, duplication is
/// cheaper than cross-module coupling.
pub async fn seed_allergen(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>("INSERT INTO allergen (name) VALUES ($1) RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await
        .expect("seed allergen")
}

/// Attach an allergen to an ingredient via the `ingredient_allergen`
/// join table. Idempotent via ON CONFLICT DO NOTHING.
pub async fn attach_allergen(pool: &PgPool, ingredient_id: Uuid, allergen_id: Uuid) {
    sqlx::query(
        "INSERT INTO ingredient_allergen (ingredient_id, allergen_id) \
         VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(ingredient_id)
    .bind(allergen_id)
    .execute(pool)
    .await
    .expect("attach allergen");
}
