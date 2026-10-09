//! User queries. No TS counterpart module — the pre-port state had
//! inline SQL in a handful of SvelteKit route handlers
//! (`src/routes/admin/companies/[id]/+page.server.ts` and
//! `src/routes/invite/[token]/+page.server.ts`) rather than a
//! `src/lib/server/queries/users.ts`. Those routes are being cut
//! over directly to this surface, so the Rust module IS the module.
//!
//! Scope: data access only. Better Auth internals (session, account,
//! password hashing) stay in TS — the SvelteKit side calls
//! `auth.$context.internalAdapter.*` and `ctx.password.hash` for
//! those paths. This module only touches columns on the `"user"`
//! table itself.
//!
//! Schema (migrations 001 + 003 + 004):
//! ```sql
//! "user" (
//!   id              TEXT PK                 -- Better Auth string id
//!   name            TEXT NOT NULL
//!   email           TEXT NOT NULL UNIQUE
//!   "emailVerified" BOOLEAN NOT NULL
//!   image           TEXT
//!   "createdAt"     TIMESTAMPTZ NOT NULL DEFAULT now()
//!   "updatedAt"     TIMESTAMPTZ NOT NULL
//!   role            TEXT                    -- nullable; migration 003
//!   banned          BOOLEAN
//!   "banReason"     TEXT
//!   "banExpires"    TIMESTAMPTZ
//!   company_id      UUID REFERENCES company(id) ON DELETE SET NULL  -- migration 004
//! )
//! ```

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

// =============================================================
// Row types — kept minimal: each query returns only the columns
// its single caller needs. Mirrors what the pre-port inline SQL
// selected, so the route-layer behaviour is unchanged.
// =============================================================

/// Just `{id}`. Returned by `get_by_email` — the sole caller
/// (`createUserWithPassword`) uses this as a duplicate-email probe
/// and discards the row shape.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserIdRow {
    pub id: String,
}

/// `{id, email}` scoped to a given company. Returned by
/// `get_by_id_in_company` — the sole caller (`resetUserPassword`)
/// needs the id to feed Better Auth's `internalAdapter.*` calls
/// and the email to echo back in the action result.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserOwnedRow {
    pub id: String,
    pub email: String,
}

/// `{id, company_id}` after a successful `assign_company`. The id is
/// echoed for the caller's benefit (so a client-side invariant check
/// can match it against the input); `company_id` confirms the write
/// landed.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserCompanyRow {
    pub id: String,
    pub company_id: Option<Uuid>,
}

// =============================================================
// Reads
// =============================================================

/// Case-sensitive email lookup. Email is stored lowercased by Better
/// Auth and by the admin "create user" action (which calls
/// `email.toLowerCase().trim()` in its Zod schema), so a plain `=`
/// comparison is correct — no `LOWER(...)` wrapper needed.
///
/// Mirrors the inline SQL:
/// ```sql
/// SELECT id FROM "user" WHERE email = $1
/// ```
/// Used as a pre-insert duplicate probe in `createUserWithPassword`.
pub async fn get_by_email(pool: &PgPool, email: &str) -> Result<Option<UserIdRow>, sqlx::Error> {
    sqlx::query_as!(
        UserIdRow,
        r#"SELECT id AS "id!" FROM "user" WHERE email = $1"#,
        email,
    )
    .fetch_optional(pool)
    .await
}

/// Ownership-scoped lookup: returns the user ONLY if it belongs to
/// `company_id`. Mirrors the inline SQL:
/// ```sql
/// SELECT id, email FROM "user" WHERE id = $1 AND company_id = $2
/// ```
/// Used by `resetUserPassword` as the gate that stops an admin on
/// `/companies/X` from touching a user whose `company_id = Y`.
pub async fn get_by_id_in_company(
    pool: &PgPool,
    user_id: &str,
    company_id: Uuid,
) -> Result<Option<UserOwnedRow>, sqlx::Error> {
    sqlx::query_as!(
        UserOwnedRow,
        r#"SELECT id AS "id!", email AS "email!"
           FROM "user"
           WHERE id = $1 AND company_id = $2"#,
        user_id,
        company_id,
    )
    .fetch_optional(pool)
    .await
}

// =============================================================
// Writes
// =============================================================

/// Stamp `company_id` on a user row and return the resulting
/// `{id, company_id}`. Mirrors the inline SQL from two call sites:
///
/// 1. `createUserWithPassword` — after Better Auth's
///    `internalAdapter.createUser` creates the row, we tag it with
///    the admin's current company (which can't flow in through the
///    admin plugin because `company_id` is marked `input: false` in
///    the auth config).
/// 2. The invite-acceptance flow — after the token is verified, the
///    client user stamps their own `company_id` from the invite.
///
/// Returns `None` when `user_id` doesn't match any row.
pub async fn assign_company(
    pool: &PgPool,
    user_id: &str,
    company_id: Uuid,
) -> Result<Option<UserCompanyRow>, sqlx::Error> {
    sqlx::query_as!(
        UserCompanyRow,
        r#"UPDATE "user"
           SET company_id = $1
           WHERE id = $2
           RETURNING id AS "id!", company_id AS "company_id?: Uuid""#,
        company_id,
        user_id,
    )
    .fetch_optional(pool)
    .await
}
