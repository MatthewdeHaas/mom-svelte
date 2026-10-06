//! Invite queries. TS counterpart is `src/lib/server/queries/invites.ts`
//! (49 lines, 5 exports). Invites are one-time tokens admins mint to
//! onboard client users — see migration 005.
//!
//! Schema (migration 005):
//! ```sql
//! invite (
//!   id          UUID PK DEFAULT gen_random_uuid(),
//!   company_id  UUID NOT NULL REFERENCES company(id) ON DELETE CASCADE,
//!   email       TEXT NOT NULL,
//!   token       TEXT NOT NULL UNIQUE,
//!   expires_at  TIMESTAMPTZ NOT NULL,
//!   used_at     TIMESTAMPTZ,
//!   created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
//! )
//! ```
//!
//! `expires_at` is `TIMESTAMPTZ` with hour-precision semantics —
//! `src/routes/invite/[token]/+page.server.ts:16` compares against
//! `new Date()`. The wire shape is a full RFC3339 `DateTime<Utc>` so
//! the dispatcher can't truncate it to `YYYY-MM-DD` (which would make
//! "48h expiry" effectively "48h rounded to end-of-day" and drift the
//! acceptance check by up to 24h).
//!
//! The TS `markInviteUsed` and `revokeInvite` emit identical SQL
//! (`UPDATE invite SET used_at = now() WHERE id = $1`). We keep ONE
//! db fn (`mark_used`) and let the handler layer expose two routes
//! for API parity with the TS surface.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

// =============================================================
// Row + input types — mirror `src/lib/types.ts` `Invite` / `InsertInvite`.
// =============================================================

/// Full invite row. Shape matches the TS `Invite` interface.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct InviteRow {
    pub id: Uuid,
    pub company_id: Uuid,
    pub email: String,
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Input to `create`. Mirrors TS `InsertInvite`. Email is trimmed and
/// lowercased inside `create` — matches `invites.ts:36`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertInviteData {
    pub company_id: Uuid,
    pub email: String,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

// =============================================================
// Reads
// =============================================================

/// `getInviteByToken` — exact-match lookup. Returns `None` on miss.
pub async fn get_by_token(pool: &PgPool, token: &str) -> Result<Option<InviteRow>, sqlx::Error> {
    sqlx::query_as!(
        InviteRow,
        r#"
        SELECT
            id         AS "id!: Uuid",
            company_id AS "company_id!: Uuid",
            email      AS "email!",
            token      AS "token!",
            expires_at AS "expires_at!: DateTime<Utc>",
            used_at    AS "used_at?: DateTime<Utc>",
            created_at AS "created_at!: DateTime<Utc>"
        FROM invite
        WHERE token = $1
        "#,
        token,
    )
    .fetch_optional(pool)
    .await
}

/// `getInvitesByCompany` — ordered `created_at DESC`.
pub async fn list_by_company(
    pool: &PgPool,
    company_id: Uuid,
) -> Result<Vec<InviteRow>, sqlx::Error> {
    sqlx::query_as!(
        InviteRow,
        r#"
        SELECT
            id         AS "id!: Uuid",
            company_id AS "company_id!: Uuid",
            email      AS "email!",
            token      AS "token!",
            expires_at AS "expires_at!: DateTime<Utc>",
            used_at    AS "used_at?: DateTime<Utc>",
            created_at AS "created_at!: DateTime<Utc>"
        FROM invite
        WHERE company_id = $1
        ORDER BY created_at DESC
        "#,
        company_id,
    )
    .fetch_all(pool)
    .await
}

// =============================================================
// Writes — UNIQUE-token violation (23505) is translated at the
// handler layer, not here.
// =============================================================

/// `createInvite`. Trims + lowercases email server-side to mirror
/// `invites.ts:36` (`data.email.toLowerCase().trim()`).
pub async fn create(
    pool: &PgPool,
    data: InsertInviteData,
) -> Result<InviteRow, sqlx::Error> {
    let email = data.email.trim().to_lowercase();
    sqlx::query_as!(
        InviteRow,
        r#"
        INSERT INTO invite (company_id, email, token, expires_at)
        VALUES ($1, $2, $3, $4)
        RETURNING
            id         AS "id!: Uuid",
            company_id AS "company_id!: Uuid",
            email      AS "email!",
            token      AS "token!",
            expires_at AS "expires_at!: DateTime<Utc>",
            used_at    AS "used_at?: DateTime<Utc>",
            created_at AS "created_at!: DateTime<Utc>"
        "#,
        data.company_id,
        email,
        data.token,
        data.expires_at,
    )
    .fetch_one(pool)
    .await
}

/// `markInviteUsed` AND `revokeInvite` — both emit the same SQL on the
/// TS side (`invites.ts:40-48`), so we expose one db function and let
/// the handler layer surface two routes.
///
/// Returns `true` iff a row was touched; `false` if the id didn't
/// match. The TS surface returns `void`, but a boolean is strictly
/// more informative and the handler can translate it to 204/404.
pub async fn mark_used(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        r#"UPDATE invite SET used_at = now() WHERE id = $1"#,
        id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}
