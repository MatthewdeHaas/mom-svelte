//! Person-domain seed helpers shared across `tests/persons.rs`.
//! Mirror of `tests/common/allergens.rs` on the person side.
//! Each `#[sqlx::test]` test gets a fresh DB, so unique-name
//! collisions aren't a concern — the helpers just panic on failure.
//!
//! `seed_company` + `seed_allergen` exist in sibling modules already;
//! re-defined here (three lines each) rather than `pub use`'d to keep
//! each test file's dependency graph flat.

#![allow(dead_code)]

use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

/// Insert a company, return its id. Needed to parent persons.
pub async fn seed_company(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO company (name, active) VALUES ($1, TRUE) RETURNING id",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("seed company")
}

/// Insert an allergen, return its id.
pub async fn seed_allergen(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>("INSERT INTO allergen (name) VALUES ($1) RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await
        .expect("seed allergen")
}

/// Insert an active person tied to a company. Returns the id.
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

/// Insert an inactive person tied to a company. Returns the id.
pub async fn seed_person_inactive(pool: &PgPool, company_id: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO person (company_id, name, active) VALUES ($1, $2, FALSE) RETURNING id",
    )
    .bind(company_id)
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("seed inactive person")
}

/// Attach an allergen to a person via `person_allergen`. Idempotent.
pub async fn attach_allergen(pool: &PgPool, person_id: Uuid, allergen_id: Uuid) {
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

/// Insert an attendance row directly. Returns its id.
pub async fn seed_attendance(
    pool: &PgPool,
    person_id: Uuid,
    serving_date: NaiveDate,
    attending: bool,
) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO attendance (person_id, serving_date, attending) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(person_id)
    .bind(serving_date)
    .bind(attending)
    .fetch_one(pool)
    .await
    .expect("seed attendance")
}
