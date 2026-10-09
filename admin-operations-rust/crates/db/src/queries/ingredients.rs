//! Ingredient queries. TS counterpart is `src/lib/server/queries/ingredients.ts`
//! (284 lines, 12 exports). Ingredients are global master data — not scoped
//! within this module; admin-only enforcement lives at the HTTP layer.
//!
//! **`cost_per_unit` is NUMERIC(10,4)** and is cast to `float8` on the way
//! out / in — same discipline recipes uses for `price_per_meal`. Avoids
//! pulling `rust_decimal`/`bigdecimal` and keeps parity with the TS side's
//! `parseFloat` semantics.
//!
//! Two queries use runtime SQL: `list_with_allergens_paginated` (optional
//! search filter) and `set_allergens` (multi-row VALUES). Everything else
//! is compile-time `query_as!`-checked.

use serde::{Deserialize, Deserializer, Serialize};
use sqlx::{FromRow, PgExecutor, PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use super::pagination::{build as build_paginated, offset, Paginated, PaginationParams};

// =============================================================
// Serde helper — distinguish "field absent" from "field present and null".
// =============================================================

/// `absent`  → `None`
/// `null`    → `Some(None)`   ("clear this column")
/// `value`   → `Some(Some(v))` ("set this column")
///
/// Used on `UpdateIngredientData.cost_per_unit` so the SQL CASE can
/// distinguish the three states — identical semantics to the TS
/// `'cost_per_unit' in data` sentinel at `ingredients.ts:213`.
fn deserialize_optional_opt<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

// =============================================================
// Public row types — mirror `src/lib/types.ts`.
// =============================================================

/// Base ingredient row. Decoded via `::float8` cast on `cost_per_unit`
/// so sqlx returns `f64` — matches `parseFloat` on the TS side.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct IngredientRow {
    pub id: Uuid,
    pub name: String,
    pub active: bool,
    pub cost_per_unit: Option<f64>,
}

/// Allergen embedded under each `IngredientWithAllergensRow`. Shape
/// matches `recipes::AllergenEmbed` and the TS `Allergen` interface —
/// kept as a separate local type per the companies/orders precedent
/// (avoid cross-module coupling even when shapes coincide).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllergenEmbed {
    pub id: Uuid,
    pub name: String,
}

/// Ingredient + nested allergen array. Matches TS `IngredientWithAllergens`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngredientWithAllergensRow {
    pub id: Uuid,
    pub name: String,
    pub active: bool,
    pub cost_per_unit: Option<f64>,
    pub allergen: Vec<AllergenEmbed>,
}

// =============================================================
// Input types for mutating endpoints.
// =============================================================

/// Filters shared by every list endpoint. `search` only applies to
/// `list_with_allergens_paginated` — a defaulted `None` is a no-op
/// for the others.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListOpts {
    #[serde(default)]
    pub include_inactive: bool,
    #[serde(default)]
    pub search: Option<String>,
}

/// Input to `create`. Matches TS `InsertIngredient`: name required,
/// `active` defaults to true server-side. `cost_per_unit` is NOT
/// settable on insert — the TS INSERT at `ingredients.ts:183-186`
/// doesn't carry it either, DB default is NULL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertIngredientData {
    pub name: String,
    #[serde(default)]
    pub active: Option<bool>,
}

/// Input to `update`. Matches TS `UpdateIngredient`:
/// - `name` / `active`: `None` = leave as-is (COALESCE).
/// - `cost_per_unit`: three-state via `Option<Option<f64>>`:
///   absent (don't touch), `null` (clear), value (set).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateIngredientData {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_optional_opt")]
    pub cost_per_unit: Option<Option<f64>>,
}

// =============================================================
// Reads — flat shape.
// =============================================================

/// `getIngredients` — ordered by name. Two static branches so
/// `query_as!` can type-check both.
pub async fn list(pool: &PgPool, opts: &ListOpts) -> Result<Vec<IngredientRow>, sqlx::Error> {
    if opts.include_inactive {
        sqlx::query_as!(
            IngredientRow,
            r#"
            SELECT
                id                       AS "id!: Uuid",
                name                     AS "name!",
                active                   AS "active!",
                cost_per_unit::float8    AS "cost_per_unit?: f64"
            FROM ingredient
            ORDER BY name ASC
            "#,
        )
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as!(
            IngredientRow,
            r#"
            SELECT
                id                       AS "id!: Uuid",
                name                     AS "name!",
                active                   AS "active!",
                cost_per_unit::float8    AS "cost_per_unit?: f64"
            FROM ingredient
            WHERE active = TRUE
            ORDER BY name ASC
            "#,
        )
        .fetch_all(pool)
        .await
    }
}

/// `getIngredientNames` — lightweight list used by the importer for
/// fuzzy matching. Returns names only, ordered.
pub async fn list_names(pool: &PgPool) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT name AS "name!" FROM ingredient ORDER BY name"#)
        .fetch_all(pool)
        .await
}

/// `getIngredientById` — returns `None` if the row doesn't exist OR is
/// inactive (unless `include_inactive`). Matches TS semantics.
pub async fn get_by_id(
    pool: &PgPool,
    id: Uuid,
    include_inactive: bool,
) -> Result<Option<IngredientRow>, sqlx::Error> {
    if include_inactive {
        sqlx::query_as!(
            IngredientRow,
            r#"
            SELECT
                id                       AS "id!: Uuid",
                name                     AS "name!",
                active                   AS "active!",
                cost_per_unit::float8    AS "cost_per_unit?: f64"
            FROM ingredient
            WHERE id = $1
            "#,
            id,
        )
        .fetch_optional(pool)
        .await
    } else {
        sqlx::query_as!(
            IngredientRow,
            r#"
            SELECT
                id                       AS "id!: Uuid",
                name                     AS "name!",
                active                   AS "active!",
                cost_per_unit::float8    AS "cost_per_unit?: f64"
            FROM ingredient
            WHERE id = $1 AND active = TRUE
            "#,
            id,
        )
        .fetch_optional(pool)
        .await
    }
}

// =============================================================
// Reads — nested shape (with allergens).
//
// Layered call for `get_with_allergens`: fetch the ingredient row,
// then its allergens via a static `query_as!`. Mirrors TS layering
// at `ingredients.ts:58-77`.
// =============================================================

/// `getIngredientWithAllergens` — single ingredient with allergens
/// ordered by name. Two queries, matches the TS layering.
pub async fn get_with_allergens(
    pool: &PgPool,
    id: Uuid,
    include_inactive: bool,
) -> Result<Option<IngredientWithAllergensRow>, sqlx::Error> {
    let Some(row) = get_by_id(pool, id, include_inactive).await? else {
        return Ok(None);
    };
    let allergen = sqlx::query_as!(
        AllergenEmbed,
        r#"
        SELECT a.id AS "id!: Uuid", a.name AS "name!"
        FROM allergen a
        JOIN ingredient_allergen ia ON ia.allergen_id = a.id
        WHERE ia.ingredient_id = $1
        ORDER BY a.name ASC
        "#,
        id,
    )
    .fetch_all(pool)
    .await?;

    Ok(Some(IngredientWithAllergensRow {
        id: row.id,
        name: row.name,
        active: row.active,
        cost_per_unit: row.cost_per_unit,
        allergen,
    }))
}

/// `getIngredientsWithAllergens` — flat list, each row with nested
/// allergen array. ARRAY_AGG + FILTER + manual zip, same pattern as
/// `recipes::get_ingredients`.
pub async fn list_with_allergens(
    pool: &PgPool,
    opts: &ListOpts,
) -> Result<Vec<IngredientWithAllergensRow>, sqlx::Error> {
    // Two branches so query! type-checks both. The ARRAY_AGG columns are
    // declared Nullable (FILTER + no GROUP matching rows → NULL) — we
    // `.unwrap_or_default()` on the Rust side to match TS's `?? []`.
    if opts.include_inactive {
        let rows = sqlx::query!(
            r#"
            SELECT
                i.id                   AS "id!: Uuid",
                i.name                 AS "name!",
                i.active               AS "active!",
                i.cost_per_unit::float8 AS "cost_per_unit?: f64",
                ARRAY_AGG(a.id   ORDER BY a.name) FILTER (WHERE a.id   IS NOT NULL) AS "allergen_ids: Vec<Uuid>",
                ARRAY_AGG(a.name ORDER BY a.name) FILTER (WHERE a.name IS NOT NULL) AS "allergen_names: Vec<String>"
            FROM ingredient i
            LEFT JOIN ingredient_allergen ia ON ia.ingredient_id = i.id
            LEFT JOIN allergen a ON a.id = ia.allergen_id
            GROUP BY i.id
            ORDER BY i.name ASC
            "#,
        )
        .fetch_all(pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let ids = r.allergen_ids.unwrap_or_default();
                let names = r.allergen_names.unwrap_or_default();
                let allergen = ids
                    .into_iter()
                    .zip(names)
                    .map(|(id, name)| AllergenEmbed { id, name })
                    .collect();
                IngredientWithAllergensRow {
                    id: r.id,
                    name: r.name,
                    active: r.active,
                    cost_per_unit: r.cost_per_unit,
                    allergen,
                }
            })
            .collect())
    } else {
        let rows = sqlx::query!(
            r#"
            SELECT
                i.id                   AS "id!: Uuid",
                i.name                 AS "name!",
                i.active               AS "active!",
                i.cost_per_unit::float8 AS "cost_per_unit?: f64",
                ARRAY_AGG(a.id   ORDER BY a.name) FILTER (WHERE a.id   IS NOT NULL) AS "allergen_ids: Vec<Uuid>",
                ARRAY_AGG(a.name ORDER BY a.name) FILTER (WHERE a.name IS NOT NULL) AS "allergen_names: Vec<String>"
            FROM ingredient i
            LEFT JOIN ingredient_allergen ia ON ia.ingredient_id = i.id
            LEFT JOIN allergen a ON a.id = ia.allergen_id
            WHERE i.active = TRUE
            GROUP BY i.id
            ORDER BY i.name ASC
            "#,
        )
        .fetch_all(pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let ids = r.allergen_ids.unwrap_or_default();
                let names = r.allergen_names.unwrap_or_default();
                let allergen = ids
                    .into_iter()
                    .zip(names)
                    .map(|(id, name)| AllergenEmbed { id, name })
                    .collect();
                IngredientWithAllergensRow {
                    id: r.id,
                    name: r.name,
                    active: r.active,
                    cost_per_unit: r.cost_per_unit,
                    allergen,
                }
            })
            .collect())
    }
}

// ---- Shared aggregate row for the paginated path. ----

/// Raw aggregate row for the paginated list. The nullable ARRAY_AGG
/// columns surface as `Option<Vec<...>>` which `.unwrap_or_default()`
/// collapses to `[]` on the Rust side (matches TS `?? []`).
#[derive(Debug, Clone, FromRow)]
struct WithAllergensRaw {
    id: Uuid,
    name: String,
    active: bool,
    cost_per_unit: Option<f64>,
    allergen_ids: Option<Vec<Uuid>>,
    allergen_names: Option<Vec<String>>,
}

/// Shared SELECT body for `list_with_allergens_paginated`. Column order
/// matches the TS counterpart so JSON diffs between TS and Rust paths
/// are empty.
const WITH_ALLERGENS_SELECT: &str = "\
    SELECT \
        i.id, \
        i.name, \
        i.active, \
        i.cost_per_unit::float8 AS cost_per_unit, \
        ARRAY_AGG(a.id   ORDER BY a.name) FILTER (WHERE a.id   IS NOT NULL) AS allergen_ids, \
        ARRAY_AGG(a.name ORDER BY a.name) FILTER (WHERE a.name IS NOT NULL) AS allergen_names \
    FROM ingredient i \
    LEFT JOIN ingredient_allergen ia ON ia.ingredient_id = i.id \
    LEFT JOIN allergen a ON a.id = ia.allergen_id";

fn fan_out(raw: WithAllergensRaw) -> IngredientWithAllergensRow {
    let ids = raw.allergen_ids.unwrap_or_default();
    let names = raw.allergen_names.unwrap_or_default();
    let allergen = ids
        .into_iter()
        .zip(names)
        .map(|(id, name)| AllergenEmbed { id, name })
        .collect();
    IngredientWithAllergensRow {
        id: raw.id,
        name: raw.name,
        active: raw.active,
        cost_per_unit: raw.cost_per_unit,
        allergen,
    }
}

/// `getIngredientsWithAllergensPaginated` — adds search filter +
/// LIMIT/OFFSET. Runtime SQL because of optional search.
pub async fn list_with_allergens_paginated(
    pool: &PgPool,
    opts: &ListOpts,
    pagination: PaginationParams,
) -> Result<Paginated<IngredientWithAllergensRow>, sqlx::Error> {
    // ---- Count: must apply the same WHERE as the data query. ----
    let mut count_qb: QueryBuilder<Postgres> =
        QueryBuilder::new("SELECT COUNT(*) AS count FROM ingredient i");
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
        push_cond(&mut count_qb, "i.active = TRUE");
    }
    if let Some(ref s) = opts.search {
        push_cond(&mut count_qb, "LOWER(i.name) LIKE LOWER(");
        count_qb.push_bind(format!("%{}%", s));
        count_qb.push(")");
    }
    let total: i64 = count_qb.build_query_scalar::<i64>().fetch_one(pool).await?;

    // ---- Data ----
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(WITH_ALLERGENS_SELECT);
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
        push_cond_d(&mut qb, "i.active = TRUE");
    }
    if let Some(ref s) = opts.search {
        push_cond_d(&mut qb, "LOWER(i.name) LIKE LOWER(");
        qb.push_bind(format!("%{}%", s));
        qb.push(")");
    }
    qb.push(" GROUP BY i.id ORDER BY i.name ASC LIMIT ");
    qb.push_bind(pagination.per_page);
    qb.push(" OFFSET ");
    qb.push_bind(offset(&pagination));

    let rows: Vec<WithAllergensRaw> = qb
        .build_query_as::<WithAllergensRaw>()
        .fetch_all(pool)
        .await?;
    let data: Vec<IngredientWithAllergensRow> = rows.into_iter().map(fan_out).collect();

    Ok(build_paginated(data, total, pagination))
}

// =============================================================
// Writes — UNIQUE-name violation (23505) is translated at the
// handler layer, not here. This module just returns `sqlx::Error`
// and the handler maps the code.
// =============================================================

/// `createIngredient`. Trims name server-side (matches TS
/// `data.name.trim()`). Defaults: active = true. `cost_per_unit`
/// is NOT accepted on insert — TS parity.
pub async fn create(
    pool: &PgPool,
    data: InsertIngredientData,
) -> Result<IngredientRow, sqlx::Error> {
    let name = data.name.trim();
    let active = data.active.unwrap_or(true);
    sqlx::query_as!(
        IngredientRow,
        r#"
        INSERT INTO ingredient (name, active)
        VALUES ($1, $2)
        RETURNING
            id                       AS "id!: Uuid",
            name                     AS "name!",
            active                   AS "active!",
            cost_per_unit::float8    AS "cost_per_unit?: f64"
        "#,
        name,
        active,
    )
    .fetch_one(pool)
    .await
}

/// `updateIngredient`. Three-state `cost_per_unit` via the CASE
/// expression — identical to the TS CASE at `ingredients.ts:206`.
///
/// `has_cost` sentinel distinguishes "field absent" from "field
/// present but null". Decided from `data.cost_per_unit.is_some()`
/// (post-serde).
pub async fn update(
    pool: &PgPool,
    id: Uuid,
    data: UpdateIngredientData,
) -> Result<Option<IngredientRow>, sqlx::Error> {
    let name = data.name.as_deref().map(str::trim);
    let active = data.active;
    let has_cost = data.cost_per_unit.is_some();
    // `Some(None)` → None (clear); `Some(Some(v))` → Some(v); `None` → None (unused when !has_cost).
    let cost: Option<f64> = data.cost_per_unit.flatten();

    sqlx::query_as!(
        IngredientRow,
        r#"
        UPDATE ingredient
        SET
            name          = COALESCE($1, name),
            active        = COALESCE($2, active),
            cost_per_unit = CASE WHEN $4 THEN $3::float8::numeric ELSE cost_per_unit END
        WHERE id = $5
        RETURNING
            id                       AS "id!: Uuid",
            name                     AS "name!",
            active                   AS "active!",
            cost_per_unit::float8    AS "cost_per_unit?: f64"
        "#,
        name,
        active,
        cost,
        has_cost,
        id,
    )
    .fetch_optional(pool)
    .await
}

/// `deactivateIngredient`. Soft delete — hard delete is blocked by
/// ON DELETE RESTRICT on recipe_ingredient. Mirrors the TS one-liner.
pub async fn deactivate(pool: &PgPool, id: Uuid) -> Result<Option<IngredientRow>, sqlx::Error> {
    update(
        pool,
        id,
        UpdateIngredientData {
            active: Some(false),
            ..Default::default()
        },
    )
    .await
}

// =============================================================
// Allergen associations. Generic over any `PgExecutor` so a caller
// could share a transaction — matches the TS `Pool | PoolClient`
// surface, even though no in-repo caller currently exercises it.
// =============================================================

/// `addIngredientAllergen` — idempotent insert into the join table.
pub async fn add_allergen<'c, E>(
    executor: E,
    ingredient_id: Uuid,
    allergen_id: Uuid,
) -> Result<(), sqlx::Error>
where
    E: PgExecutor<'c>,
{
    sqlx::query!(
        r#"
        INSERT INTO ingredient_allergen (ingredient_id, allergen_id)
        VALUES ($1, $2)
        ON CONFLICT DO NOTHING
        "#,
        ingredient_id,
        allergen_id,
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// `removeIngredientAllergen` — plain DELETE, no-op if the row isn't there.
pub async fn remove_allergen<'c, E>(
    executor: E,
    ingredient_id: Uuid,
    allergen_id: Uuid,
) -> Result<(), sqlx::Error>
where
    E: PgExecutor<'c>,
{
    sqlx::query!(
        r#"
        DELETE FROM ingredient_allergen
        WHERE ingredient_id = $1 AND allergen_id = $2
        "#,
        ingredient_id,
        allergen_id,
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// `setIngredientAllergens` — atomic full-replace. DELETE + bulk INSERT
/// in one transaction. The TS surface accepts `Pool | PoolClient`; here
/// we always own the transaction from a pool. No in-repo caller passes
/// a `PoolClient` (grep-verified), so this is sufficient.
pub async fn set_allergens(
    pool: &PgPool,
    ingredient_id: Uuid,
    allergen_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query!(
        r#"DELETE FROM ingredient_allergen WHERE ingredient_id = $1"#,
        ingredient_id,
    )
    .execute(&mut *tx)
    .await?;

    if !allergen_ids.is_empty() {
        let mut qb: QueryBuilder<Postgres> =
            QueryBuilder::new("INSERT INTO ingredient_allergen (ingredient_id, allergen_id) ");
        qb.push_values(allergen_ids.iter(), |mut b, aid| {
            b.push_bind(ingredient_id).push_bind(aid);
        });
        qb.build().execute(&mut *tx).await?;
    }

    tx.commit().await?;
    Ok(())
}
