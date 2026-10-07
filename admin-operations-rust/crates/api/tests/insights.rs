//! Parity + behavior tests for `/api/insights/*`.
//!
//! The TS surface has no vitest coverage — this port adds one on the
//! Rust side with the full HTTP boundary (admin vs client scoping,
//! query-param validation, status-code shape) that the TS can't
//! express without a route.
//!
//! Shape:
//! - 8 endpoints, admin-only across the board.
//! - Numeric columns are `::float8`-cast in SQL; we assert on `f64`
//!   with exact equality where the seeded inputs admit it (small
//!   integers × 2-decimal prices → no float drift) and `(a - b).abs()
//!   < 0.05` on the one spot where ROUND(…, 1) is in play.
//! - Each test runs against a fresh `#[sqlx::test]` DB so cases are
//!   embarrassingly parallel — no shared seeded state.

mod common;

use chrono::NaiveDate;
use common::{companies::seed_company, seed_user, signed_headers, spawn_server, TestServer};
use db::queries::insights::{
    AttendanceByCompanyRow, AttendanceDayRateRow, CompanyBreakdownRow, IngredientDemandRow,
    KpiMetrics, SourceMixRow, TopRecipeRow, VolumeSeriesRow,
};
use reqwest::{Client, StatusCode};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Tiny HTTP helpers — same shape as financials/menus test files.
// ---------------------------------------------------------------------------

async fn get_admin(server: &TestServer, user_id: &str, path: &str) -> reqwest::Response {
    let mut req = Client::new().get(format!("{}{}", server.base_url, path));
    for (k, v) in signed_headers(user_id, "admin", "") {
        req = req.header(k, v);
    }
    req.send().await.expect("GET admin")
}

async fn get_client(
    server: &TestServer,
    user_id: &str,
    company_id: Uuid,
    path: &str,
) -> reqwest::Response {
    let mut req = Client::new().get(format!("{}{}", server.base_url, path));
    for (k, v) in signed_headers(user_id, "client", &company_id.to_string()) {
        req = req.header(k, v);
    }
    req.send().await.expect("GET client")
}

async fn decode_ok<T: serde::de::DeserializeOwned>(res: reqwest::Response) -> T {
    let status = res.status();
    let text = res.text().await.expect("body");
    assert!(status.is_success(), "non-2xx: status={status} body={text}");
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("decode: {e}; body={text}"))
}

// ---------------------------------------------------------------------------
// Seed helpers — raw SQL. The insights surface aggregates across orders,
// attendance, and recipe→ingredient→allergen chains. We seed each
// directly so no test depends on another domain's create endpoints.
// ---------------------------------------------------------------------------

/// Seed a recipe with an optional `price_per_meal`. Returns id.
async fn seed_recipe(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO recipe (name, active, price_per_meal) VALUES ($1, TRUE, NULL) RETURNING id",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("seed recipe")
}

async fn seed_ingredient(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO ingredient (name, active, cost_per_unit) VALUES ($1, TRUE, NULL) RETURNING id",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("seed ingredient")
}

async fn seed_allergen(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>("INSERT INTO allergen (name) VALUES ($1) RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await
        .expect("seed allergen")
}

async fn attach_recipe_ingredient(
    pool: &PgPool,
    recipe_id: Uuid,
    ingredient_id: Uuid,
    amount: f64,
    unit: &str,
) {
    sqlx::query(
        "INSERT INTO recipe_ingredient (recipe_id, ingredient_id, amount, unit) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(recipe_id)
    .bind(ingredient_id)
    .bind(amount)
    .bind(unit)
    .execute(pool)
    .await
    .expect("attach recipe_ingredient");
}

async fn attach_ingredient_allergen(pool: &PgPool, ingredient_id: Uuid, allergen_id: Uuid) {
    sqlx::query(
        "INSERT INTO ingredient_allergen (ingredient_id, allergen_id) VALUES ($1, $2)",
    )
    .bind(ingredient_id)
    .bind(allergen_id)
    .execute(pool)
    .await
    .expect("attach ingredient_allergen");
}

/// Seed one order with explicit (status, source, amount, serving_date).
/// `created_by = NULL` sidesteps the user FK — the insights surface
/// doesn't care who placed the order.
async fn seed_order(
    pool: &PgPool,
    company_id: Uuid,
    recipe_id: Uuid,
    status: &str,
    source: &str,
    amount: f64,
    serving_date: NaiveDate,
) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        r#"INSERT INTO "order"
           (company_id, recipe_id, order_source, status, meal_category,
            amount, serving_date, shipment_date, frozen, chilled, recurring)
           VALUES ($1, $2, $3, $4, 'Lunch', $5, $6, $6, FALSE, FALSE, FALSE)
           RETURNING id"#,
    )
    .bind(company_id)
    .bind(recipe_id)
    .bind(source)
    .bind(status)
    .bind(amount)
    .bind(serving_date)
    .fetch_one(pool)
    .await
    .expect("seed order")
}

async fn seed_person(pool: &PgPool, company_id: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO person (company_id, name, active) VALUES ($1, $2, TRUE) RETURNING id",
    )
    .bind(company_id)
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("seed person")
}

async fn seed_person_inactive(pool: &PgPool, company_id: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO person (company_id, name, active) VALUES ($1, $2, FALSE) RETURNING id",
    )
    .bind(company_id)
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("seed person inactive")
}

async fn seed_attendance(
    pool: &PgPool,
    person_id: Uuid,
    serving_date: NaiveDate,
    attending: bool,
) {
    sqlx::query(
        "INSERT INTO attendance (person_id, serving_date, attending) VALUES ($1, $2, $3)",
    )
    .bind(person_id)
    .bind(serving_date)
    .bind(attending)
    .execute(pool)
    .await
    .expect("seed attendance");
}

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("valid date")
}

// ===========================================================================
// GET /api/insights/kpi — CTE + FILTER rewrite
// ===========================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn kpi_empty_window_returns_zeros(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let kpi: KpiMetrics = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/insights/kpi?from=2026-06-01&to=2026-06-30\
             &prior_from=2026-05-01&prior_to=2026-05-31",
        )
        .await,
    )
    .await;
    assert_eq!(kpi.total_meals, 0.0);
    assert_eq!(kpi.total_meals_prior_week, 0.0);
    assert_eq!(kpi.week_over_week_change, None);
    assert_eq!(kpi.status_counts.pending, 0);
    assert_eq!(kpi.status_counts.confirmed, 0);
    assert_eq!(kpi.status_counts.fulfilled, 0);
    assert_eq!(kpi.status_counts.cancelled, 0);
    assert_eq!(kpi.active_companies, 0);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn kpi_total_meals_excludes_cancelled(pool: PgPool) {
    let c = seed_company(&pool, "Co").await;
    let r = seed_recipe(&pool, "R").await;
    // Two live orders (3 + 4) + one cancelled (99 should not count).
    seed_order(&pool, c, r, "confirmed", "admin_manual", 3.0, date(2026, 6, 10)).await;
    seed_order(&pool, c, r, "fulfilled", "admin_manual", 4.0, date(2026, 6, 20)).await;
    seed_order(&pool, c, r, "cancelled", "admin_manual", 99.0, date(2026, 6, 15)).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let kpi: KpiMetrics = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/insights/kpi?from=2026-06-01&to=2026-06-30\
             &prior_from=2026-05-01&prior_to=2026-05-31",
        )
        .await,
    )
    .await;
    assert_eq!(kpi.total_meals, 7.0);
    assert_eq!(kpi.status_counts.cancelled, 1);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn kpi_wow_pct_positive_change(pool: PgPool) {
    // Prior window = 10 meals, current = 15 → +50%.
    let c = seed_company(&pool, "Co").await;
    let r = seed_recipe(&pool, "R").await;
    seed_order(&pool, c, r, "confirmed", "admin_manual", 10.0, date(2026, 5, 15)).await;
    seed_order(&pool, c, r, "confirmed", "admin_manual", 15.0, date(2026, 6, 15)).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let kpi: KpiMetrics = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/insights/kpi?from=2026-06-01&to=2026-06-30\
             &prior_from=2026-05-01&prior_to=2026-05-31",
        )
        .await,
    )
    .await;
    assert_eq!(kpi.total_meals, 15.0);
    assert_eq!(kpi.total_meals_prior_week, 10.0);
    assert_eq!(kpi.week_over_week_change, Some(50));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn kpi_wow_pct_null_when_prior_zero(pool: PgPool) {
    // Current has data, prior window is empty → wow should be null
    // (matches TS `weekOverWeekChange: null`).
    let c = seed_company(&pool, "Co").await;
    let r = seed_recipe(&pool, "R").await;
    seed_order(&pool, c, r, "confirmed", "admin_manual", 5.0, date(2026, 6, 10)).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let kpi: KpiMetrics = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/insights/kpi?from=2026-06-01&to=2026-06-30\
             &prior_from=2026-05-01&prior_to=2026-05-31",
        )
        .await,
    )
    .await;
    assert_eq!(kpi.total_meals, 5.0);
    assert_eq!(kpi.total_meals_prior_week, 0.0);
    assert_eq!(kpi.week_over_week_change, None);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn kpi_status_counts_include_cancelled(pool: PgPool) {
    // Status breakdown counts ALL four statuses, including cancelled.
    // (The TS surface does `GROUP BY status` which also includes
    // cancelled; the cancelled filter only affects `total_meals`.)
    let c = seed_company(&pool, "Co").await;
    let r = seed_recipe(&pool, "R").await;
    seed_order(&pool, c, r, "pending", "admin_manual", 1.0, date(2026, 6, 1)).await;
    seed_order(&pool, c, r, "pending", "admin_manual", 1.0, date(2026, 6, 2)).await;
    seed_order(&pool, c, r, "confirmed", "admin_manual", 1.0, date(2026, 6, 3)).await;
    seed_order(&pool, c, r, "fulfilled", "admin_manual", 1.0, date(2026, 6, 4)).await;
    seed_order(&pool, c, r, "cancelled", "admin_manual", 1.0, date(2026, 6, 5)).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let kpi: KpiMetrics = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/insights/kpi?from=2026-06-01&to=2026-06-30\
             &prior_from=2026-05-01&prior_to=2026-05-31",
        )
        .await,
    )
    .await;
    assert_eq!(kpi.status_counts.pending, 2);
    assert_eq!(kpi.status_counts.confirmed, 1);
    assert_eq!(kpi.status_counts.fulfilled, 1);
    assert_eq!(kpi.status_counts.cancelled, 1);
    // total_meals = 2+1+1 = 4 (cancelled excluded)
    assert_eq!(kpi.total_meals, 4.0);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn kpi_active_companies_distinct_and_excludes_all_cancelled(pool: PgPool) {
    // Co A has one live order, Co B has only cancelled orders.
    // active_companies should be 1 (A), not 2.
    let a = seed_company(&pool, "A").await;
    let b = seed_company(&pool, "B").await;
    let r = seed_recipe(&pool, "R").await;
    seed_order(&pool, a, r, "confirmed", "admin_manual", 1.0, date(2026, 6, 10)).await;
    seed_order(&pool, b, r, "cancelled", "admin_manual", 1.0, date(2026, 6, 10)).await;
    seed_order(&pool, b, r, "cancelled", "admin_manual", 1.0, date(2026, 6, 11)).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let kpi: KpiMetrics = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/insights/kpi?from=2026-06-01&to=2026-06-30\
             &prior_from=2026-05-01&prior_to=2026-05-31",
        )
        .await,
    )
    .await;
    assert_eq!(kpi.active_companies, 1);

    server.stop().await;
}

// ===========================================================================
// GET /api/insights/volume
// ===========================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn volume_daily_groups_per_day(pool: PgPool) {
    let c = seed_company(&pool, "Co").await;
    let r = seed_recipe(&pool, "R").await;
    seed_order(&pool, c, r, "confirmed", "admin_manual", 2.0, date(2026, 6, 10)).await;
    seed_order(&pool, c, r, "confirmed", "admin_manual", 3.0, date(2026, 6, 10)).await;
    seed_order(&pool, c, r, "confirmed", "admin_manual", 5.0, date(2026, 6, 11)).await;
    // Cancelled must not count.
    seed_order(&pool, c, r, "cancelled", "admin_manual", 99.0, date(2026, 6, 10)).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<VolumeSeriesRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/insights/volume?from=2026-06-01&to=2026-06-30",
        )
        .await,
    )
    .await;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].date, date(2026, 6, 10));
    assert_eq!(rows[0].total, 5.0);
    assert_eq!(rows[1].date, date(2026, 6, 11));
    assert_eq!(rows[1].total, 5.0);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn volume_weekly_buckets(pool: PgPool) {
    // ISO weeks: 2026-06-08 (Mon) is week start for 06-10. 2026-06-15
    // (Mon) is week start for 06-17. One row per week bucket.
    let c = seed_company(&pool, "Co").await;
    let r = seed_recipe(&pool, "R").await;
    seed_order(&pool, c, r, "confirmed", "admin_manual", 2.0, date(2026, 6, 10)).await;
    seed_order(&pool, c, r, "confirmed", "admin_manual", 3.0, date(2026, 6, 11)).await;
    seed_order(&pool, c, r, "confirmed", "admin_manual", 7.0, date(2026, 6, 17)).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<VolumeSeriesRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/insights/volume?from=2026-06-01&to=2026-06-30&granularity=weekly",
        )
        .await,
    )
    .await;
    assert_eq!(rows.len(), 2);
    // ISO Monday of 06-10 = 06-08
    assert_eq!(rows[0].date, date(2026, 6, 8));
    assert_eq!(rows[0].total, 5.0);
    assert_eq!(rows[1].date, date(2026, 6, 15));
    assert_eq!(rows[1].total, 7.0);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn volume_empty_window_returns_empty(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<VolumeSeriesRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/insights/volume?from=2026-06-01&to=2026-06-30",
        )
        .await,
    )
    .await;
    assert!(rows.is_empty());

    server.stop().await;
}

// ===========================================================================
// GET /api/insights/top-recipes — SUM(SUM()) OVER () window
// ===========================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn top_recipes_ordered_by_amount_desc_and_percent_sums_100(pool: PgPool) {
    // Three recipes, totals 60/30/10 → percent_of_total 60/30/10.
    let c = seed_company(&pool, "Co").await;
    let r1 = seed_recipe(&pool, "Alpha").await;
    let r2 = seed_recipe(&pool, "Beta").await;
    let r3 = seed_recipe(&pool, "Gamma").await;
    seed_order(&pool, c, r1, "confirmed", "admin_manual", 60.0, date(2026, 6, 10)).await;
    seed_order(&pool, c, r2, "confirmed", "admin_manual", 30.0, date(2026, 6, 11)).await;
    seed_order(&pool, c, r3, "confirmed", "admin_manual", 10.0, date(2026, 6, 12)).await;
    // Cancelled must be excluded.
    seed_order(&pool, c, r1, "cancelled", "admin_manual", 999.0, date(2026, 6, 13)).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<TopRecipeRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/insights/top-recipes?from=2026-06-01&to=2026-06-30",
        )
        .await,
    )
    .await;
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].name, "Alpha");
    assert_eq!(rows[0].total_amount, 60.0);
    assert_eq!(rows[0].percent_of_total, 60.0);
    assert_eq!(rows[1].name, "Beta");
    assert_eq!(rows[1].percent_of_total, 30.0);
    assert_eq!(rows[2].name, "Gamma");
    assert_eq!(rows[2].percent_of_total, 10.0);
    // Pct sums to 100% exactly here (no float drift at this scale).
    let pct_sum: f64 = rows.iter().map(|r| r.percent_of_total).sum();
    assert_eq!(pct_sum, 100.0);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn top_recipes_respects_limit(pool: PgPool) {
    // Five recipes → limit=2 returns the top two by total.
    let c = seed_company(&pool, "Co").await;
    for (i, amt) in [50.0, 40.0, 30.0, 20.0, 10.0].into_iter().enumerate() {
        let r = seed_recipe(&pool, &format!("R{i:02}")).await;
        seed_order(&pool, c, r, "confirmed", "admin_manual", amt, date(2026, 6, 10)).await;
    }
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<TopRecipeRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/insights/top-recipes?from=2026-06-01&to=2026-06-30&limit=2",
        )
        .await,
    )
    .await;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].name, "R00");
    assert_eq!(rows[1].name, "R01");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn top_recipes_empty_window_is_empty(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<TopRecipeRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/insights/top-recipes?from=2026-06-01&to=2026-06-30",
        )
        .await,
    )
    .await;
    assert!(rows.is_empty());

    server.stop().await;
}

// ===========================================================================
// GET /api/insights/companies
// ===========================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn companies_ordered_by_total_amount_desc(pool: PgPool) {
    let a = seed_company(&pool, "A").await;
    let b = seed_company(&pool, "B").await;
    let r = seed_recipe(&pool, "R").await;
    seed_order(&pool, a, r, "confirmed", "admin_manual", 10.0, date(2026, 6, 10)).await;
    seed_order(&pool, b, r, "confirmed", "admin_manual", 30.0, date(2026, 6, 10)).await;
    seed_order(&pool, b, r, "cancelled", "admin_manual", 999.0, date(2026, 6, 11)).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<CompanyBreakdownRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/insights/companies?from=2026-06-01&to=2026-06-30",
        )
        .await,
    )
    .await;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].name, "B");
    assert_eq!(rows[0].total_amount, 30.0);
    assert_eq!(rows[0].order_count, 1);
    assert_eq!(rows[1].name, "A");
    assert_eq!(rows[1].total_amount, 10.0);

    server.stop().await;
}

// ===========================================================================
// GET /api/insights/source-mix
// ===========================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn source_mix_groups_by_source(pool: PgPool) {
    let c = seed_company(&pool, "Co").await;
    let r = seed_recipe(&pool, "R").await;
    seed_order(&pool, c, r, "confirmed", "admin_manual", 5.0, date(2026, 6, 1)).await;
    seed_order(&pool, c, r, "confirmed", "admin_manual", 3.0, date(2026, 6, 2)).await;
    seed_order(&pool, c, r, "confirmed", "excel_import", 10.0, date(2026, 6, 3)).await;
    seed_order(&pool, c, r, "cancelled", "client_portal", 99.0, date(2026, 6, 4)).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let mut rows: Vec<SourceMixRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/insights/source-mix?from=2026-06-01&to=2026-06-30",
        )
        .await,
    )
    .await;
    rows.sort_by(|a, b| a.order_source.cmp(&b.order_source));
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].order_source, "admin_manual");
    assert_eq!(rows[0].total_amount, 8.0);
    assert_eq!(rows[0].order_count, 2);
    assert_eq!(rows[1].order_source, "excel_import");
    assert_eq!(rows[1].total_amount, 10.0);

    server.stop().await;
}

// ===========================================================================
// GET /api/insights/ingredients
// ===========================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn ingredient_demand_sums_product_and_includes_allergens(pool: PgPool) {
    // Recipe uses Flour (2 cups) with allergen Gluten.
    // 3 orders × 10 meals = 30 meals; 30 × 2 cups = 60 cups.
    let c = seed_company(&pool, "Co").await;
    let r = seed_recipe(&pool, "Pasta").await;
    let flour = seed_ingredient(&pool, "Flour").await;
    let gluten = seed_allergen(&pool, "Gluten").await;
    attach_recipe_ingredient(&pool, r, flour, 2.0, "cup").await;
    attach_ingredient_allergen(&pool, flour, gluten).await;

    seed_order(&pool, c, r, "pending", "admin_manual", 10.0, date(2026, 6, 10)).await;
    seed_order(&pool, c, r, "confirmed", "admin_manual", 10.0, date(2026, 6, 11)).await;
    seed_order(&pool, c, r, "confirmed", "admin_manual", 10.0, date(2026, 6, 12)).await;
    // 'cancelled' and 'fulfilled' must NOT count — ingredient demand
    // is for future production (pending/confirmed only).
    seed_order(&pool, c, r, "cancelled", "admin_manual", 999.0, date(2026, 6, 13)).await;
    seed_order(&pool, c, r, "fulfilled", "admin_manual", 999.0, date(2026, 6, 14)).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<IngredientDemandRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/insights/ingredients?from=2026-06-01&to=2026-06-30",
        )
        .await,
    )
    .await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "Flour");
    assert_eq!(rows[0].total_amount, 60.0);
    assert_eq!(rows[0].unit, "cup");
    assert_eq!(rows[0].allergens, "Gluten");

    server.stop().await;
}

// ===========================================================================
// GET /api/insights/attendance
// ===========================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn attendance_daily_rate(pool: PgPool) {
    // One day, 2 attending of 4 persons → 50.0% rate.
    let c = seed_company(&pool, "Co").await;
    let p1 = seed_person(&pool, c, "P1").await;
    let p2 = seed_person(&pool, c, "P2").await;
    let p3 = seed_person(&pool, c, "P3").await;
    let p4 = seed_person(&pool, c, "P4").await;
    let d = date(2026, 6, 10);
    seed_attendance(&pool, p1, d, true).await;
    seed_attendance(&pool, p2, d, true).await;
    seed_attendance(&pool, p3, d, false).await;
    seed_attendance(&pool, p4, d, false).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<AttendanceDayRateRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/insights/attendance?from=2026-06-01&to=2026-06-30",
        )
        .await,
    )
    .await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].date, d);
    assert_eq!(rows[0].attending, 2);
    assert_eq!(rows[0].total, 4);
    assert!((rows[0].rate - 50.0).abs() < 0.05);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn attendance_excludes_inactive_persons(pool: PgPool) {
    // 1 active (attending), 1 inactive (attending). Rate should be
    // 100% over 1 person — not 100% over 2 — because inactive is
    // filtered.
    let c = seed_company(&pool, "Co").await;
    let active = seed_person(&pool, c, "Active").await;
    let inactive = seed_person_inactive(&pool, c, "Inactive").await;
    let d = date(2026, 6, 10);
    seed_attendance(&pool, active, d, true).await;
    seed_attendance(&pool, inactive, d, true).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<AttendanceDayRateRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/insights/attendance?from=2026-06-01&to=2026-06-30",
        )
        .await,
    )
    .await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].total, 1);
    assert_eq!(rows[0].attending, 1);
    assert!((rows[0].rate - 100.0).abs() < 0.05);

    server.stop().await;
}

// ===========================================================================
// GET /api/insights/attendance-by-company
// ===========================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn attendance_by_company_ordered_by_rate_desc(pool: PgPool) {
    // Co A: 1/1 attending = 100%. Co B: 1/2 = 50%. A ranks first.
    let a = seed_company(&pool, "A").await;
    let b = seed_company(&pool, "B").await;
    let a1 = seed_person(&pool, a, "A1").await;
    let b1 = seed_person(&pool, b, "B1").await;
    let b2 = seed_person(&pool, b, "B2").await;
    let d = date(2026, 6, 10);
    seed_attendance(&pool, a1, d, true).await;
    seed_attendance(&pool, b1, d, true).await;
    seed_attendance(&pool, b2, d, false).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<AttendanceByCompanyRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/insights/attendance-by-company?from=2026-06-01&to=2026-06-30",
        )
        .await,
    )
    .await;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].company_name, "A");
    assert!((rows[0].rate - 100.0).abs() < 0.05);
    assert_eq!(rows[1].company_name, "B");
    assert!((rows[1].rate - 50.0).abs() < 0.05);

    server.stop().await;
}

// ===========================================================================
// Client scoping — all 8 endpoints must 403 for client callers.
// ===========================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_read_kpi(pool: PgPool) {
    let c = seed_company(&pool, "Co").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        c,
        "/api/insights/kpi?from=2026-06-01&to=2026-06-30\
         &prior_from=2026-05-01&prior_to=2026-05-31",
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_read_volume(pool: PgPool) {
    let c = seed_company(&pool, "Co").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        c,
        "/api/insights/volume?from=2026-06-01&to=2026-06-30",
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_read_top_recipes(pool: PgPool) {
    let c = seed_company(&pool, "Co").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        c,
        "/api/insights/top-recipes?from=2026-06-01&to=2026-06-30",
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_read_companies(pool: PgPool) {
    let c = seed_company(&pool, "Co").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        c,
        "/api/insights/companies?from=2026-06-01&to=2026-06-30",
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_read_source_mix(pool: PgPool) {
    let c = seed_company(&pool, "Co").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        c,
        "/api/insights/source-mix?from=2026-06-01&to=2026-06-30",
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_read_ingredients(pool: PgPool) {
    let c = seed_company(&pool, "Co").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        c,
        "/api/insights/ingredients?from=2026-06-01&to=2026-06-30",
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_read_attendance(pool: PgPool) {
    let c = seed_company(&pool, "Co").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        c,
        "/api/insights/attendance?from=2026-06-01&to=2026-06-30",
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_read_attendance_by_company(pool: PgPool) {
    let c = seed_company(&pool, "Co").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        c,
        "/api/insights/attendance-by-company?from=2026-06-01&to=2026-06-30",
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

// ===========================================================================
// Validation — malformed date → 400 (serde-driven, smoke check only).
// ===========================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn malformed_date_returns_400(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(
        &server,
        &user.id,
        "/api/insights/volume?from=not-a-date&to=2026-06-30",
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    server.stop().await;
}
