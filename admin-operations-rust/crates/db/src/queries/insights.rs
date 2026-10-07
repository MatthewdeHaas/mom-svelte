//! Admin-insights aggregations. TS counterpart is
//! `src/lib/server/queries/insights.ts` (~384 LOC, 8 exports). All
//! consumers live under `/admin/insights/*` (plus the admin KPI
//! tile) — this surface is admin-only end-to-end.
//!
//! **Design note — treat the TS as a spec, not a template.** Two of
//! the eight functions have post-SQL work that is really a windowed
//! aggregate; we collapse them here:
//!
//! - `get_kpi_metrics` was 4 parallel queries in TS plus a manual
//!   `Record<OrderStatus, number>` build. Rust uses one CTE query
//!   that computes current-window totals and status counts via
//!   `FILTER (WHERE …)`, plus a one-row prior-window CTE for the
//!   week-over-week comparison.
//! - `get_top_recipes` was `GROUP BY` + a second TS loop computing
//!   `percent_of_total` from the grand total. Rust inlines the grand
//!   total with `SUM(SUM(amount)) OVER ()`, no post-SQL pass.
//!
//! The remaining six functions are already single-query aggregates
//! and port straight across. Their SQL is unchanged apart from
//! `::float8` casts (sqlx has no safe NUMERIC → f64 decode without
//! pulling `rust_decimal`; the TS side already `parseFloat`s every
//! TEXT-cast cell, so the semantics are identical).
//!
//! Schema notes:
//!  - `"order"` is a reserved word → always quoted in SQL.
//!  - `attendance.serving_date` is `DATE NOT NULL`; `attendance.attending`
//!    is `BOOLEAN NOT NULL`. Rate math stays in SQL (`ROUND(numeric, 1)`)
//!    to keep rounding identical to the TS path.
//!  - `person.active = TRUE` filter mirrors the TS surface — ex-tenants
//!    are excluded from attendance aggregates even if their rows persist.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

// =============================================================
// Public row types — mirror `src/lib/server/queries/insights.ts`
// 1:1. Field names stay snake_case where the TS surface already uses
// snake_case; camelCase at the top-level `KpiMetrics` struct matches
// the TS `interface KPIMetrics` property casing.
// =============================================================

/// Order status breakdown. Fields are the four order statuses from
/// migration 002's CHECK constraint. Serialized with lowercase keys
/// to match the TS `Record<OrderStatus, number>` wire shape.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatusCounts {
    pub pending: i64,
    pub confirmed: i64,
    pub fulfilled: i64,
    pub cancelled: i64,
}

/// KPI tile payload for the admin landing page. `week_over_week_change`
/// is `None` when the prior window has zero meals (division-by-zero
/// guard, matches TS `weekOverWeekChange: number | null`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KpiMetrics {
    pub total_meals: f64,
    pub total_meals_prior_week: f64,
    pub week_over_week_change: Option<i32>,
    pub status_counts: StatusCounts,
    pub active_companies: i64,
}

/// One point on the order-volume time series. `date` is the bucket
/// start (daily or weekly) as `YYYY-MM-DD` — matches TS surface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeSeriesRow {
    pub date: NaiveDate,
    pub total: f64,
}

/// Top-recipe leaderboard row. `percent_of_total` is computed in SQL
/// via `SUM(SUM()) OVER ()` so the TS post-aggregate loop is gone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopRecipeRow {
    pub name: String,
    pub order_count: i64,
    pub total_amount: f64,
    pub percent_of_total: f64,
}

/// Per-company totals in a window. Ordered by `total_amount DESC`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyBreakdownRow {
    pub name: String,
    pub total_amount: f64,
    pub order_count: i64,
}

/// Per-source totals in a window (`admin_manual` / `excel_import` /
/// `client_portal`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMixRow {
    pub order_source: String,
    pub total_amount: f64,
    pub order_count: i64,
}

/// Projected ingredient demand for pending + confirmed orders in a
/// window. `total_amount` is `SUM(ri.amount * o.amount)` — the recipe
/// per-unit × orders-of-that-recipe. Allergens are a comma-joined
/// alphabetical list for display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngredientDemandRow {
    pub name: String,
    pub total_amount: f64,
    pub unit: String,
    pub allergens: String,
}

/// Daily attendance-rate point. `rate` is a percentage with one
/// decimal of precision — rounding stays in Postgres via `ROUND(…, 1)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttendanceDayRateRow {
    pub date: NaiveDate,
    pub attending: i64,
    pub total: i64,
    pub rate: f64,
}

/// Per-company attendance rate across the whole window. Ordered by
/// `rate DESC` (the highest-attendance companies first).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttendanceByCompanyRow {
    pub company_name: String,
    pub attending: i64,
    pub total: i64,
    pub rate: f64,
}

// =============================================================
// 1. `getKPIMetrics` — one query via CTE + FILTER (WHERE…).
// =============================================================

/// Collapses the TS surface's four parallel queries into a single
/// round-trip. Status counts land as separate `FILTER` aggregates
/// (not `GROUP BY status`) so a window with zero of a given status
/// still produces a 0 — otherwise we'd have to fill the gaps on the
/// client. Prior-window totals come from a sibling CTE joined via
/// cross-product (both are single-row, so the cross-product is 1×1).
///
/// Week-over-week math stays in Rust so the rounding (`Math.round`)
/// matches the TS surface byte-for-byte.
pub async fn get_kpi_metrics(
    pool: &PgPool,
    from: NaiveDate,
    to: NaiveDate,
    prior_from: NaiveDate,
    prior_to: NaiveDate,
) -> Result<KpiMetrics, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        WITH current_window AS (
            SELECT
                COALESCE(SUM(amount) FILTER (WHERE status != 'cancelled'), 0)::float8
                    AS total_meals,
                COUNT(*) FILTER (WHERE status = 'pending')   AS pending,
                COUNT(*) FILTER (WHERE status = 'confirmed') AS confirmed,
                COUNT(*) FILTER (WHERE status = 'fulfilled') AS fulfilled,
                COUNT(*) FILTER (WHERE status = 'cancelled') AS cancelled,
                COUNT(DISTINCT company_id) FILTER (WHERE status != 'cancelled')
                    AS active_companies
            FROM "order"
            WHERE serving_date >= $1 AND serving_date <= $2
        ),
        prior_window AS (
            SELECT
                COALESCE(SUM(amount) FILTER (WHERE status != 'cancelled'), 0)::float8
                    AS total_meals_prior
            FROM "order"
            WHERE serving_date >= $3 AND serving_date <= $4
        )
        SELECT
            c.total_meals        AS "total_meals!: f64",
            p.total_meals_prior  AS "total_meals_prior!: f64",
            c.pending            AS "pending!: i64",
            c.confirmed          AS "confirmed!: i64",
            c.fulfilled          AS "fulfilled!: i64",
            c.cancelled          AS "cancelled!: i64",
            c.active_companies   AS "active_companies!: i64"
        FROM current_window c, prior_window p
        "#,
        from,
        to,
        prior_from,
        prior_to,
    )
    .fetch_one(pool)
    .await?;

    let week_over_week_change = if row.total_meals_prior > 0.0 {
        Some(((row.total_meals - row.total_meals_prior) / row.total_meals_prior * 100.0).round() as i32)
    } else {
        None
    };

    Ok(KpiMetrics {
        total_meals: row.total_meals,
        total_meals_prior_week: row.total_meals_prior,
        week_over_week_change,
        status_counts: StatusCounts {
            pending: row.pending,
            confirmed: row.confirmed,
            fulfilled: row.fulfilled,
            cancelled: row.cancelled,
        },
        active_companies: row.active_companies,
    })
}

// =============================================================
// 2. `getOrderVolumeSeries` — daily or weekly `GROUP BY`.
// =============================================================

/// Returns one row per day (or ISO-week start, when `granularity ==
/// "weekly"`). Direct translation of the TS SQL — no logic lives on
/// the client side anymore (no `parseFloat` round-trip).
pub async fn get_order_volume_series(
    pool: &PgPool,
    from: NaiveDate,
    to: NaiveDate,
    granularity: &str,
) -> Result<Vec<VolumeSeriesRow>, sqlx::Error> {
    if granularity == "weekly" {
        sqlx::query_as!(
            VolumeSeriesRow,
            r#"
            SELECT
                DATE_TRUNC('week', serving_date)::date AS "date!: NaiveDate",
                SUM(amount)::float8                    AS "total!: f64"
            FROM "order"
            WHERE serving_date >= $1 AND serving_date <= $2
              AND status != 'cancelled'
            GROUP BY DATE_TRUNC('week', serving_date)
            ORDER BY 1
            "#,
            from,
            to,
        )
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as!(
            VolumeSeriesRow,
            r#"
            SELECT
                serving_date::date  AS "date!: NaiveDate",
                SUM(amount)::float8 AS "total!: f64"
            FROM "order"
            WHERE serving_date >= $1 AND serving_date <= $2
              AND status != 'cancelled'
            GROUP BY serving_date::date
            ORDER BY 1
            "#,
            from,
            to,
        )
        .fetch_all(pool)
        .await
    }
}

// =============================================================
// 3. `getTopRecipes` — single query, grand total via window fn.
// =============================================================

/// Replaces the TS post-SQL `grandTotal = rows.reduce(...)` loop
/// with `SUM(SUM(amount)) OVER ()`. Rounding stays in SQL
/// (`ROUND(x * 1000)::float8 / 10`) so the one-decimal percent
/// shape is identical to `Math.round(x * 1000) / 10`.
pub async fn get_top_recipes(
    pool: &PgPool,
    from: NaiveDate,
    to: NaiveDate,
    limit: i64,
) -> Result<Vec<TopRecipeRow>, sqlx::Error> {
    sqlx::query_as!(
        TopRecipeRow,
        r#"
        SELECT
            r.name                   AS "name!",
            COUNT(o.id)              AS "order_count!: i64",
            SUM(o.amount)::float8    AS "total_amount!: f64",
            CASE
              WHEN SUM(SUM(o.amount)) OVER () > 0
              THEN ROUND((SUM(o.amount) / SUM(SUM(o.amount)) OVER ()) * 1000)::float8 / 10
              ELSE 0::float8
            END                      AS "percent_of_total!: f64"
        FROM "order" o
        JOIN recipe r ON r.id = o.recipe_id
        WHERE o.serving_date >= $1 AND o.serving_date <= $2
          AND o.status != 'cancelled'
        GROUP BY r.id, r.name
        ORDER BY 3 DESC
        LIMIT $3
        "#,
        from,
        to,
        limit,
    )
    .fetch_all(pool)
    .await
}

// =============================================================
// 4. `getCompanyBreakdown` — straight translation.
// =============================================================

pub async fn get_company_breakdown(
    pool: &PgPool,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<CompanyBreakdownRow>, sqlx::Error> {
    sqlx::query_as!(
        CompanyBreakdownRow,
        r#"
        SELECT
            c.name                 AS "name!",
            SUM(o.amount)::float8  AS "total_amount!: f64",
            COUNT(o.id)            AS "order_count!: i64"
        FROM "order" o
        JOIN company c ON c.id = o.company_id
        WHERE o.serving_date >= $1 AND o.serving_date <= $2
          AND o.status != 'cancelled'
        GROUP BY c.id, c.name
        ORDER BY 2 DESC
        "#,
        from,
        to,
    )
    .fetch_all(pool)
    .await
}

// =============================================================
// 5. `getSourceMix` — straight translation.
// =============================================================

pub async fn get_source_mix(
    pool: &PgPool,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<SourceMixRow>, sqlx::Error> {
    sqlx::query_as!(
        SourceMixRow,
        r#"
        SELECT
            order_source           AS "order_source!",
            SUM(amount)::float8    AS "total_amount!: f64",
            COUNT(*)               AS "order_count!: i64"
        FROM "order"
        WHERE serving_date >= $1 AND serving_date <= $2
          AND status != 'cancelled'
        GROUP BY order_source
        "#,
        from,
        to,
    )
    .fetch_all(pool)
    .await
}

// =============================================================
// 6. `getIngredientDemand` — SUM(product) + STRING_AGG.
// =============================================================

/// `SUM(ri.amount * o.amount)` is per-unit × per-order units ordered.
/// `STRING_AGG(DISTINCT …)` matches the TS surface's allergen list
/// shape (`"Dairy, Gluten"`). The TS surface returns `""` for a null
/// allergen aggregate / unit; we null-coalesce at the row builder
/// below rather than in SQL so sqlx doesn't force a nullable column
/// type.
pub async fn get_ingredient_demand(
    pool: &PgPool,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<IngredientDemandRow>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"
        SELECT
            i.name                             AS "name!",
            SUM(ri.amount * o.amount)::float8  AS "total_amount!: f64",
            ri.unit                            AS "unit?",
            STRING_AGG(DISTINCT a.name, ', ' ORDER BY a.name) AS "allergens?"
        FROM "order" o
        JOIN recipe r             ON r.id  = o.recipe_id
        JOIN recipe_ingredient ri ON ri.recipe_id = r.id
        JOIN ingredient i         ON i.id  = ri.ingredient_id
        LEFT JOIN ingredient_allergen ia ON ia.ingredient_id = i.id
        LEFT JOIN allergen a             ON a.id  = ia.allergen_id
        WHERE o.serving_date >= $1 AND o.serving_date <= $2
          AND o.status IN ('pending', 'confirmed')
        GROUP BY i.id, i.name, ri.unit
        ORDER BY i.name
        "#,
        from,
        to,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| IngredientDemandRow {
            name: r.name,
            total_amount: r.total_amount,
            unit: r.unit.unwrap_or_default(),
            allergens: r.allergens.unwrap_or_default(),
        })
        .collect())
}

// =============================================================
// 7. `getAttendanceRate` — daily, `COUNT(*) FILTER` + ROUND.
// =============================================================

pub async fn get_attendance_rate(
    pool: &PgPool,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<AttendanceDayRateRow>, sqlx::Error> {
    sqlx::query_as!(
        AttendanceDayRateRow,
        r#"
        SELECT
            a.serving_date::date                            AS "date!: NaiveDate",
            COUNT(*) FILTER (WHERE a.attending = true)      AS "attending!: i64",
            COUNT(*)                                        AS "total!: i64",
            ROUND(
              COUNT(*) FILTER (WHERE a.attending = true)::numeric
              / NULLIF(COUNT(*), 0) * 100, 1
            )::float8                                        AS "rate!: f64"
        FROM attendance a
        JOIN person p ON p.id = a.person_id
        WHERE a.serving_date >= $1 AND a.serving_date <= $2
          AND p.active = true
        GROUP BY a.serving_date::date
        ORDER BY 1
        "#,
        from,
        to,
    )
    .fetch_all(pool)
    .await
}

// =============================================================
// 8. `getAttendanceByCompany` — same shape, grouped by company.
// =============================================================

pub async fn get_attendance_by_company(
    pool: &PgPool,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<AttendanceByCompanyRow>, sqlx::Error> {
    sqlx::query_as!(
        AttendanceByCompanyRow,
        r#"
        SELECT
            c.name                                          AS "company_name!",
            COUNT(*) FILTER (WHERE a.attending = true)      AS "attending!: i64",
            COUNT(*)                                        AS "total!: i64",
            ROUND(
              COUNT(*) FILTER (WHERE a.attending = true)::numeric
              / NULLIF(COUNT(*), 0) * 100, 1
            )::float8                                        AS "rate!: f64"
        FROM attendance a
        JOIN person p  ON p.id = a.person_id
        JOIN company c ON c.id = p.company_id
        WHERE a.serving_date >= $1 AND a.serving_date <= $2
          AND p.active = true
        GROUP BY c.id, c.name
        ORDER BY 4 DESC
        "#,
        from,
        to,
    )
    .fetch_all(pool)
    .await
}
