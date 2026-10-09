//! Recipe-domain seed helpers shared across `tests/recipes.rs`.
//! Mirror of `tests/common/orders.rs`. Each `#[sqlx::test]` test gets
//! a fresh DB, so unique-name collisions aren't a concern — the
//! helpers just panic on failure.

#![allow(dead_code)]

use sqlx::PgPool;
use uuid::Uuid;

/// Insert a recipe with the given name, return its id. Reuses the
/// existing seeder in `common::orders` would introduce an import cycle,
/// so we duplicate the three-line body here.
pub async fn seed_recipe(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO recipe (name, active) VALUES ($1, TRUE) RETURNING id",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("seed recipe")
}

/// Insert an inactive recipe. Useful for tests that want to assert
/// `include_inactive` behavior without first calling the deactivate
/// endpoint.
pub async fn seed_recipe_inactive(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO recipe (name, active) VALUES ($1, FALSE) RETURNING id",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("seed inactive recipe")
}

/// Insert an ingredient, return its id.
pub async fn seed_ingredient(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>("INSERT INTO ingredient (name) VALUES ($1) RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await
        .expect("seed ingredient")
}

/// Insert an allergen, return its id.
pub async fn seed_allergen(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>("INSERT INTO allergen (name) VALUES ($1) RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await
        .expect("seed allergen")
}

/// Attach an allergen to an ingredient via the `ingredient_allergen`
/// join table. Idempotent via ON CONFLICT DO NOTHING.
pub async fn attach_allergen_to_ingredient(pool: &PgPool, ingredient_id: Uuid, allergen_id: Uuid) {
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

/// Attach an ingredient to a recipe via `recipe_ingredient`. Mirrors
/// the TS `addRecipeIngredient` so tests can seed without going through
/// the HTTP surface they're trying to exercise.
pub async fn attach_ingredient_to_recipe(
    pool: &PgPool,
    recipe_id: Uuid,
    ingredient_id: Uuid,
    amount: f64,
    unit: &str,
) {
    sqlx::query(
        "INSERT INTO recipe_ingredient (recipe_id, ingredient_id, amount, unit) \
         VALUES ($1, $2, $3::float8::numeric, $4) \
         ON CONFLICT (recipe_id, ingredient_id) DO UPDATE \
             SET amount = EXCLUDED.amount, unit = EXCLUDED.unit",
    )
    .bind(recipe_id)
    .bind(ingredient_id)
    .bind(amount)
    .bind(unit)
    .execute(pool)
    .await
    .expect("attach ingredient");
}
