//! Allergen queries. TS counterpart is `src/lib/server/queries/allergens.ts`
//! (88 lines, 7 exports). Allergens are a tiny reference table —
//! `(id uuid pk, name text UNIQUE NOT NULL)` — referenced by
//! `ingredient_allergen` and `person_allergen` join tables.
//!
//! No `::float8` casts, no three-state fields, no pagination. Every
//! query is compile-time `query_as!`-checked; no runtime SQL builder.
//! The one wrinkle is `get_ref_count`, which emits two subquery
//! `COUNT(*)`s to replicate the TS surface at `allergens.ts:67-79`.
//!
//! Delete is a HARD delete (unlike ingredients which soft-delete) —
//! the admin page pre-checks `get_ref_count` before calling delete,
//! so FK violations (23503) are only reachable on a race. We still
//! map them to 400 at the handler layer defensively.

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

// =============================================================
// Public row types — mirror `src/lib/types.ts` `Allergen`.
// =============================================================

/// Full allergen row. Shape matches the TS `Allergen` interface:
/// just `{ id, name }`.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AllergenRow {
    pub id: Uuid,
    pub name: String,
}

/// Reference count for a given allergen. Returned by `get_ref_count`
/// so the admin "delete" action can give a friendly message instead
/// of letting the DB raise a 23503.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefCount {
    pub ingredients: i64,
    pub persons: i64,
}

// =============================================================
// Input types for mutating endpoints.
// =============================================================

/// Input to `create` / `update`. Matches TS `InsertAllergen`: just
/// `{ name }`. Trimming happens server-side.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertAllergenData {
    pub name: String,
}

// =============================================================
// Reads
// =============================================================

/// `getAllergens` — ordered by name ascending.
pub async fn list(pool: &PgPool) -> Result<Vec<AllergenRow>, sqlx::Error> {
    sqlx::query_as!(
        AllergenRow,
        r#"
        SELECT
            id   AS "id!: Uuid",
            name AS "name!"
        FROM allergen
        ORDER BY name ASC
        "#,
    )
    .fetch_all(pool)
    .await
}

/// `getAllergenById` — returns `None` on miss.
pub async fn get_by_id(pool: &PgPool, id: Uuid) -> Result<Option<AllergenRow>, sqlx::Error> {
    sqlx::query_as!(
        AllergenRow,
        r#"
        SELECT
            id   AS "id!: Uuid",
            name AS "name!"
        FROM allergen
        WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(pool)
    .await
}

/// `getAllergenByName` — case-insensitive exact match. Mirrors
/// `LOWER(name) = LOWER($1)` on the TS side at `allergens.ts:28-34`.
pub async fn get_by_name(pool: &PgPool, name: &str) -> Result<Option<AllergenRow>, sqlx::Error> {
    sqlx::query_as!(
        AllergenRow,
        r#"
        SELECT
            id   AS "id!: Uuid",
            name AS "name!"
        FROM allergen
        WHERE LOWER(name) = LOWER($1)
        "#,
        name,
    )
    .fetch_optional(pool)
    .await
}

/// `getAllergenRefCount` — how many ingredients and persons reference
/// this allergen. Two subqueries in one round-trip; `COUNT(*)` returns
/// non-null `BIGINT` which sqlx surfaces as `Option<i64>` → we
/// `.unwrap_or(0)` for parity with TS `Number(row.ingredients)`.
pub async fn get_ref_count(pool: &PgPool, id: Uuid) -> Result<RefCount, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        SELECT
            (SELECT COUNT(*) FROM ingredient_allergen WHERE allergen_id = $1) AS "ingredients!: i64",
            (SELECT COUNT(*) FROM person_allergen     WHERE allergen_id = $1) AS "persons!: i64"
        "#,
        id,
    )
    .fetch_one(pool)
    .await?;

    Ok(RefCount {
        ingredients: row.ingredients,
        persons: row.persons,
    })
}

// =============================================================
// Writes — UNIQUE-name violation (23505) is translated at the
// handler layer, not here. This module just returns `sqlx::Error`
// and the handler maps the code.
// =============================================================

/// `createAllergen`. Trims name server-side (matches TS
/// `data.name.trim()` at `allergens.ts:46`).
pub async fn create(
    pool: &PgPool,
    data: InsertAllergenData,
) -> Result<AllergenRow, sqlx::Error> {
    let name = data.name.trim();
    sqlx::query_as!(
        AllergenRow,
        r#"
        INSERT INTO allergen (name)
        VALUES ($1)
        RETURNING
            id   AS "id!: Uuid",
            name AS "name!"
        "#,
        name,
    )
    .fetch_one(pool)
    .await
}

/// `updateAllergen`. Mirrors the TS UPDATE at `allergens.ts:57-62`:
/// bare `SET name = $1 WHERE id = $2`, no COALESCE needed since
/// `InsertAllergen` is total (name is required). Returns `None`
/// when no row matched.
pub async fn update(
    pool: &PgPool,
    id: Uuid,
    data: InsertAllergenData,
) -> Result<Option<AllergenRow>, sqlx::Error> {
    let name = data.name.trim();
    sqlx::query_as!(
        AllergenRow,
        r#"
        UPDATE allergen
        SET name = $1
        WHERE id = $2
        RETURNING
            id   AS "id!: Uuid",
            name AS "name!"
        "#,
        name,
        id,
    )
    .fetch_optional(pool)
    .await
}

/// `deleteAllergen` — HARD delete. Returns `true` iff a row was
/// removed; `false` if nothing matched. Mirrors `rowCount > 0` on
/// the TS side at `allergens.ts:86`.
///
/// The admin UI pre-checks `get_ref_count` to avoid raising 23503
/// (FK violation on `ingredient_allergen` / `person_allergen` which
/// both ON DELETE CASCADE — so actually the referencing rows cascade
/// away). The caller's pre-check is still the right UX move: deleting
/// an allergen that's in use would silently drop its join rows.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        r#"DELETE FROM allergen WHERE id = $1"#,
        id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}
