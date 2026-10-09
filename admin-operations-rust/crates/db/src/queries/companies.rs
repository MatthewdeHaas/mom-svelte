//! Company queries. TS counterpart is `src/lib/server/queries/companies.ts`
//! (192 lines, 8 exports). Companies are global master data — not scoped
//! within this module; the admin-vs-client scope lives at the HTTP layer.
//!
//! The two public items preserved from the pre-port stub, `CompanyRef`
//! and `get_by_name` (bool-only), are the Excel importer's dependency —
//! `crates/excel/src/import.rs` reads them. DO NOT change their shape
//! here; the full-shape helpers used by the admin API are new siblings
//! (`get_by_name_full` etc), not replacements.
//!
//! No `::float8` casts in this module — all columns are TEXT / BOOLEAN /
//! TIMESTAMPTZ / UUID, straightforward to decode.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use super::pagination::{build as build_paginated, offset, Paginated, PaginationParams};

// =============================================================
// Preserved for the Excel importer. DO NOT RENAME or change the
// signature — `crates/excel/src/import.rs` depends on this shape.
// =============================================================

/// Enough of a company to resolve an import row — the importer only
/// needs the id for the FK and `active` to reject orders against a
/// deactivated company.
#[derive(Debug, Clone)]
pub struct CompanyRef {
    pub id: Uuid,
    pub active: bool,
}

/// Case-insensitive exact-name lookup. Mirrors
/// `getCompanyByName(db, name)` on the TS side — `LOWER(name) = LOWER($1)`
/// so leading/trailing whitespace in the sheet is the caller's
/// problem, but case variance isn't.
pub async fn get_by_name(pool: &PgPool, name: &str) -> Result<Option<CompanyRef>, sqlx::Error> {
    let row: Option<(Uuid, bool)> =
        sqlx::query_as("SELECT id, active FROM company WHERE LOWER(name) = LOWER($1)")
            .bind(name)
            .fetch_optional(pool)
            .await?;

    Ok(row.map(|(id, active)| CompanyRef { id, active }))
}

/// Batched case-insensitive lookup for the importer. Returns a map
/// keyed by **lowercased** name so a `MapResolver` can match a
/// validator-produced name against the same key the importer uses on
/// the single-name path. Unknown names are simply absent from the map
/// — matching the `Option` semantics of `get_by_name`.
pub async fn get_many_by_name(
    pool: &PgPool,
    names: &[String],
) -> Result<std::collections::HashMap<String, CompanyRef>, sqlx::Error> {
    use std::collections::HashMap;

    if names.is_empty() {
        return Ok(HashMap::new());
    }

    // Lowercase keys both for the SQL filter and for the returned map
    // so lookup is uniformly case-insensitive. `name_lc` is what the
    // resolver will key against.
    let lc: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
    let rows: Vec<(String, Uuid, bool)> = sqlx::query_as(
        "SELECT LOWER(name) AS name_lc, id, active \
         FROM company \
         WHERE LOWER(name) = ANY($1::text[])",
    )
    .bind(&lc)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(name_lc, id, active)| (name_lc, CompanyRef { id, active }))
        .collect())
}

// =============================================================
// Public row types — mirror `src/lib/types.ts` `Company` and
// `UserWithCompany`.
// =============================================================

/// Full company row. Field order matches the TS `Company` interface so
/// JSON serialization is identical between the Rust and TS paths.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CompanyRow {
    pub id: Uuid,
    pub name: String,
    pub contact_email: Option<String>,
    pub phone: Option<String>,
    pub shipping_address: Option<String>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

/// Company embed on `UserWithCompanyRow`. Same columns as `CompanyRow` —
/// kept as a distinct type so a reader doesn't confuse the two call
/// sites. Serialised identically; a client decoding either sees the
/// same keys in the same order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyEmbed {
    pub id: Uuid,
    pub name: String,
    pub contact_email: Option<String>,
    pub phone: Option<String>,
    pub shipping_address: Option<String>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

/// User + embedded company. Mirrors TS `UserWithCompany`. `company_id`
/// is nullable on the users table but the JOIN forces NOT NULL for the
/// rows this function returns, so `company` is always `Some` here. The
/// nullable-outer-type (`Option<CompanyEmbed>`) matches the TS field
/// (`company: Company | null`) so decoders don't fork.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserWithCompanyRow {
    pub id: String,
    pub company_id: Option<Uuid>,
    pub email: String,
    /// Human-readable display name — Better Auth stores it on the
    /// `"user"` table as NOT NULL TEXT. The admin companies/[id]
    /// template surfaces it ("Full Name" column), so the pre-port
    /// `getUsersByCompany` SELECTed it; carry it through here too.
    pub name: String,
    /// `"user".role` is nullable TEXT (added by migration 003, no NOT
    /// NULL). TS surface types it as `UserRole` but doesn't enforce
    /// non-null at the pg layer — mirror that leniency here.
    pub role: Option<String>,
    pub created_at: DateTime<Utc>,
    pub company: Option<CompanyEmbed>,
}

// =============================================================
// Input types for mutating endpoints.
// =============================================================

/// Filters shared by every list endpoint. `search` only applies to
/// `list_paginated` — a defaulted `None` is a no-op for `list`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListOpts {
    #[serde(default)]
    pub include_inactive: bool,
    #[serde(default)]
    pub search: Option<String>,
}

/// Input to `create`. Matches TS `InsertCompany`: `name` required,
/// `active` defaults to true server-side, optional contact fields
/// default to NULL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertCompanyData {
    pub name: String,
    #[serde(default)]
    pub contact_email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub shipping_address: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
}

/// Input to `update`. All fields optional. `None` → leave unchanged
/// (COALESCE). The TS surface uses `?? null` + COALESCE semantics,
/// which collapses the "clear this field" case to "leave as-is" — we
/// mirror that exactly rather than introducing a three-state helper
/// here. If per-field clearing becomes a product requirement, port the
/// pattern from `recipes::update` (`deserialize_optional_opt`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateCompanyData {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub contact_email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub shipping_address: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
}

// =============================================================
// Reads
// =============================================================

/// `getCompanies` — ordered by name ASC. Two static branches so
/// `query_as!` can type-check both.
pub async fn list(pool: &PgPool, opts: &ListOpts) -> Result<Vec<CompanyRow>, sqlx::Error> {
    if opts.include_inactive {
        sqlx::query_as!(
            CompanyRow,
            r#"
            SELECT
                id               AS "id!: Uuid",
                name             AS "name!",
                contact_email,
                phone,
                shipping_address,
                active           AS "active!",
                created_at       AS "created_at!: DateTime<Utc>"
            FROM company
            ORDER BY name ASC
            "#,
        )
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as!(
            CompanyRow,
            r#"
            SELECT
                id               AS "id!: Uuid",
                name             AS "name!",
                contact_email,
                phone,
                shipping_address,
                active           AS "active!",
                created_at       AS "created_at!: DateTime<Utc>"
            FROM company
            WHERE active = TRUE
            ORDER BY name ASC
            "#,
        )
        .fetch_all(pool)
        .await
    }
}

/// `getCompanyById` — note NO active filter (returns inactive rows
/// too, matching TS semantics). The admin UI needs to show deactivated
/// companies on their detail page.
pub async fn get_by_id(pool: &PgPool, id: Uuid) -> Result<Option<CompanyRow>, sqlx::Error> {
    sqlx::query_as!(
        CompanyRow,
        r#"
        SELECT
            id               AS "id!: Uuid",
            name             AS "name!",
            contact_email,
            phone,
            shipping_address,
            active           AS "active!",
            created_at       AS "created_at!: DateTime<Utc>"
        FROM company
        WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(pool)
    .await
}

/// `getCompanyByName` — full-shape variant. Case-insensitive exact
/// match. The importer-only helper `get_by_name` above returns just
/// `(id, active)`; this one returns the full row for the admin API.
pub async fn get_by_name_full(
    pool: &PgPool,
    name: &str,
) -> Result<Option<CompanyRow>, sqlx::Error> {
    sqlx::query_as!(
        CompanyRow,
        r#"
        SELECT
            id               AS "id!: Uuid",
            name             AS "name!",
            contact_email,
            phone,
            shipping_address,
            active           AS "active!",
            created_at       AS "created_at!: DateTime<Utc>"
        FROM company
        WHERE LOWER(name) = LOWER($1)
        "#,
        name,
    )
    .fetch_optional(pool)
    .await
}

/// `getCompaniesPaginated` — runtime SQL because of the optional
/// `search` filter. Count + data queries share the same WHERE stack
/// built with `QueryBuilder`.
pub async fn list_paginated(
    pool: &PgPool,
    opts: &ListOpts,
    pagination: PaginationParams,
) -> Result<Paginated<CompanyRow>, sqlx::Error> {
    // ---- Count ----
    let mut count_qb: QueryBuilder<Postgres> =
        QueryBuilder::new("SELECT COUNT(*) AS count FROM company");
    let mut first = true;
    let mut push_cond = |qb: &mut QueryBuilder<'_, Postgres>, text: &str| {
        if first {
            qb.push(" WHERE ");
            first = false;
        } else {
            qb.push(" AND ");
        }
        qb.push(text);
    };
    if !opts.include_inactive {
        push_cond(&mut count_qb, "active = TRUE");
    }
    if let Some(ref s) = opts.search {
        push_cond(&mut count_qb, "LOWER(name) LIKE LOWER(");
        count_qb.push_bind(format!("%{}%", s));
        count_qb.push(")");
    }
    let total: i64 = count_qb.build_query_scalar::<i64>().fetch_one(pool).await?;

    // ---- Data ----
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT id, name, contact_email, phone, shipping_address, active, created_at \
         FROM company",
    );
    let mut first_d = true;
    let mut push_cond_d = |qb: &mut QueryBuilder<'_, Postgres>, text: &str| {
        if first_d {
            qb.push(" WHERE ");
            first_d = false;
        } else {
            qb.push(" AND ");
        }
        qb.push(text);
    };
    if !opts.include_inactive {
        push_cond_d(&mut qb, "active = TRUE");
    }
    if let Some(ref s) = opts.search {
        push_cond_d(&mut qb, "LOWER(name) LIKE LOWER(");
        qb.push_bind(format!("%{}%", s));
        qb.push(")");
    }
    qb.push(" ORDER BY name ASC LIMIT ");
    qb.push_bind(pagination.per_page);
    qb.push(" OFFSET ");
    qb.push_bind(offset(&pagination));

    let data: Vec<CompanyRow> = qb.build_query_as::<CompanyRow>().fetch_all(pool).await?;
    Ok(build_paginated(data, total, pagination))
}

/// `getUsersByCompany` — JOIN "user" + company, return rows with the
/// company embedded. Matches the TS mapping (`id`, `company_id`,
/// `email`, `role`, `created_at` from the user; nested `company`).
///
/// `query_as!` doesn't support `#[sqlx(flatten)]`-style nested decode,
/// so we use `query!` + manual row build — same pattern recipes uses
/// for its aggregate rows.
pub async fn get_users_by_company(
    pool: &PgPool,
    company_id: Uuid,
) -> Result<Vec<UserWithCompanyRow>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"
        SELECT
            u.id                 AS "id!",
            u.company_id         AS "company_id?: Uuid",
            u.email              AS "email!",
            u.name               AS "name!",
            u.role               AS "role?",
            u."createdAt"        AS "created_at!: DateTime<Utc>",
            c.id                 AS "company_pk!: Uuid",
            c.name               AS "company_name!",
            c.contact_email      AS "company_contact_email?",
            c.phone              AS "company_phone?",
            c.shipping_address   AS "company_shipping_address?",
            c.active             AS "company_active!",
            c.created_at         AS "company_created_at!: DateTime<Utc>"
        FROM "user" u
        JOIN company c ON c.id = u.company_id
        WHERE u.company_id = $1
        ORDER BY u.email ASC
        "#,
        company_id,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| UserWithCompanyRow {
            id: r.id,
            company_id: r.company_id,
            email: r.email,
            name: r.name,
            role: r.role,
            created_at: r.created_at,
            company: Some(CompanyEmbed {
                id: r.company_pk,
                name: r.company_name,
                contact_email: r.company_contact_email,
                phone: r.company_phone,
                shipping_address: r.company_shipping_address,
                active: r.company_active,
                created_at: r.company_created_at,
            }),
        })
        .collect())
}

// =============================================================
// Writes — UNIQUE-name violation (23505) is translated at the
// handler layer, not here. This module just returns `sqlx::Error`
// and the handler maps the code.
// =============================================================

/// `createCompany`. Trims name server-side (matches TS
/// `data.name.trim()`). Defaults: active = true.
pub async fn create(pool: &PgPool, data: InsertCompanyData) -> Result<CompanyRow, sqlx::Error> {
    let name = data.name.trim();
    let active = data.active.unwrap_or(true);
    sqlx::query_as!(
        CompanyRow,
        r#"
        INSERT INTO company (name, contact_email, phone, shipping_address, active)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING
            id               AS "id!: Uuid",
            name             AS "name!",
            contact_email,
            phone,
            shipping_address,
            active           AS "active!",
            created_at       AS "created_at!: DateTime<Utc>"
        "#,
        name,
        data.contact_email,
        data.phone,
        data.shipping_address,
        active,
    )
    .fetch_one(pool)
    .await
}

/// `updateCompany`. COALESCE on every field — `None` leaves the
/// current value in place. Trims `name` if present. Returns `None`
/// if no row matches `id`.
pub async fn update(
    pool: &PgPool,
    id: Uuid,
    data: UpdateCompanyData,
) -> Result<Option<CompanyRow>, sqlx::Error> {
    let name = data.name.as_deref().map(str::trim);
    sqlx::query_as!(
        CompanyRow,
        r#"
        UPDATE company
        SET
            name             = COALESCE($1, name),
            contact_email    = COALESCE($2, contact_email),
            phone            = COALESCE($3, phone),
            shipping_address = COALESCE($4, shipping_address),
            active           = COALESCE($5, active)
        WHERE id = $6
        RETURNING
            id               AS "id!: Uuid",
            name             AS "name!",
            contact_email,
            phone,
            shipping_address,
            active           AS "active!",
            created_at       AS "created_at!: DateTime<Utc>"
        "#,
        name,
        data.contact_email,
        data.phone,
        data.shipping_address,
        data.active,
        id,
    )
    .fetch_optional(pool)
    .await
}

/// `deactivateCompany`. Soft delete only — companies own persons and
/// orders, so hard delete is destructive. Mirrors the TS one-liner.
pub async fn deactivate(pool: &PgPool, id: Uuid) -> Result<Option<CompanyRow>, sqlx::Error> {
    update(
        pool,
        id,
        UpdateCompanyData {
            active: Some(false),
            ..Default::default()
        },
    )
    .await
}
