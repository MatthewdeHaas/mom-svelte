//! Company-domain seed helpers shared across `tests/companies.rs`.
//! Mirror of `tests/common/recipes.rs`. Each `#[sqlx::test]` test
//! gets a fresh DB, so unique-name collisions aren't a concern.

#![allow(dead_code)]

use sqlx::PgPool;
use uuid::Uuid;

/// Insert a company with the given name, return its id. The TS test
/// seeds via `createCompany(ctx.client, {...})`; we skip that path on
/// the Rust side so we can seed directly without first going through
/// the HTTP surface the tests exercise.
pub async fn seed_company(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO company (name, active) VALUES ($1, TRUE) RETURNING id",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("seed company")
}

/// Insert a company with full optional fields set. Mirrors the "full
/// shape" fixture rows from the TS test.
pub async fn seed_company_full(
    pool: &PgPool,
    name: &str,
    contact_email: Option<&str>,
    phone: Option<&str>,
    shipping_address: Option<&str>,
) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO company (name, contact_email, phone, shipping_address, active) \
         VALUES ($1, $2, $3, $4, TRUE) RETURNING id",
    )
    .bind(name)
    .bind(contact_email)
    .bind(phone)
    .bind(shipping_address)
    .fetch_one(pool)
    .await
    .expect("seed company full")
}

/// Insert an inactive company directly. Useful for tests that assert
/// the `include_inactive` flag's behavior without first calling the
/// deactivate endpoint.
pub async fn seed_company_inactive(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO company (name, active) VALUES ($1, FALSE) RETURNING id",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("seed inactive company")
}

/// Insert a `"user"` row attached to the given company. The role is
/// `'user'` to match the TS test fixture (`getUsersByCompany` doesn't
/// filter on role, so any value works — we stay literal).
///
/// Returns the inserted user id so a caller can assert on it.
pub async fn seed_user_in_company(
    pool: &PgPool,
    user_id: &str,
    email: &str,
    company_id: Uuid,
) -> String {
    sqlx::query(
        r#"INSERT INTO "user" (id, name, email, "emailVerified", role, company_id, "createdAt", "updatedAt")
           VALUES ($1, 'Test User', $2, true, 'user', $3, now(), now())"#,
    )
    .bind(user_id)
    .bind(email)
    .bind(company_id)
    .execute(pool)
    .await
    .expect("seed user in company");
    user_id.to_string()
}
