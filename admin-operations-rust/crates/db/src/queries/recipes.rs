//! Recipe queries. TS counterpart is `src/lib/server/queries/recipes.ts`
//! (515 lines, 13 exports). Unlike orders, recipes are global master
//! data — not company-scoped. Admin can write; clients get 403 on
//! mutations but can read freely. Row-level filtering lives nowhere in
//! this module; it's enforced in the API handlers.
//!
//! **Numeric columns are cast to `float8`** in SQL for the same reason
//! orders does it: `NUMERIC` has no safe f64 decode in sqlx without
//! pulling `rust_decimal`/`bigdecimal`, and the TS side already does
//! `parseFloat` everywhere. Keep casts in sync or parity drifts.
//!
//! Two queries use runtime SQL (`list_with_ingredients_paginated` —
//! optional search filter; `set_ingredients` — multi-row VALUES).
//! Everything else is compile-time `query_as!`-checked.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::{FromRow, PgExecutor, PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use super::pagination::{build as build_paginated, offset, Paginated, PaginationParams};

// =============================================================
// Preserved for the Excel importer. DO NOT RENAME or change the
// signature — `crates/excel/src/import.rs` depends on this shape.
// =============================================================

/// Enough of a recipe to resolve an import row. The importer always
/// filters inactive recipes out, so there's no `active` field here —
/// a `None` return means "not found OR inactive", which is exactly
/// what the TS side surfaces as a single error message.
#[derive(Debug, Clone)]
pub struct RecipeRef {
    pub id: Uuid,
}

/// Case-insensitive exact-name lookup, inactive recipes excluded.
/// Mirrors `getRecipeByName(db, name, { includeInactive: false })`.
pub async fn get_by_name_active(
    pool: &PgPool,
    name: &str,
) -> Result<Option<RecipeRef>, sqlx::Error> {
    let row: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM recipe WHERE LOWER(name) = LOWER($1) AND active = TRUE")
            .bind(name)
            .fetch_optional(pool)
            .await?;

    Ok(row.map(|(id,)| RecipeRef { id }))
}

/// Batched case-insensitive active-only lookup for the importer.
/// Returns a map keyed by **lowercased** name so a resolver can match
/// a validator-produced name case-insensitively. Unknown or inactive
/// names are absent from the map — same semantics as the `Option`
/// return of `get_by_name_active`.
pub async fn get_many_by_name_active(
    pool: &PgPool,
    names: &[String],
) -> Result<std::collections::HashMap<String, RecipeRef>, sqlx::Error> {
    use std::collections::HashMap;

    if names.is_empty() {
        return Ok(HashMap::new());
    }

    let lc: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
    let rows: Vec<(String, Uuid)> = sqlx::query_as(
        "SELECT LOWER(name) AS name_lc, id \
         FROM recipe \
         WHERE LOWER(name) = ANY($1::text[]) AND active = TRUE",
    )
    .bind(&lc)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(name_lc, id)| (name_lc, RecipeRef { id }))
        .collect())
}

// =============================================================
// Serde helper — distinguish "field absent" from "field present and null".
// =============================================================

/// `absent`  → `None`
/// `null`    → `Some(None)`   ("clear this column")
/// `value`   → `Some(Some(v))` ("set this column")
///
/// Used on `UpdateRecipeData.price_per_meal` so the SQL CASE can
/// distinguish the three states — identical semantics to the TS
/// `'price_per_meal' in data` sentinel at `recipes.ts:411`.
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

/// Base recipe row. Decoded via `::float8` cast on `price_per_meal`
/// so sqlx returns `f64` — matches `parseFloat` on the TS side.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RecipeRow {
    pub id: Uuid,
    pub name: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub price_per_meal: Option<f64>,
}

/// Allergen embedded under each `RecipeIngredientRow`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllergenEmbed {
    pub id: Uuid,
    pub name: String,
}

/// Ingredient row embedded on `RecipeWithIngredientsRow` and returned
/// by `get_ingredients` directly. Matches TS `RecipeIngredient`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeIngredientRow {
    pub ingredient_id: Uuid,
    pub name: String,
    pub amount: f64,
    pub unit: String,
    pub allergen: Vec<AllergenEmbed>,
}

/// Recipe + nested ingredient array. Matches TS `RecipeWithIngredients`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeWithIngredientsRow {
    pub id: Uuid,
    pub name: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub price_per_meal: Option<f64>,
    pub ingredients: Vec<RecipeIngredientRow>,
}

// =============================================================
// Input types for mutating endpoints.
// =============================================================

/// Filters shared by every list endpoint. `search` only applies to
/// `list_with_ingredients_paginated` — a defaulted `None` is a no-op
/// for the others.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListOpts {
    #[serde(default)]
    pub include_inactive: bool,
    #[serde(default)]
    pub search: Option<String>,
}

/// Input to `create`. Matches TS `InsertRecipe`: name is required,
/// `active` defaults to true server-side.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertRecipeData {
    pub name: String,
    #[serde(default)]
    pub active: Option<bool>,
}

/// Input to `update`. Matches TS `UpdateRecipe`:
/// - `name` / `active`: `None` = leave as-is (COALESCE).
/// - `price_per_meal`: three-state via `Option<Option<f64>>`:
///   absent (don't touch), `null` (clear), value (set).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateRecipeData {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_optional_opt")]
    pub price_per_meal: Option<Option<f64>>,
}

/// One line of ingredient attached to a recipe. Used by
/// `set_ingredients` (full-replace) and `add_ingredient` (one-off upsert).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngredientLine {
    pub ingredient_id: Uuid,
    pub amount: f64,
    pub unit: String,
}

// =============================================================
// Reads — simple shape.
// =============================================================

/// `getRecipes` — ordered by name. Static SQL via two branches so
/// query_as! can type-check both.
pub async fn list(pool: &PgPool, opts: &ListOpts) -> Result<Vec<RecipeRow>, sqlx::Error> {
    if opts.include_inactive {
        sqlx::query_as!(
            RecipeRow,
            r#"
            SELECT
                id                       AS "id!: Uuid",
                name                     AS "name!",
                active                   AS "active!",
                created_at               AS "created_at!: DateTime<Utc>",
                price_per_meal::float8   AS "price_per_meal?: f64"
            FROM recipe
            ORDER BY name ASC
            "#,
        )
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as!(
            RecipeRow,
            r#"
            SELECT
                id                       AS "id!: Uuid",
                name                     AS "name!",
                active                   AS "active!",
                created_at               AS "created_at!: DateTime<Utc>",
                price_per_meal::float8   AS "price_per_meal?: f64"
            FROM recipe
            WHERE active = TRUE
            ORDER BY name ASC
            "#,
        )
        .fetch_all(pool)
        .await
    }
}

/// `getRecipeById` — returns `None` if the row doesn't exist OR is
/// inactive (unless `include_inactive`). Matches TS semantics.
pub async fn get_by_id(
    pool: &PgPool,
    id: Uuid,
    include_inactive: bool,
) -> Result<Option<RecipeRow>, sqlx::Error> {
    if include_inactive {
        sqlx::query_as!(
            RecipeRow,
            r#"
            SELECT
                id                       AS "id!: Uuid",
                name                     AS "name!",
                active                   AS "active!",
                created_at               AS "created_at!: DateTime<Utc>",
                price_per_meal::float8   AS "price_per_meal?: f64"
            FROM recipe
            WHERE id = $1
            "#,
            id,
        )
        .fetch_optional(pool)
        .await
    } else {
        sqlx::query_as!(
            RecipeRow,
            r#"
            SELECT
                id                       AS "id!: Uuid",
                name                     AS "name!",
                active                   AS "active!",
                created_at               AS "created_at!: DateTime<Utc>",
                price_per_meal::float8   AS "price_per_meal?: f64"
            FROM recipe
            WHERE id = $1 AND active = TRUE
            "#,
            id,
        )
        .fetch_optional(pool)
        .await
    }
}

/// `getRecipeByName` — case-insensitive. The importer-only helper
/// `get_by_name_active` above is the hot path; this is the full-shape
/// variant used by the admin API.
pub async fn get_by_name(
    pool: &PgPool,
    name: &str,
    include_inactive: bool,
) -> Result<Option<RecipeRow>, sqlx::Error> {
    if include_inactive {
        sqlx::query_as!(
            RecipeRow,
            r#"
            SELECT
                id                       AS "id!: Uuid",
                name                     AS "name!",
                active                   AS "active!",
                created_at               AS "created_at!: DateTime<Utc>",
                price_per_meal::float8   AS "price_per_meal?: f64"
            FROM recipe
            WHERE LOWER(name) = LOWER($1)
            "#,
            name,
        )
        .fetch_optional(pool)
        .await
    } else {
        sqlx::query_as!(
            RecipeRow,
            r#"
            SELECT
                id                       AS "id!: Uuid",
                name                     AS "name!",
                active                   AS "active!",
                created_at               AS "created_at!: DateTime<Utc>",
                price_per_meal::float8   AS "price_per_meal?: f64"
            FROM recipe
            WHERE LOWER(name) = LOWER($1) AND active = TRUE
            "#,
            name,
        )
        .fetch_optional(pool)
        .await
    }
}

/// `isRecipeActive` — returns `false` when the row doesn't exist.
/// Matches TS semantics (`result.rows[0]?.active ?? false`).
pub async fn is_active(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    // Explicit annotation pins the macro's return type so rustc 1.99.0's
    // never-type fallback (issue #156047) doesn't collapse the inferred
    // `Option<bool>` to `!` when the surrounding expression is terse.
    let row: Option<bool> = sqlx::query_scalar!(
        r#"SELECT active AS "active!" FROM recipe WHERE id = $1"#,
        id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.unwrap_or(false))
}

// =============================================================
// Reads — nested shape (ingredients + allergens).
//
// The TS SQL packs allergen per ingredient into comma-separated strings
// via a correlated subquery, then zips in-memory. We port that verbatim
// to keep the JSON byte-identical. `query_as!` can't type-check the
// aggregate columns here (nullable ARRAY_AGG with FILTER), so we use
// `query!` + manual row build — same pattern `orders::get_by_id` uses
// for the detail join.
// =============================================================

/// `getRecipeIngredients` — ARRAY_AGG allergens per ingredient, then
/// zip in Rust.
pub async fn get_ingredients(
    pool: &PgPool,
    recipe_id: Uuid,
) -> Result<Vec<RecipeIngredientRow>, sqlx::Error> {
    // The query! macro produces an anonymous row type. rustc 1.99.0's
    // never-type fallback (issue #156047) collapses that anonymous type
    // to `!` here because no explicit annotation on `rows` or `mapped`
    // can name it. Workaround: use `query_as!` with a named row struct
    // so the row type is `IngredientAggRow`, not anonymous.
    #[derive(FromRow)]
    struct IngredientAggRow {
        ingredient_id: Uuid,
        name: String,
        amount: f64,
        unit: String,
        allergen_ids: Option<Vec<Uuid>>,
        allergen_names: Option<Vec<String>>,
    }
    let rows: Vec<IngredientAggRow> = sqlx::query_as!(
        IngredientAggRow,
        r#"
        SELECT
            i.id             AS "ingredient_id!: Uuid",
            i.name           AS "name!",
            ri.amount::float8 AS "amount!: f64",
            ri.unit          AS "unit!",
            ARRAY_AGG(a.id   ORDER BY a.name) FILTER (WHERE a.id IS NOT NULL)   AS "allergen_ids: Vec<Uuid>",
            ARRAY_AGG(a.name ORDER BY a.name) FILTER (WHERE a.name IS NOT NULL) AS "allergen_names: Vec<String>"
        FROM recipe_ingredient ri
        JOIN ingredient i ON i.id = ri.ingredient_id
        LEFT JOIN ingredient_allergen ia ON ia.ingredient_id = i.id
        LEFT JOIN allergen a ON a.id = ia.allergen_id
        WHERE ri.recipe_id = $1
        GROUP BY i.id, i.name, ri.amount, ri.unit
        ORDER BY i.name ASC
        "#,
        recipe_id,
    )
    .fetch_all(pool)
    .await?;

    let mut mapped: Vec<RecipeIngredientRow> = Vec::new();
    for r in rows {
        let ids = r.allergen_ids.unwrap_or_default();
        let names = r.allergen_names.unwrap_or_default();
        let allergen: Vec<AllergenEmbed> = ids
            .into_iter()
            .zip(names)
            .map(|(id, name)| AllergenEmbed { id, name })
            .collect();
        mapped.push(RecipeIngredientRow {
            ingredient_id: r.ingredient_id,
            name: r.name,
            amount: r.amount,
            unit: r.unit,
            allergen,
        });
    }
    Ok(mapped)
}

/// `getRecipeWithIngredients` — recipe row + nested ingredient. Two
/// queries, matches the TS layering (`getRecipeById` → `getRecipeIngredients`).
pub async fn get_with_ingredients(
    pool: &PgPool,
    id: Uuid,
    include_inactive: bool,
) -> Result<Option<RecipeWithIngredientsRow>, sqlx::Error> {
    let Some(recipe) = get_by_id(pool, id, include_inactive).await? else {
        return Ok(None);
    };
    let ingredients = get_ingredients(pool, id).await?;
    Ok(Some(RecipeWithIngredientsRow {
        id: recipe.id,
        name: recipe.name,
        active: recipe.active,
        created_at: recipe.created_at,
        price_per_meal: recipe.price_per_meal,
        ingredients,
    }))
}

// ---- Shared SELECT for the aggregated listing endpoints. ----
//
// The three list_with_ingredients* paths share the same SELECT shape;
// the differences are WHERE and LIMIT/OFFSET. Keeping the body in one
// const + `query_as!`-decoded Raw struct keeps the three callers short
// and ensures byte-identical column order (JSON parity).

/// Raw aggregate row. Each `*_sets` column holds comma-separated allergen
/// ids/names, one entry per ingredient (zip with ingredient_ids).
#[derive(Debug, Clone, FromRow)]
struct WithIngredientsRaw {
    id: Uuid,
    name: String,
    active: bool,
    created_at: DateTime<Utc>,
    price_per_meal: Option<f64>,
    ingredient_ids: Option<Vec<Uuid>>,
    ingredient_names: Option<Vec<String>>,
    ingredient_amounts: Option<Vec<f64>>,
    ingredient_units: Option<Vec<String>>,
    allergen_id_sets: Option<Vec<Option<String>>>,
    allergen_name_sets: Option<Vec<Option<String>>>,
}

/// Shared SELECT body. Column order and ORDER BY identical to the TS
/// counterpart so JSON diffs between TS and Rust paths are empty.
///
/// `::float8[]` casts the aggregated NUMERIC array in one shot — avoids
/// per-element decode cost and matches the `parseFloat` discipline.
const WITH_INGREDIENTS_SELECT: &str = "\
    SELECT \
        r.id, \
        r.name, \
        r.active, \
        r.created_at, \
        r.price_per_meal::float8 AS price_per_meal, \
        ARRAY_AGG(i.id      ORDER BY i.name) FILTER (WHERE i.id IS NOT NULL)                  AS ingredient_ids, \
        ARRAY_AGG(i.name    ORDER BY i.name) FILTER (WHERE i.name IS NOT NULL)                AS ingredient_names, \
        ARRAY_AGG(ri.amount::float8 ORDER BY i.name) FILTER (WHERE i.id IS NOT NULL)          AS ingredient_amounts, \
        ARRAY_AGG(ri.unit   ORDER BY i.name) FILTER (WHERE i.id IS NOT NULL)                  AS ingredient_units, \
        ARRAY_AGG( \
            (SELECT STRING_AGG(a.id::TEXT, ',' ORDER BY a.name) \
             FROM ingredient_allergen ia \
             JOIN allergen a ON a.id = ia.allergen_id \
             WHERE ia.ingredient_id = i.id) \
            ORDER BY i.name \
        ) FILTER (WHERE i.id IS NOT NULL) AS allergen_id_sets, \
        ARRAY_AGG( \
            (SELECT STRING_AGG(a.name, ',' ORDER BY a.name) \
             FROM ingredient_allergen ia \
             JOIN allergen a ON a.id = ia.allergen_id \
             WHERE ia.ingredient_id = i.id) \
            ORDER BY i.name \
        ) FILTER (WHERE i.id IS NOT NULL) AS allergen_name_sets \
    FROM recipe r \
    LEFT JOIN recipe_ingredient ri ON ri.recipe_id = r.id \
    LEFT JOIN ingredient i ON i.id = ri.ingredient_id";

/// Fan out one aggregate row into the nested RecipeWithIngredientsRow
/// shape. Matches the TS zip at `recipes.ts:135-154` byte for byte.
fn fan_out(raw: WithIngredientsRaw) -> RecipeWithIngredientsRow {
    let iids = raw.ingredient_ids.unwrap_or_default();
    let inames = raw.ingredient_names.unwrap_or_default();
    let iamts = raw.ingredient_amounts.unwrap_or_default();
    let iunits = raw.ingredient_units.unwrap_or_default();
    let aid_sets = raw.allergen_id_sets.unwrap_or_default();
    let aname_sets = raw.allergen_name_sets.unwrap_or_default();

    let ingredients = iids
        .into_iter()
        .enumerate()
        .map(|(idx, iid)| {
            let name = inames.get(idx).cloned().unwrap_or_default();
            let amount = *iamts.get(idx).unwrap_or(&0.0);
            let unit = iunits.get(idx).cloned().unwrap_or_default();

            let aid_s = aid_sets.get(idx).and_then(|o| o.as_deref()).unwrap_or("");
            let aname_s = aname_sets.get(idx).and_then(|o| o.as_deref()).unwrap_or("");

            let aids: Vec<&str> = if aid_s.is_empty() {
                Vec::new()
            } else {
                aid_s.split(',').collect()
            };
            let anames: Vec<&str> = if aname_s.is_empty() {
                Vec::new()
            } else {
                aname_s.split(',').collect()
            };

            let allergen = aids
                .into_iter()
                .enumerate()
                .filter_map(|(aidx, aid)| {
                    let aid = Uuid::parse_str(aid).ok()?;
                    let aname = anames.get(aidx).cloned().unwrap_or("").to_string();
                    Some(AllergenEmbed {
                        id: aid,
                        name: aname,
                    })
                })
                .collect();

            RecipeIngredientRow {
                ingredient_id: iid,
                name,
                amount,
                unit,
                allergen,
            }
        })
        .collect();

    RecipeWithIngredientsRow {
        id: raw.id,
        name: raw.name,
        active: raw.active,
        created_at: raw.created_at,
        price_per_meal: raw.price_per_meal,
        ingredients,
    }
}

/// `getRecipesWithIngredients` — all recipes, each with nested ingredient.
pub async fn list_with_ingredients(
    pool: &PgPool,
    opts: &ListOpts,
) -> Result<Vec<RecipeWithIngredientsRow>, sqlx::Error> {
    let mut sql = String::from(WITH_INGREDIENTS_SELECT);
    if !opts.include_inactive {
        sql.push_str(" WHERE r.active = TRUE");
    }
    sql.push_str(" GROUP BY r.id ORDER BY r.name ASC");

    let rows: Vec<WithIngredientsRaw> = sqlx::query_as(&sql).fetch_all(pool).await?;
    Ok(rows.into_iter().map(fan_out).collect())
}

/// `getRecipesWithIngredientsByIds` — returns a flat Vec; the TS side
/// re-indexes into a Map post-fetch. We keep the Vec here and let the
/// handler shape the response.
pub async fn get_with_ingredients_by_ids(
    pool: &PgPool,
    ids: &[Uuid],
) -> Result<Vec<RecipeWithIngredientsRow>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut sql = String::from(WITH_INGREDIENTS_SELECT);
    sql.push_str(" WHERE r.id = ANY($1) GROUP BY r.id ORDER BY r.name ASC");

    let rows: Vec<WithIngredientsRaw> = sqlx::query_as(&sql).bind(ids).fetch_all(pool).await?;
    Ok(rows.into_iter().map(fan_out).collect())
}

/// `getRecipesWithIngredientsPaginated` — adds search filter + LIMIT/OFFSET.
/// Runtime SQL because of optional search.
pub async fn list_with_ingredients_paginated(
    pool: &PgPool,
    opts: &ListOpts,
    pagination: PaginationParams,
) -> Result<Paginated<RecipeWithIngredientsRow>, sqlx::Error> {
    // ---- Count: must apply the same WHERE as the data query. ----
    let mut count_qb: QueryBuilder<Postgres> =
        QueryBuilder::new("SELECT COUNT(*) AS count FROM recipe r");
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
        push_cond(&mut count_qb, "r.active = TRUE");
    }
    if let Some(ref s) = opts.search {
        push_cond(&mut count_qb, "LOWER(r.name) LIKE LOWER(");
        count_qb.push_bind(format!("%{}%", s));
        count_qb.push(")");
    }
    let total: i64 = count_qb.build_query_scalar::<i64>().fetch_one(pool).await?;

    // ---- Data ----
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(WITH_INGREDIENTS_SELECT);
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
        push_cond_d(&mut qb, "r.active = TRUE");
    }
    if let Some(ref s) = opts.search {
        push_cond_d(&mut qb, "LOWER(r.name) LIKE LOWER(");
        qb.push_bind(format!("%{}%", s));
        qb.push(")");
    }
    qb.push(" GROUP BY r.id ORDER BY r.name ASC LIMIT ");
    qb.push_bind(pagination.per_page);
    qb.push(" OFFSET ");
    qb.push_bind(offset(&pagination));

    let rows: Vec<WithIngredientsRaw> = qb
        .build_query_as::<WithIngredientsRaw>()
        .fetch_all(pool)
        .await?;
    let data: Vec<RecipeWithIngredientsRow> = rows.into_iter().map(fan_out).collect();

    Ok(build_paginated(data, total, pagination))
}

// =============================================================
// Writes — UNIQUE-name violation (23505) is translated at the
// handler layer, not here. This module just returns `sqlx::Error`
// and the handler maps the code.
// =============================================================

/// `createRecipe`. Trims name server-side (matches TS `data.name.trim()`).
/// Defaults: active = true.
pub async fn create(pool: &PgPool, data: InsertRecipeData) -> Result<RecipeRow, sqlx::Error> {
    let name = data.name.trim();
    let active = data.active.unwrap_or(true);
    sqlx::query_as!(
        RecipeRow,
        r#"
        INSERT INTO recipe (name, active)
        VALUES ($1, $2)
        RETURNING
            id                       AS "id!: Uuid",
            name                     AS "name!",
            active                   AS "active!",
            created_at               AS "created_at!: DateTime<Utc>",
            price_per_meal::float8   AS "price_per_meal?: f64"
        "#,
        name,
        active,
    )
    .fetch_one(pool)
    .await
}

/// `updateRecipe`. Three-state `price_per_meal` via the CASE expression —
/// identical to the TS CASE at `recipes.ts:411`.
///
/// `has_price` arg distinguishes "field absent" from "field present but
/// null". Decided from `data.price_per_meal.is_some()` (post-serde).
pub async fn update(
    pool: &PgPool,
    id: Uuid,
    data: UpdateRecipeData,
) -> Result<Option<RecipeRow>, sqlx::Error> {
    let name = data.name.as_deref().map(str::trim);
    let active = data.active;
    let has_price = data.price_per_meal.is_some();
    // `Some(None)` → None here (clear); `Some(Some(v))` → Some(v); `None` → None (unused when !has_price).
    let price: Option<f64> = data.price_per_meal.flatten();

    sqlx::query_as!(
        RecipeRow,
        r#"
        UPDATE recipe
        SET
            name           = COALESCE($1, name),
            active         = COALESCE($2, active),
            price_per_meal = CASE WHEN $4 THEN $3::float8::numeric ELSE price_per_meal END
        WHERE id = $5
        RETURNING
            id                       AS "id!: Uuid",
            name                     AS "name!",
            active                   AS "active!",
            created_at               AS "created_at!: DateTime<Utc>",
            price_per_meal::float8   AS "price_per_meal?: f64"
        "#,
        name,
        active,
        price,
        has_price,
        id,
    )
    .fetch_optional(pool)
    .await
}

/// `deactivateRecipe`. Soft delete; hard delete is blocked by
/// ON DELETE RESTRICT on orders. Mirrors the TS one-liner.
pub async fn deactivate(pool: &PgPool, id: Uuid) -> Result<Option<RecipeRow>, sqlx::Error> {
    update(
        pool,
        id,
        UpdateRecipeData {
            active: Some(false),
            ..Default::default()
        },
    )
    .await
}

// =============================================================
// Ingredient associations. Generic over any `PgExecutor` so the
// Rust importer can share a transaction — matches the TS
// `Pool | PoolClient` surface.
// =============================================================

/// `addRecipeIngredient` — upsert one line of the join row.
pub async fn add_ingredient<'c, E>(
    executor: E,
    recipe_id: Uuid,
    ingredient_id: Uuid,
    amount: f64,
    unit: &str,
) -> Result<(), sqlx::Error>
where
    E: PgExecutor<'c>,
{
    sqlx::query!(
        r#"
        INSERT INTO recipe_ingredient (recipe_id, ingredient_id, amount, unit)
        VALUES ($1, $2, $3::float8::numeric, $4)
        ON CONFLICT (recipe_id, ingredient_id)
        DO UPDATE SET amount = EXCLUDED.amount, unit = EXCLUDED.unit
        "#,
        recipe_id,
        ingredient_id,
        amount,
        unit,
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// `removeRecipeIngredient`.
pub async fn remove_ingredient<'c, E>(
    executor: E,
    recipe_id: Uuid,
    ingredient_id: Uuid,
) -> Result<(), sqlx::Error>
where
    E: PgExecutor<'c>,
{
    sqlx::query!(
        r#"
        DELETE FROM recipe_ingredient
        WHERE recipe_id = $1 AND ingredient_id = $2
        "#,
        recipe_id,
        ingredient_id,
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// `setRecipeIngredients` — atomic replace. Wraps DELETE + bulk INSERT
/// in a transaction. The TS surface accepts `Pool | PoolClient`; here
/// we always start our own transaction from a pool — the importer calls
/// `add_ingredient` directly inside its larger transaction, so this
/// endpoint is admin-API-only and can own its tx boundary.
pub async fn set_ingredients(
    pool: &PgPool,
    recipe_id: Uuid,
    ingredients: &[IngredientLine],
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query!(
        r#"DELETE FROM recipe_ingredient WHERE recipe_id = $1"#,
        recipe_id,
    )
    .execute(&mut *tx)
    .await?;

    if !ingredients.is_empty() {
        let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
            "INSERT INTO recipe_ingredient (recipe_id, ingredient_id, amount, unit) ",
        );
        qb.push_values(ingredients.iter(), |mut b, line| {
            b.push_bind(recipe_id)
                .push_bind(line.ingredient_id)
                .push_bind(line.amount)
                .push_bind(&line.unit);
        });
        qb.build().execute(&mut *tx).await?;
    }

    tx.commit().await?;
    Ok(())
}
