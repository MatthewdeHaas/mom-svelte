//! Parity integration tests for `/api/financials/*`.
//!
//! The TS surface has no existing test suite — this port adds one on
//! both sides. The Rust suite covers the HTTP surface end-to-end, with
//! the client-scoping tests (403 on every endpoint) the TS suite can't
//! express.
//!
//! Shape:
//! - 3 endpoints, admin-only across the board.
//! - Numeric columns (`price_per_meal`, `cost_per_unit`, `amount`) are
//!   NUMERIC in Postgres; the handler casts to `float8` and we assert
//!   on `f64`. Tolerance is exact on seeded inputs (no float drift at
//!   this scale: single-digit integers × 2-decimal prices).

mod common;

use chrono::{NaiveDate, Utc};
use common::{companies::seed_company, seed_user, signed_headers, spawn_server, TestServer};
use db::queries::{
    financials::{InvoiceSummary, RecipeMarginRow},
    pagination::Paginated,
};
use reqwest::{Client, StatusCode};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Tiny HTTP helpers — same shape as the invites test file.
// ---------------------------------------------------------------------------

async fn get_admin(server: &TestServer, user_id: &str, path: &str) -> reqwest::Response {
    let mut req = Client::new().get(format!("{}{}", server.base_url, path));
    for (k, v) in signed_headers(user_id, "admin", "") {
        req = req.header(k, v);
    }
    req.send().await.expect("GET")
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

const BOGUS: &str = "00000000-0000-0000-0000-000000000000";

// ---------------------------------------------------------------------------
// Seed helpers — insert recipes / ingredients / orders directly so tests
// don't depend on the create endpoints of other domains.
// ---------------------------------------------------------------------------

/// Seed a recipe with an optional price_per_meal. Returns id.
async fn seed_recipe(pool: &PgPool, name: &str, price: Option<f64>) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO recipe (name, active, price_per_meal) \
         VALUES ($1, TRUE, $2) RETURNING id",
    )
    .bind(name)
    .bind(price)
    .fetch_one(pool)
    .await
    .expect("seed recipe")
}

/// Seed an inactive recipe. Used to assert active-only filtering on the
/// margin surface.
async fn seed_recipe_inactive(pool: &PgPool, name: &str, price: Option<f64>) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO recipe (name, active, price_per_meal) \
         VALUES ($1, FALSE, $2) RETURNING id",
    )
    .bind(name)
    .bind(price)
    .fetch_one(pool)
    .await
    .expect("seed inactive recipe")
}

/// Seed an ingredient with a cost_per_unit. Returns id.
async fn seed_ingredient(pool: &PgPool, name: &str, cost: Option<f64>) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO ingredient (name, active, cost_per_unit) \
         VALUES ($1, TRUE, $2) RETURNING id",
    )
    .bind(name)
    .bind(cost)
    .fetch_one(pool)
    .await
    .expect("seed ingredient")
}

/// Attach an ingredient to a recipe with (amount, unit).
async fn attach_ingredient(
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
    .expect("attach ingredient");
}

/// Seed one order against (company, recipe) with (amount, status, serving_date).
/// `created_by` is set to NULL to avoid the user FK — financial reads
/// don't care who placed the order.
async fn seed_order(
    pool: &PgPool,
    company_id: Uuid,
    recipe_id: Uuid,
    status: &str,
    amount: f64,
    serving_date: NaiveDate,
) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        r#"INSERT INTO "order"
           (company_id, recipe_id, order_source, status, meal_category,
            amount, serving_date, shipment_date, frozen, chilled, recurring)
           VALUES ($1, $2, 'admin_manual', $3, 'Lunch',
                   $4, $5, $5, FALSE, FALSE, FALSE)
           RETURNING id"#,
    )
    .bind(company_id)
    .bind(recipe_id)
    .bind(status)
    .bind(amount)
    .bind(serving_date)
    .fetch_one(pool)
    .await
    .expect("seed order")
}

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("valid date")
}

// ===========================================================================
// GET /api/financials/recipe-margins
// ===========================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn margins_empty_when_no_recipes(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<RecipeMarginRow> =
        decode_ok(get_admin(&server, &user.id, "/api/financials/recipe-margins").await).await;
    assert!(rows.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn margins_recipe_with_no_price_has_null_margin(pool: PgPool) {
    seed_recipe(&pool, "NoPriceRecipe", None).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<RecipeMarginRow> =
        decode_ok(get_admin(&server, &user.id, "/api/financials/recipe-margins").await).await;
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!(r.name, "NoPriceRecipe");
    assert_eq!(r.price_per_meal, None);
    assert_eq!(r.ingredient_cost, None);
    assert_eq!(r.margin, None);
    assert_eq!(r.margin_pct, None);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn margins_recipe_with_no_ingredients_has_null_cost(pool: PgPool) {
    seed_recipe(&pool, "Priced", Some(12.50)).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<RecipeMarginRow> =
        decode_ok(get_admin(&server, &user.id, "/api/financials/recipe-margins").await).await;
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!(r.price_per_meal, Some(12.50));
    assert_eq!(r.ingredient_cost, None);
    // Price present but cost null → margin null (parity with TS).
    assert_eq!(r.margin, None);
    assert_eq!(r.margin_pct, None);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn margins_computes_margin_and_pct(pool: PgPool) {
    // Price 10.00; two ingredients at (2 units × 1.50) + (3 units × 1.00) = 6.00
    // Margin = 4.00; pct = (10-6)/10 * 100 = 40.0
    let r = seed_recipe(&pool, "BigMargin", Some(10.00)).await;
    let a = seed_ingredient(&pool, "Flour", Some(1.5000)).await;
    let b = seed_ingredient(&pool, "Oil", Some(1.0000)).await;
    attach_ingredient(&pool, r, a, 2.0, "cup").await;
    attach_ingredient(&pool, r, b, 3.0, "tbsp").await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<RecipeMarginRow> =
        decode_ok(get_admin(&server, &user.id, "/api/financials/recipe-margins").await).await;
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.price_per_meal, Some(10.00));
    assert_eq!(row.ingredient_cost, Some(6.00));
    assert_eq!(row.margin, Some(4.00));
    assert_eq!(row.margin_pct, Some(40.0));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn margins_excludes_inactive_recipes(pool: PgPool) {
    seed_recipe(&pool, "Active", Some(5.00)).await;
    seed_recipe_inactive(&pool, "Inactive", Some(99.00)).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<RecipeMarginRow> =
        decode_ok(get_admin(&server, &user.id, "/api/financials/recipe-margins").await).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "Active");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn margins_ordered_by_name_ascending(pool: PgPool) {
    seed_recipe(&pool, "Zeta", None).await;
    seed_recipe(&pool, "Alpha", None).await;
    seed_recipe(&pool, "Mu", None).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<RecipeMarginRow> =
        decode_ok(get_admin(&server, &user.id, "/api/financials/recipe-margins").await).await;
    let names: Vec<_> = rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, vec!["Alpha", "Mu", "Zeta"]);

    server.stop().await;
}

// ===========================================================================
// GET /api/financials/recipe-margins/paginated
// ===========================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn margins_paginated_defaults_to_page_1(pool: PgPool) {
    for i in 0..3 {
        seed_recipe(&pool, &format!("R{i:02}"), Some(1.0)).await;
    }
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let page: Paginated<RecipeMarginRow> = decode_ok(
        get_admin(&server, &user.id, "/api/financials/recipe-margins/paginated").await,
    )
    .await;
    assert_eq!(page.total, 3);
    assert_eq!(page.page, 1);
    assert_eq!(page.per_page, 25);
    assert_eq!(page.total_pages, 1);
    assert_eq!(page.data.len(), 3);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn margins_paginated_respects_page_and_per_page(pool: PgPool) {
    // 15 recipes, per_page=10 → page 1 = 10 rows, page 2 = 5 rows.
    for i in 0..15 {
        seed_recipe(&pool, &format!("R{i:02}"), Some(1.0)).await;
    }
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let p1: Paginated<RecipeMarginRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/financials/recipe-margins/paginated?page=1&per_page=10",
        )
        .await,
    )
    .await;
    assert_eq!(p1.page, 1);
    assert_eq!(p1.per_page, 10);
    assert_eq!(p1.total, 15);
    assert_eq!(p1.total_pages, 2);
    assert_eq!(p1.data.len(), 10);

    let p2: Paginated<RecipeMarginRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/financials/recipe-margins/paginated?page=2&per_page=10",
        )
        .await,
    )
    .await;
    assert_eq!(p2.page, 2);
    assert_eq!(p2.data.len(), 5);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn margins_paginated_count_ignores_join(pool: PgPool) {
    // One recipe with 5 ingredients — total must be 1, not 5 (count
    // query is `COUNT(*) FROM recipe`, not through the join).
    let r = seed_recipe(&pool, "Multi", Some(10.0)).await;
    for i in 0..5 {
        let ing = seed_ingredient(&pool, &format!("I{i}"), Some(0.50)).await;
        attach_ingredient(&pool, r, ing, 1.0, "g").await;
    }
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let page: Paginated<RecipeMarginRow> = decode_ok(
        get_admin(&server, &user.id, "/api/financials/recipe-margins/paginated").await,
    )
    .await;
    assert_eq!(page.total, 1);
    assert_eq!(page.data.len(), 1);
    assert_eq!(page.data[0].ingredient_cost, Some(2.50));

    server.stop().await;
}

// ===========================================================================
// GET /api/financials/invoice
// ===========================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn invoice_404_when_company_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(
        &server,
        &user.id,
        &format!("/api/financials/invoice?company_id={BOGUS}&from=2026-01-01&to=2026-12-31"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn invoice_empty_line_items_when_no_matching_orders(pool: PgPool) {
    let c = seed_company(&pool, "EmptyInvCo").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let inv: InvoiceSummary = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/financials/invoice?company_id={c}&from=2026-01-01&to=2026-12-31"),
        )
        .await,
    )
    .await;
    assert_eq!(inv.company_id, c);
    assert_eq!(inv.company_name, "EmptyInvCo");
    assert_eq!(inv.subtotal, 0.0);
    assert!(inv.line_items.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn invoice_aggregates_confirmed_and_fulfilled_orders(pool: PgPool) {
    // Three orders: two in range (confirmed + fulfilled), one out-of-range,
    // one in range but 'pending' (excluded).
    let c = seed_company(&pool, "AggCo").await;
    let r1 = seed_recipe(&pool, "RecipeA", Some(10.00)).await;
    let r2 = seed_recipe(&pool, "RecipeB", Some(5.00)).await;

    // Date window: June 2026
    seed_order(&pool, c, r1, "confirmed", 3.0, date(2026, 6, 10)).await;
    seed_order(&pool, c, r2, "fulfilled", 4.0, date(2026, 6, 20)).await;
    // Excluded: outside window
    seed_order(&pool, c, r1, "fulfilled", 10.0, date(2026, 7, 1)).await;
    // Excluded: wrong status
    seed_order(&pool, c, r1, "pending", 10.0, date(2026, 6, 15)).await;
    // Excluded: wrong status (cancelled)
    seed_order(&pool, c, r2, "cancelled", 10.0, date(2026, 6, 15)).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let inv: InvoiceSummary = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/financials/invoice?company_id={c}&from=2026-06-01&to=2026-06-30"),
        )
        .await,
    )
    .await;
    assert_eq!(inv.line_items.len(), 2);
    // Subtotal: (3 × 10) + (4 × 5) = 50.00
    assert_eq!(inv.subtotal, 50.00);
    // Ordered by serving_date ASC, recipe name ASC — RecipeA (Jun 10) first.
    assert_eq!(inv.line_items[0].recipe_name, "RecipeA");
    assert_eq!(inv.line_items[0].amount, 3.0);
    assert_eq!(inv.line_items[0].unit_price, 10.00);
    assert_eq!(inv.line_items[0].line_total, 30.00);
    assert_eq!(inv.line_items[1].recipe_name, "RecipeB");
    assert_eq!(inv.line_items[1].line_total, 20.00);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn invoice_excludes_recipes_without_price(pool: PgPool) {
    // Order against an unpriced recipe is dropped — matches TS
    // `r.price_per_meal IS NOT NULL` filter.
    let c = seed_company(&pool, "UnpricedCo").await;
    let priced = seed_recipe(&pool, "Priced", Some(5.00)).await;
    let unpriced = seed_recipe(&pool, "Unpriced", None).await;

    seed_order(&pool, c, priced, "confirmed", 2.0, date(2026, 6, 10)).await;
    seed_order(&pool, c, unpriced, "confirmed", 2.0, date(2026, 6, 11)).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let inv: InvoiceSummary = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/financials/invoice?company_id={c}&from=2026-06-01&to=2026-06-30"),
        )
        .await,
    )
    .await;
    assert_eq!(inv.line_items.len(), 1);
    assert_eq!(inv.line_items[0].recipe_name, "Priced");
    assert_eq!(inv.subtotal, 10.00);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn invoice_rejects_invalid_date(pool: PgPool) {
    let c = seed_company(&pool, "DateCo").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(
        &server,
        &user.id,
        &format!("/api/financials/invoice?company_id={c}&from=not-a-date&to=2026-12-31"),
    )
    .await;
    // Query extractor rejects malformed date with 400.
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn invoice_shipping_address_is_preserved(pool: PgPool) {
    // Full-shape company — assert shipping_address makes it to the wire.
    let c = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO company (name, shipping_address, active) \
         VALUES ($1, $2, TRUE) RETURNING id",
    )
    .bind("ShipCo")
    .bind("123 Delivery Lane")
    .fetch_one(&pool)
    .await
    .expect("seed shipping company");

    let user = seed_user(&pool).await;
    let server = spawn_server(pool.clone()).await;

    let inv: InvoiceSummary = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/financials/invoice?company_id={c}&from=2026-01-01&to=2026-12-31"),
        )
        .await,
    )
    .await;
    assert_eq!(inv.shipping_address.as_deref(), Some("123 Delivery Lane"));

    server.stop().await;
}

// ===========================================================================
// Client scoping — admin-only across the board. 403 uniformly.
// ===========================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_read_margins(pool: PgPool) {
    let c = seed_company(&pool, "ScopeMarginCo").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(&server, &user.id, c, "/api/financials/recipe-margins").await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_read_margins_paginated(pool: PgPool) {
    let c = seed_company(&pool, "ScopePagCo").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        c,
        "/api/financials/recipe-margins/paginated",
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_read_invoice_even_for_own_company(pool: PgPool) {
    // Admin-only — the client can't see their own invoice through this
    // endpoint. (The product requirement would be a separate
    // `/api/client/invoice/me` endpoint; not in scope for this port.)
    let c = seed_company(&pool, "ScopeInvCo").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        c,
        &format!("/api/financials/invoice?company_id={c}&from=2026-01-01&to=2026-12-31"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

// Suppress unused-import warning: `Utc` is pulled in for completeness
// alongside NaiveDate but no test currently constructs a `DateTime<Utc>`.
#[allow(dead_code)]
fn _assert_chrono() {
    let _ = Utc::now();
}
