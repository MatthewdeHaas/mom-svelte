//! Menu + menu_recipe queries. TS counterpart is
//! `src/lib/server/queries/menus.ts` (290 lines, 13 exports). Menus are
//! GLOBAL master data (no `menu.company_id` — they're shared across
//! clients), so scoping follows the recipes model: reads are
//! unrestricted, writes are admin-only. The HTTP handler layer enforces
//! that split; this db module is a thin SQL layer.
//!
//! Schema notes (migration 001 + 004 rename):
//! ```sql
//! menu (
//!   id         UUID PK DEFAULT gen_random_uuid(),
//!   name       TEXT NOT NULL UNIQUE,
//!   is_active  BOOLEAN NOT NULL DEFAULT TRUE,
//!   start_date DATE NOT NULL,
//!   end_date   DATE NOT NULL,
//!   created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
//!   CHECK (end_date > start_date)
//! )
//! menu_recipe (
//!   id            UUID PK,
//!   menu_id       UUID NOT NULL REFERENCES menu(id) ON DELETE CASCADE,
//!   recipe_id     UUID NOT NULL REFERENCES recipe(id) ON DELETE RESTRICT,
//!   cycle_week    SMALLINT NOT NULL CHECK (cycle_week BETWEEN 1 AND 4),
//!   day_of_week   TEXT NOT NULL CHECK (day_of_week IN (...7 names)),
//!   meal_category TEXT NOT NULL,
//!   UNIQUE (menu_id, cycle_week, day_of_week, meal_category)
//! )
//! ```
//!
//! The TS `Menu` type types `start_date` / `end_date` as `Date | null`
//! but the DB is NOT NULL on both — the TS type is fiction. Rust uses
//! required `NaiveDate` and the create endpoint rejects missing dates
//! at the serde layer.
//!
//! `set_all_slots` uses `QueryBuilder::push_values`. Per the gotcha
//! from commit `7be4729f`, we trust sqlx's auto-comma between the stage
//! separator and VALUES — NO manual `.push(" ")`.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

// =============================================================
// Public row types — mirror `src/lib/types.ts`.
// =============================================================

/// Base menu row. Shape matches TS `Menu` interface. `start_date` and
/// `end_date` are required (DB-level NOT NULL); the TS type's `| null`
/// is a lie no SQL path ever produces.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MenuRow {
    pub id: Uuid,
    pub name: String,
    pub is_active: bool,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub created_at: DateTime<Utc>,
}

/// Menu recipe slot — base shape with no recipe name denormalised.
/// Matches TS `MenuRecipe`. Returned by `set_slot` which the UI uses
/// optimistically; display paths call `get_menu_recipes` for the
/// denormalised shape.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MenuRecipeRow {
    pub id: Uuid,
    pub menu_id: Uuid,
    pub recipe_id: Uuid,
    pub cycle_week: i16,
    pub day_of_week: String,
    pub meal_category: String,
}

/// Menu recipe slot with recipe name + active flag denormalised. Shape
/// matches TS `MenuRecipeWithName`. The TS surface uses this for every
/// slot read path — list, by-menu, by-coordinate — so we match.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MenuRecipeWithNameRow {
    pub id: Uuid,
    pub menu_id: Uuid,
    pub recipe_id: Uuid,
    pub cycle_week: i16,
    pub day_of_week: String,
    pub meal_category: String,
    pub recipe_name: String,
    pub recipe_active: bool,
}

/// Menu + nested recipe slots. Matches TS `MenuWithRecipes`. The
/// embedded `recipe` field is the TS surface's singular naming — the
/// Vec just happens to be the plural reality.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuWithRecipesRow {
    pub id: Uuid,
    pub name: String,
    pub is_active: bool,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub created_at: DateTime<Utc>,
    pub recipe: Vec<MenuRecipeWithNameRow>,
}

// =============================================================
// Input types for mutating endpoints.
// =============================================================

/// Filters shared by every menu-list endpoint. Mirrors
/// `getMenus({ includeInactive })` in the TS surface.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListOpts {
    #[serde(default)]
    pub include_inactive: bool,
}

/// Input to `create`. Mirrors TS `InsertMenu`. `start_date` /
/// `end_date` are required (DB-level NOT NULL); `is_active` defaults
/// to `true` server-side.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertMenuData {
    pub name: String,
    #[serde(default)]
    pub is_active: Option<bool>,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
}

/// Input to `update`. All fields optional. `None` → leave unchanged
/// (COALESCE), matching the TS `?? null` + COALESCE pattern.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateMenuData {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub is_active: Option<bool>,
    #[serde(default)]
    pub start_date: Option<NaiveDate>,
    #[serde(default)]
    pub end_date: Option<NaiveDate>,
}

/// Input to `set_slot` / `set_all_slots`. Mirrors TS `InsertMenuRecipe`
/// (`Omit<MenuRecipe, 'id'>`). `cycle_week` is `i16` to match the DB
/// `SMALLINT`; the TS surface types it as `1|2|3|4`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertMenuRecipeData {
    pub menu_id: Uuid,
    pub recipe_id: Uuid,
    pub cycle_week: i16,
    pub day_of_week: String,
    pub meal_category: String,
}

// =============================================================
// Reads — menu rows.
// =============================================================

/// `getMenus` — ordered `start_date DESC`. Two static branches so
/// `query_as!` can type-check both.
pub async fn list(pool: &PgPool, opts: &ListOpts) -> Result<Vec<MenuRow>, sqlx::Error> {
    if opts.include_inactive {
        sqlx::query_as!(
            MenuRow,
            r#"
            SELECT
                id         AS "id!: Uuid",
                name       AS "name!",
                is_active  AS "is_active!",
                start_date AS "start_date!: NaiveDate",
                end_date   AS "end_date!: NaiveDate",
                created_at AS "created_at!: DateTime<Utc>"
            FROM menu
            ORDER BY start_date DESC
            "#,
        )
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as!(
            MenuRow,
            r#"
            SELECT
                id         AS "id!: Uuid",
                name       AS "name!",
                is_active  AS "is_active!",
                start_date AS "start_date!: NaiveDate",
                end_date   AS "end_date!: NaiveDate",
                created_at AS "created_at!: DateTime<Utc>"
            FROM menu
            WHERE is_active = TRUE
            ORDER BY start_date DESC
            "#,
        )
        .fetch_all(pool)
        .await
    }
}

/// `getMenuById` — returns `None` if the row doesn't exist OR is
/// inactive (unless `include_inactive`). Matches TS semantics at
/// `menus.ts:31-44`.
pub async fn get_by_id(
    pool: &PgPool,
    id: Uuid,
    include_inactive: bool,
) -> Result<Option<MenuRow>, sqlx::Error> {
    if include_inactive {
        sqlx::query_as!(
            MenuRow,
            r#"
            SELECT
                id         AS "id!: Uuid",
                name       AS "name!",
                is_active  AS "is_active!",
                start_date AS "start_date!: NaiveDate",
                end_date   AS "end_date!: NaiveDate",
                created_at AS "created_at!: DateTime<Utc>"
            FROM menu
            WHERE id = $1
            "#,
            id,
        )
        .fetch_optional(pool)
        .await
    } else {
        sqlx::query_as!(
            MenuRow,
            r#"
            SELECT
                id         AS "id!: Uuid",
                name       AS "name!",
                is_active  AS "is_active!",
                start_date AS "start_date!: NaiveDate",
                end_date   AS "end_date!: NaiveDate",
                created_at AS "created_at!: DateTime<Utc>"
            FROM menu
            WHERE id = $1 AND is_active = TRUE
            "#,
            id,
        )
        .fetch_optional(pool)
        .await
    }
}

/// `getActiveMenuForDate` — the one active menu whose date range
/// covers `date`. Ordered `start_date DESC LIMIT 1` so if two
/// overlapping active menus somehow exist, the newer wins (matches TS
/// `menus.ts:132`).
pub async fn get_active_for_date(
    pool: &PgPool,
    date: NaiveDate,
) -> Result<Option<MenuRow>, sqlx::Error> {
    sqlx::query_as!(
        MenuRow,
        r#"
        SELECT
            id         AS "id!: Uuid",
            name       AS "name!",
            is_active  AS "is_active!",
            start_date AS "start_date!: NaiveDate",
            end_date   AS "end_date!: NaiveDate",
            created_at AS "created_at!: DateTime<Utc>"
        FROM menu
        WHERE is_active = TRUE
          AND start_date <= $1
          AND end_date   >= $1
        ORDER BY start_date DESC
        LIMIT 1
        "#,
        date,
    )
    .fetch_optional(pool)
    .await
}

// =============================================================
// Reads — slot rows.
// =============================================================

/// `getMenuRecipes` — all slots for a menu, ordered for display
/// (cycle_week ASC, day_of_week ASC, meal_category ASC). JOIN pulls
/// recipe name + active flag for denormalised display.
///
/// `day_of_week` is `text` with CHECK constraint — sorts alphabetically
/// (friday, monday, saturday, sunday, thursday, tuesday, wednesday).
/// Matches TS at `menus.ts:111` (which produces the same lexical order
/// as pg's default collation).
pub async fn get_menu_recipes(
    pool: &PgPool,
    menu_id: Uuid,
) -> Result<Vec<MenuRecipeWithNameRow>, sqlx::Error> {
    sqlx::query_as!(
        MenuRecipeWithNameRow,
        r#"
        SELECT
            mr.id            AS "id!: Uuid",
            mr.menu_id       AS "menu_id!: Uuid",
            mr.recipe_id     AS "recipe_id!: Uuid",
            mr.cycle_week    AS "cycle_week!: i16",
            mr.day_of_week   AS "day_of_week!",
            mr.meal_category AS "meal_category!",
            r.name           AS "recipe_name!",
            r.active         AS "recipe_active!"
        FROM menu_recipe mr
        JOIN recipe r ON r.id = mr.recipe_id
        WHERE mr.menu_id = $1
        ORDER BY mr.cycle_week ASC, mr.day_of_week ASC, mr.meal_category ASC
        "#,
        menu_id,
    )
    .fetch_all(pool)
    .await
}

/// `getMenuSlot` — single slot by (menu, cycle_week, day, meal).
/// Returns `None` on miss. Used by order creation to validate that a
/// recipe-day-slot combination exists in the active menu.
pub async fn get_slot(
    pool: &PgPool,
    menu_id: Uuid,
    cycle_week: i16,
    day_of_week: &str,
    meal_category: &str,
) -> Result<Option<MenuRecipeWithNameRow>, sqlx::Error> {
    sqlx::query_as!(
        MenuRecipeWithNameRow,
        r#"
        SELECT
            mr.id            AS "id!: Uuid",
            mr.menu_id       AS "menu_id!: Uuid",
            mr.recipe_id     AS "recipe_id!: Uuid",
            mr.cycle_week    AS "cycle_week!: i16",
            mr.day_of_week   AS "day_of_week!",
            mr.meal_category AS "meal_category!",
            r.name           AS "recipe_name!",
            r.active         AS "recipe_active!"
        FROM menu_recipe mr
        JOIN recipe r ON r.id = mr.recipe_id
        WHERE mr.menu_id       = $1
          AND mr.cycle_week    = $2
          AND mr.day_of_week   = $3
          AND mr.meal_category = $4
        "#,
        menu_id,
        cycle_week,
        day_of_week,
        meal_category,
    )
    .fetch_optional(pool)
    .await
}

// =============================================================
// Reads — nested (menu + recipe slots).
// =============================================================

/// `getMenuWithRecipes` — single menu with its slots. Layered: fetch
/// menu first, then its slots. Mirrors TS layering at `menus.ts:46-56`.
pub async fn get_with_recipes(
    pool: &PgPool,
    id: Uuid,
    include_inactive: bool,
) -> Result<Option<MenuWithRecipesRow>, sqlx::Error> {
    let Some(menu) = get_by_id(pool, id, include_inactive).await? else {
        return Ok(None);
    };
    let recipe = get_menu_recipes(pool, id).await?;
    Ok(Some(MenuWithRecipesRow {
        id: menu.id,
        name: menu.name,
        is_active: menu.is_active,
        start_date: menu.start_date,
        end_date: menu.end_date,
        created_at: menu.created_at,
        recipe,
    }))
}

/// `getMenusWithRecipes` — flat list of menus, each with nested slots.
/// Mirrors TS `menus.ts:60-107` — one query for menus, one for all slot
/// rows filtered by `menu_id = ANY($1)`, then group in-memory. Avoids
/// an N+1 pattern.
pub async fn list_with_recipes(
    pool: &PgPool,
    opts: &ListOpts,
) -> Result<Vec<MenuWithRecipesRow>, sqlx::Error> {
    let menus = list(pool, opts).await?;
    if menus.is_empty() {
        return Ok(Vec::new());
    }

    let menu_ids: Vec<Uuid> = menus.iter().map(|m| m.id).collect();

    let slot_rows = sqlx::query_as!(
        MenuRecipeWithNameRow,
        r#"
        SELECT
            mr.id            AS "id!: Uuid",
            mr.menu_id       AS "menu_id!: Uuid",
            mr.recipe_id     AS "recipe_id!: Uuid",
            mr.cycle_week    AS "cycle_week!: i16",
            mr.day_of_week   AS "day_of_week!",
            mr.meal_category AS "meal_category!",
            r.name           AS "recipe_name!",
            r.active         AS "recipe_active!"
        FROM menu_recipe mr
        JOIN recipe r ON r.id = mr.recipe_id
        WHERE mr.menu_id = ANY($1)
        ORDER BY mr.menu_id, mr.cycle_week, mr.day_of_week, mr.meal_category
        "#,
        &menu_ids,
    )
    .fetch_all(pool)
    .await?;

    // Group slot rows by menu_id. Preserving the slot query's ORDER BY
    // keeps slots within each menu in the right display order even
    // after grouping.
    use std::collections::HashMap;
    let mut by_menu: HashMap<Uuid, Vec<MenuRecipeWithNameRow>> = HashMap::new();
    for row in slot_rows {
        by_menu.entry(row.menu_id).or_default().push(row);
    }

    Ok(menus
        .into_iter()
        .map(|m| {
            let recipe = by_menu.remove(&m.id).unwrap_or_default();
            MenuWithRecipesRow {
                id: m.id,
                name: m.name,
                is_active: m.is_active,
                start_date: m.start_date,
                end_date: m.end_date,
                created_at: m.created_at,
                recipe,
            }
        })
        .collect())
}

// =============================================================
// Writes — menus. UNIQUE on `name` (23505 translated at handler layer).
// CHECK on (end_date > start_date) → 23514 translated at handler layer.
// =============================================================

/// `createMenu`. Trims name server-side (matches TS `data.name.trim()`
/// at `menus.ts:185`). Default: `is_active = true`.
pub async fn create(pool: &PgPool, data: InsertMenuData) -> Result<MenuRow, sqlx::Error> {
    let name = data.name.trim();
    let is_active = data.is_active.unwrap_or(true);
    sqlx::query_as!(
        MenuRow,
        r#"
        INSERT INTO menu (name, is_active, start_date, end_date)
        VALUES ($1, $2, $3, $4)
        RETURNING
            id         AS "id!: Uuid",
            name       AS "name!",
            is_active  AS "is_active!",
            start_date AS "start_date!: NaiveDate",
            end_date   AS "end_date!: NaiveDate",
            created_at AS "created_at!: DateTime<Utc>"
        "#,
        name,
        is_active,
        data.start_date,
        data.end_date,
    )
    .fetch_one(pool)
    .await
}

/// `updateMenu`. COALESCE on every field — `None` leaves the current
/// value in place. Trims `name` if present. Returns `None` if no row
/// matches `id`.
pub async fn update(
    pool: &PgPool,
    id: Uuid,
    data: UpdateMenuData,
) -> Result<Option<MenuRow>, sqlx::Error> {
    let name = data.name.as_deref().map(str::trim);
    sqlx::query_as!(
        MenuRow,
        r#"
        UPDATE menu
        SET
            name       = COALESCE($1, name),
            is_active  = COALESCE($2, is_active),
            start_date = COALESCE($3, start_date),
            end_date   = COALESCE($4, end_date)
        WHERE id = $5
        RETURNING
            id         AS "id!: Uuid",
            name       AS "name!",
            is_active  AS "is_active!",
            start_date AS "start_date!: NaiveDate",
            end_date   AS "end_date!: NaiveDate",
            created_at AS "created_at!: DateTime<Utc>"
        "#,
        name,
        data.is_active,
        data.start_date,
        data.end_date,
        id,
    )
    .fetch_optional(pool)
    .await
}

/// `deactivateMenu`. Soft delete — slots are preserved so re-activating
/// restores the full layout. Mirrors the TS one-liner at `menus.ts:215`.
pub async fn deactivate(pool: &PgPool, id: Uuid) -> Result<Option<MenuRow>, sqlx::Error> {
    update(
        pool,
        id,
        UpdateMenuData {
            is_active: Some(false),
            ..Default::default()
        },
    )
    .await
}

// =============================================================
// Writes — slots.
// =============================================================

/// `setMenuSlot` — upsert on the UNIQUE
/// `(menu_id, cycle_week, day_of_week, meal_category)` constraint.
/// Assigning a new recipe to an occupied slot replaces it cleanly.
pub async fn set_slot(
    pool: &PgPool,
    data: InsertMenuRecipeData,
) -> Result<MenuRecipeRow, sqlx::Error> {
    sqlx::query_as!(
        MenuRecipeRow,
        r#"
        INSERT INTO menu_recipe (menu_id, recipe_id, cycle_week, day_of_week, meal_category)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (menu_id, cycle_week, day_of_week, meal_category)
        DO UPDATE SET recipe_id = EXCLUDED.recipe_id
        RETURNING
            id            AS "id!: Uuid",
            menu_id       AS "menu_id!: Uuid",
            recipe_id     AS "recipe_id!: Uuid",
            cycle_week    AS "cycle_week!: i16",
            day_of_week   AS "day_of_week!",
            meal_category AS "meal_category!"
        "#,
        data.menu_id,
        data.recipe_id,
        data.cycle_week,
        data.day_of_week,
        data.meal_category,
    )
    .fetch_one(pool)
    .await
}

/// `clearMenuSlot` — delete one slot by coordinate. Returns `true` iff
/// a row was removed. Mirrors the TS `rowCount > 0` return.
pub async fn clear_slot(
    pool: &PgPool,
    menu_id: Uuid,
    cycle_week: i16,
    day_of_week: &str,
    meal_category: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        DELETE FROM menu_recipe
        WHERE menu_id       = $1
          AND cycle_week    = $2
          AND day_of_week   = $3
          AND meal_category = $4
        "#,
        menu_id,
        cycle_week,
        day_of_week,
        meal_category,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// `setAllMenuSlots` — atomic full-replace. DELETE + bulk INSERT in one
/// transaction. Uses `QueryBuilder::push_values` for the multi-row
/// insert; per the gotcha from commit `7be4729f`, sqlx auto-commas
/// between values — NO manual `.push(" ")` between the stage separator
/// and VALUES.
pub async fn set_all_slots(
    pool: &PgPool,
    menu_id: Uuid,
    slots: &[InsertMenuRecipeData],
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query!(r#"DELETE FROM menu_recipe WHERE menu_id = $1"#, menu_id,)
        .execute(&mut *tx)
        .await?;

    if !slots.is_empty() {
        let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
            "INSERT INTO menu_recipe (menu_id, recipe_id, cycle_week, day_of_week, meal_category) ",
        );
        qb.push_values(slots.iter(), |mut b, s| {
            b.push_bind(s.menu_id)
                .push_bind(s.recipe_id)
                .push_bind(s.cycle_week)
                .push_bind(&s.day_of_week)
                .push_bind(&s.meal_category);
        });
        qb.build().execute(&mut *tx).await?;
    }

    tx.commit().await?;
    Ok(())
}
