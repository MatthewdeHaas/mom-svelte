//! Financials queries. TS counterpart is `src/lib/server/queries/financials.ts`
//! (183 lines, 3 exports). Admin-only across the board — the one consumer
//! (`src/routes/admin/financials/+page.server.ts`) is behind the admin
//! guard already.
//!
//! Three functions, three shapes:
//!
//! 1. `get_recipes_with_margins` — flat list of all active recipes with
//!    aggregated ingredient cost. LEFT JOIN so recipes with no ingredients
//!    appear with `ingredient_cost = NULL`.
//! 2. `get_recipes_with_margins_paginated` — same shape, paginated. Count
//!    query ignores the join (counts recipes, not ingredient rows).
//! 3. `get_invoice_data` — company + line items for confirmed/fulfilled
//!    orders in a date range. Two round-trips: company lookup + line
//!    item join. Returns `None` when the company id doesn't exist.
//!
//! **Numeric columns are cast to `float8`** in SQL for the same reason
//! orders/recipes do it: `NUMERIC` has no safe f64 decode in sqlx without
//! pulling `rust_decimal`/`bigdecimal`, and the TS side already does
//! `parseFloat` on every TEXT-cast cell. Multiplications
//! (`o.amount * r.price_per_meal`, `ri.amount * i.cost_per_unit`) happen
//! in SQL so byte-identical rounding is Postgres's problem, not Rust's.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

// =============================================================
// Public row types — mirror `src/lib/server/queries/financials.ts`
// `RecipeMargin` / `InvoiceLineItem` / `InvoiceSummary`.
// =============================================================

/// One row of recipe-margin data. `price_per_meal` is nullable (recipe
/// may not be priced), `ingredient_cost` is nullable (recipe may have
/// no ingredients). `margin` and `margin_pct` are derived in Rust once
/// both operands are known.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeMarginRow {
    pub id: Uuid,
    pub name: String,
    pub price_per_meal: Option<f64>,
    pub ingredient_cost: Option<f64>,
    pub margin: Option<f64>,
    pub margin_pct: Option<f64>,
}

/// Invoice line item — one order × its recipe pricing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceLineItem {
    pub order_id: Uuid,
    pub recipe_name: String,
    pub serving_date: NaiveDate,
    pub amount: f64,
    pub unit_price: f64,
    pub line_total: f64,
}

/// Full invoice payload. `None` returned at the handler layer when the
/// company id doesn't exist (TS surface returns `null` on the same
/// condition).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceSummary {
    pub company_id: Uuid,
    pub company_name: String,
    pub shipping_address: Option<String>,
    pub line_items: Vec<InvoiceLineItem>,
    pub subtotal: f64,
}

// =============================================================
// Internal helper — derive `margin` + `margin_pct` from the two
// nullable operands. Pulled out of both callers so the arithmetic
// stays in one place and matches `financials.ts:65-73` byte-for-byte.
// =============================================================

fn derive_margin(price: Option<f64>, cost: Option<f64>) -> (Option<f64>, Option<f64>) {
    match (price, cost) {
        (Some(p), Some(c)) => {
            let margin = p - c;
            let pct = if p > 0.0 { Some((p - c) / p * 100.0) } else { None };
            (Some(margin), pct)
        }
        _ => (None, None),
    }
}

// =============================================================
// Reads
// =============================================================

/// `getRecipesWithMargins` — one row per active recipe, with the sum of
/// ingredient costs. LEFT JOINs so a recipe with no ingredients still
/// appears (ingredient_cost = NULL).
pub async fn get_recipes_with_margins(
    pool: &PgPool,
) -> Result<Vec<RecipeMarginRow>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"
        SELECT
            r.id                                   AS "id!: Uuid",
            r.name                                 AS "name!",
            r.price_per_meal::float8               AS "price_per_meal?: f64",
            SUM(ri.amount * i.cost_per_unit)::float8 AS "ingredient_cost?: f64"
        FROM recipe r
        LEFT JOIN recipe_ingredient ri ON ri.recipe_id = r.id
        LEFT JOIN ingredient i ON i.id = ri.ingredient_id
        WHERE r.active = TRUE
        GROUP BY r.id
        ORDER BY r.name ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let (margin, margin_pct) = derive_margin(r.price_per_meal, r.ingredient_cost);
            RecipeMarginRow {
                id: r.id,
                name: r.name,
                price_per_meal: r.price_per_meal,
                ingredient_cost: r.ingredient_cost,
                margin,
                margin_pct,
            }
        })
        .collect())
}

/// `getRecipesWithMarginsPaginated` — same shape, bounded by LIMIT/OFFSET.
/// Count query ignores the join (counts recipes, not ingredient join
/// rows). Static WHERE (`active = TRUE`) means `query_as!`-friendly
/// shape, but we use `query!` to accommodate the ARRAY_AGG-style
/// aggregates.
pub async fn get_recipes_with_margins_paginated(
    pool: &PgPool,
    pagination: super::pagination::PaginationParams,
) -> Result<super::pagination::Paginated<RecipeMarginRow>, sqlx::Error> {
    // ---- Count ----
    // `COUNT(*)` on a bare recipe table so we count recipes, not join
    // rows. Matches TS `financials.ts:81`.
    let total: i64 = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!: i64" FROM recipe WHERE active = TRUE"#,
    )
    .fetch_one(pool)
    .await?;

    // ---- Data ----
    let offset = super::pagination::offset(&pagination);
    let rows = sqlx::query!(
        r#"
        SELECT
            r.id                                     AS "id!: Uuid",
            r.name                                   AS "name!",
            r.price_per_meal::float8                 AS "price_per_meal?: f64",
            SUM(ri.amount * i.cost_per_unit)::float8 AS "ingredient_cost?: f64"
        FROM recipe r
        LEFT JOIN recipe_ingredient ri ON ri.recipe_id = r.id
        LEFT JOIN ingredient i ON i.id = ri.ingredient_id
        WHERE r.active = TRUE
        GROUP BY r.id
        ORDER BY r.name ASC
        LIMIT $1 OFFSET $2
        "#,
        pagination.per_page,
        offset,
    )
    .fetch_all(pool)
    .await?;

    let data: Vec<RecipeMarginRow> = rows
        .into_iter()
        .map(|r| {
            let (margin, margin_pct) = derive_margin(r.price_per_meal, r.ingredient_cost);
            RecipeMarginRow {
                id: r.id,
                name: r.name,
                price_per_meal: r.price_per_meal,
                ingredient_cost: r.ingredient_cost,
                margin,
                margin_pct,
            }
        })
        .collect();

    Ok(super::pagination::build(data, total, pagination))
}

/// `getInvoiceData` — company header + line items for all confirmed /
/// fulfilled orders in the date range. Two round-trips (one for the
/// company header, one for the line items) matching the TS surface at
/// `financials.ts:131-162`.
///
/// Returns `None` when `company_id` doesn't exist. An existing company
/// with zero matching orders returns `Some` with an empty `line_items`
/// vec and `subtotal = 0.0`.
pub async fn get_invoice_data(
    pool: &PgPool,
    company_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Option<InvoiceSummary>, sqlx::Error> {
    // ---- Company header ----
    let company = sqlx::query!(
        r#"
        SELECT
            id               AS "id!: Uuid",
            name             AS "name!",
            shipping_address
        FROM company
        WHERE id = $1
        "#,
        company_id,
    )
    .fetch_optional(pool)
    .await?;

    let Some(company) = company else {
        return Ok(None);
    };

    // ---- Line items ----
    //
    // Status filter mirrors TS exactly: `('confirmed', 'fulfilled')`.
    // Multiplication happens in SQL so pg's NUMERIC rounding governs
    // the result, same as the TS `(o.amount * r.price_per_meal)::TEXT`
    // path. `price_per_meal IS NOT NULL` filter drops unpriced recipes
    // — matches TS `financials.ts:160`.
    let line_rows = sqlx::query!(
        r#"
        SELECT
            o.id                               AS "order_id!: Uuid",
            r.name                             AS "recipe_name!",
            o.serving_date                     AS "serving_date!: NaiveDate",
            o.amount::float8                   AS "amount!: f64",
            r.price_per_meal::float8           AS "unit_price!: f64",
            (o.amount * r.price_per_meal)::float8 AS "line_total!: f64"
        FROM "order" o
        JOIN recipe r ON r.id = o.recipe_id
        WHERE o.company_id = $1
          AND o.status IN ('confirmed', 'fulfilled')
          AND o.serving_date >= $2
          AND o.serving_date <= $3
          AND r.price_per_meal IS NOT NULL
        ORDER BY o.serving_date, r.name
        "#,
        company_id,
        from,
        to,
    )
    .fetch_all(pool)
    .await?;

    let line_items: Vec<InvoiceLineItem> = line_rows
        .into_iter()
        .map(|r| InvoiceLineItem {
            order_id: r.order_id,
            recipe_name: r.recipe_name,
            serving_date: r.serving_date,
            amount: r.amount,
            unit_price: r.unit_price,
            line_total: r.line_total,
        })
        .collect();

    let subtotal: f64 = line_items.iter().map(|li| li.line_total).sum();

    Ok(Some(InvoiceSummary {
        company_id: company.id,
        company_name: company.name,
        shipping_address: company.shipping_address,
        line_items,
        subtotal,
    }))
}
