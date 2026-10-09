//! Allergen-domain seed helpers shared across `tests/allergens.rs`.
//! Mirror of `tests/common/ingredients.rs`. Each `#[sqlx::test]` test
//! gets a fresh DB, so unique-name collisions aren't a concern — the
//! helpers just panic on failure.
//!
//! `seed_allergen` already exists in `common::recipes` and
//! `common::ingredients`; re-defined here (three lines) rather than
//! `pub use`'d to keep each test file's dependency graph flat.

#![allow(dead_code)]

use sqlx::PgPool;
use uuid::Uuid;

/// Insert an allergen, return its id.
pub async fn seed_allergen(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>("INSERT INTO allergen (name) VALUES ($1) RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await
        .expect("seed allergen")
}

/// Insert an ingredient, return its id. Needed to seed
/// `ingredient_allergen` rows that `get_ref_count` reads.
pub async fn seed_ingredient(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO ingredient (name, active) VALUES ($1, TRUE) RETURNING id",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("seed ingredient")
}

/// Insert a company, return its id. Needed to parent persons for the
/// `get_ref_count` person-side.
pub async fn seed_company(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>("INSERT INTO company (name) VALUES ($1) RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await
        .expect("seed company")
}

/// Insert a person tied to a company. Returns the person id.
pub async fn seed_person(pool: &PgPool, company_id: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO person (company_id, name, active) VALUES ($1, $2, TRUE) RETURNING id",
    )
    .bind(company_id)
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("seed person")
}

/// Attach an allergen to an ingredient. Idempotent via ON CONFLICT.
pub async fn attach_allergen_to_ingredient(pool: &PgPool, ingredient_id: Uuid, allergen_id: Uuid) {
    sqlx::query(
        "INSERT INTO ingredient_allergen (ingredient_id, allergen_id) \
         VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(ingredient_id)
    .bind(allergen_id)
    .execute(pool)
    .await
    .expect("attach ingredient_allergen");
}

/// Attach an allergen to a person. Idempotent via ON CONFLICT.
pub async fn attach_allergen_to_person(pool: &PgPool, person_id: Uuid, allergen_id: Uuid) {
    sqlx::query(
        "INSERT INTO person_allergen (person_id, allergen_id) \
         VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(person_id)
    .bind(allergen_id)
    .execute(pool)
    .await
    .expect("attach person_allergen");
}
