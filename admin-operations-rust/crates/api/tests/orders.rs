//! Parity integration tests for `/api/orders/*`.
//!
//! Mirrors `tests/integration/core/orders.test.ts` 1:1 — same seed
//! shapes, same assertions, same edge cases. Transport is real HTTP
//! via `reqwest` against the live router mounted on an ephemeral
//! `TcpListener` (same pattern as `import_real_client.rs`), so the
//! HMAC middleware is exercised on every call.
//!
//! Each test uses `#[sqlx::test(migrations = "../../migrations")]` so
//! every case gets a pristine DB — tests are embarrassingly parallel.

mod common;

use chrono::NaiveDate;
use common::{
    orders::{default_insert, seed_company, seed_order, seed_recipe, BASE_SERVING, BASE_SHIPMENT},
    seed_user, signed_headers, spawn_server, TestServer,
};
use db::queries::{
    orders::{
        self, InsertOrderData, OrderDetailRow, OrderRow, OrderSummaryRow, OrderStatusCounts,
        UpdateOrderData,
    },
    pagination::Paginated,
};
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Tiny HTTP helpers — keep the per-test boilerplate down to one line.
// ---------------------------------------------------------------------------

/// Issue a GET against the live router with admin HMAC headers.
async fn get_admin(server: &TestServer, user_id: &str, path: &str) -> reqwest::Response {
    let mut req = Client::new().get(format!("{}{}", server.base_url, path));
    for (k, v) in signed_headers(user_id, "admin", "") {
        req = req.header(k, v);
    }
    req.send().await.expect("GET")
}

/// Issue a GET with client HMAC headers (role=client, scoped to `company_id`).
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

/// Issue a POST with a JSON body as admin.
async fn post_admin(
    server: &TestServer,
    user_id: &str,
    path: &str,
    body: &Value,
) -> reqwest::Response {
    let mut req = Client::new()
        .post(format!("{}{}", server.base_url, path))
        .json(body);
    for (k, v) in signed_headers(user_id, "admin", "") {
        req = req.header(k, v);
    }
    req.send().await.expect("POST")
}

/// Issue a POST with no body as admin — status-transition endpoints.
async fn post_admin_empty(
    server: &TestServer,
    user_id: &str,
    path: &str,
) -> reqwest::Response {
    let mut req = Client::new().post(format!("{}{}", server.base_url, path));
    for (k, v) in signed_headers(user_id, "admin", "") {
        req = req.header(k, v);
    }
    req.send().await.expect("POST empty")
}

/// PATCH with JSON as admin.
async fn patch_admin(
    server: &TestServer,
    user_id: &str,
    path: &str,
    body: &Value,
) -> reqwest::Response {
    let mut req = Client::new()
        .patch(format!("{}{}", server.base_url, path))
        .json(body);
    for (k, v) in signed_headers(user_id, "admin", "") {
        req = req.header(k, v);
    }
    req.send().await.expect("PATCH")
}

/// Decode the body as `T`, panicking with context on failure. Also
/// asserts the response was 2xx — anything else is a test-harness bug.
async fn decode_ok<T: serde::de::DeserializeOwned>(res: reqwest::Response) -> T {
    let status = res.status();
    let text = res.text().await.expect("body");
    assert!(status.is_success(), "non-2xx: status={status} body={text}");
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("decode: {e}; body={text}"))
}

// ---------------------------------------------------------------------------
// list — GET /api/orders
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn list_returns_empty_when_no_orders(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(&server, &user.id, "/api/orders").await;
    let rows: Vec<OrderSummaryRow> = decode_ok(res).await;
    assert!(rows.is_empty(), "expected empty, got {rows:?}");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_returns_all_orders_without_filters(pool: PgPool) {
    let o1 = seed_order(&pool, "co-list-1").await;
    let o2 = seed_order(&pool, "co-list-2").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(&server, &user.id, "/api/orders").await;
    let rows: Vec<OrderSummaryRow> = decode_ok(res).await;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();

    assert!(ids.contains(&o1.id));
    assert!(ids.contains(&o2.id));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_includes_company_name(pool: PgPool) {
    seed_order(&pool, "co-name-check").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<OrderSummaryRow> =
        decode_ok(get_admin(&server, &user.id, "/api/orders").await).await;

    assert!(rows.iter().all(|r| !r.company_name.is_empty()));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_filters_by_company_id(pool: PgPool) {
    let c1 = seed_company(&pool, "filter-co-1").await;
    let c2 = seed_company(&pool, "filter-co-2").await;

    let mut d1 = default_insert(c1);
    d1.amount = 5.0;
    let o1 = orders::create(&pool, d1).await.expect("create o1");
    let mut d2 = default_insert(c2);
    d2.amount = 5.0;
    orders::create(&pool, d2).await.expect("create o2");

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<OrderSummaryRow> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/orders?company_id={c1}")).await,
    )
    .await;

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, o1.id);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_filters_by_status(pool: PgPool) {
    let pending = seed_order(&pool, "status-pending").await;
    let confirmed = seed_order(&pool, "status-confirmed").await;
    orders::confirm(&pool, confirmed.id)
        .await
        .expect("confirm")
        .expect("row");

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<OrderSummaryRow> =
        decode_ok(get_admin(&server, &user.id, "/api/orders?status=pending").await).await;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();

    assert!(ids.contains(&pending.id));
    assert!(!ids.contains(&confirmed.id));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_filters_by_recipe_id(pool: PgPool) {
    let recipe = seed_recipe(&pool, "filter-recipe").await;
    let company = seed_company(&pool, "recipe-co").await;

    let mut with_r = default_insert(company);
    with_r.amount = 5.0;
    with_r.recipe_id = Some(recipe);
    let with_recipe = orders::create(&pool, with_r).await.expect("create");
    let without_recipe = seed_order(&pool, "recipe-co-other").await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<OrderSummaryRow> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/orders?recipe_id={recipe}")).await,
    )
    .await;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();

    assert!(ids.contains(&with_recipe.id));
    assert!(!ids.contains(&without_recipe.id));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_filters_by_serving_date_range(pool: PgPool) {
    let cid = seed_company(&pool, "date-range-co").await;

    let mut early = default_insert(cid);
    early.amount = 5.0;
    early.serving_date = NaiveDate::from_ymd_opt(2025, 1, 10).unwrap();
    early.shipment_date = NaiveDate::from_ymd_opt(2025, 1, 5).unwrap();
    let early = orders::create(&pool, early).await.expect("early");

    let mut late = default_insert(cid);
    late.amount = 5.0;
    late.serving_date = NaiveDate::from_ymd_opt(2025, 12, 20).unwrap();
    late.shipment_date = NaiveDate::from_ymd_opt(2025, 12, 15).unwrap();
    let late = orders::create(&pool, late).await.expect("late");

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<OrderSummaryRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/orders?serving_date_from=2025-06-01&serving_date_to=2025-06-30",
        )
        .await,
    )
    .await;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();

    assert!(!ids.contains(&early.id));
    assert!(!ids.contains(&late.id));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_filters_by_frozen(pool: PgPool) {
    let cid = seed_company(&pool, "frozen-co").await;

    let mut frozen = default_insert(cid);
    frozen.amount = 5.0;
    frozen.frozen = Some(true);
    let frozen = orders::create(&pool, frozen).await.expect("frozen");

    let mut not_frozen = default_insert(cid);
    not_frozen.amount = 5.0;
    not_frozen.frozen = Some(false);
    let not_frozen = orders::create(&pool, not_frozen).await.expect("not frozen");

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<OrderSummaryRow> =
        decode_ok(get_admin(&server, &user.id, "/api/orders?frozen=true").await).await;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();

    assert!(ids.contains(&frozen.id));
    assert!(!ids.contains(&not_frozen.id));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_orders_by_serving_date_desc_then_company_asc(pool: PgPool) {
    let c1 = seed_company(&pool, "AAA Company").await;
    let c2 = seed_company(&pool, "ZZZ Company").await;

    let mut d1 = default_insert(c2);
    d1.amount = 5.0;
    orders::create(&pool, d1).await.expect("zzz");
    let mut d2 = default_insert(c1);
    d2.amount = 5.0;
    orders::create(&pool, d2).await.expect("aaa");

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<OrderSummaryRow> =
        decode_ok(get_admin(&server, &user.id, "/api/orders").await).await;
    let names: Vec<&str> = rows.iter().map(|r| r.company_name.as_str()).collect();
    let aaa = names.iter().position(|n| *n == "AAA Company").expect("aaa");
    let zzz = names.iter().position(|n| *n == "ZZZ Company").expect("zzz");

    assert!(aaa < zzz, "AAA should sort before ZZZ at same serving_date");

    server.stop().await;
}

// ---------------------------------------------------------------------------
// list_paginated — GET /api/orders/paginated
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn paginated_clamps_page_below_one(pool: PgPool) {
    seed_order(&pool, "pg-clamp").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let page: Paginated<OrderSummaryRow> = decode_ok(
        get_admin(&server, &user.id, "/api/orders/paginated?page=0&per_page=10").await,
    )
    .await;

    assert_eq!(page.page, 1, "page=0 clamps to 1");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn paginated_clamps_per_page_bounds(pool: PgPool) {
    seed_order(&pool, "pg-bounds").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let low: Paginated<OrderSummaryRow> = decode_ok(
        get_admin(&server, &user.id, "/api/orders/paginated?per_page=1").await,
    )
    .await;
    assert_eq!(low.per_page, 10, "per_page<10 clamps up to 10");

    let high: Paginated<OrderSummaryRow> = decode_ok(
        get_admin(&server, &user.id, "/api/orders/paginated?per_page=5000").await,
    )
    .await;
    assert_eq!(high.per_page, 100, "per_page>100 clamps down to 100");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn paginated_reports_total_and_total_pages(pool: PgPool) {
    // Seed 25 orders — with per_page=10 that's 3 pages, total 25.
    let cid = seed_company(&pool, "pg-math-co").await;
    for _ in 0..25 {
        let mut d = default_insert(cid);
        d.amount = 5.0;
        orders::create(&pool, d).await.expect("seed");
    }
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let page: Paginated<OrderSummaryRow> = decode_ok(
        get_admin(&server, &user.id, "/api/orders/paginated?page=1&per_page=10").await,
    )
    .await;

    assert_eq!(page.total, 25);
    assert_eq!(page.total_pages, 3);
    assert_eq!(page.data.len(), 10);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// get_by_id — GET /api/orders/:id
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_returns_404_for_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(
        &server,
        &user.id,
        "/api/orders/00000000-0000-0000-0000-000000000000",
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_returns_embedded_company(pool: PgPool) {
    let cid = seed_company(&pool, "detail-co").await;
    let mut d = default_insert(cid);
    d.amount = 5.0;
    let order = orders::create(&pool, d).await.expect("create");

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let detail: OrderDetailRow =
        decode_ok(get_admin(&server, &user.id, &format!("/api/orders/{}", order.id)).await).await;

    assert_eq!(detail.id, order.id);
    assert_eq!(detail.company.id, cid);
    assert_eq!(detail.company.name, "detail-co");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_returns_null_recipe_when_unset(pool: PgPool) {
    let order = seed_order(&pool, "no-recipe").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let detail: OrderDetailRow =
        decode_ok(get_admin(&server, &user.id, &format!("/api/orders/{}", order.id)).await).await;

    assert!(detail.recipe.is_none());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_returns_embedded_recipe_when_set(pool: PgPool) {
    let cid = seed_company(&pool, "with-recipe-co").await;
    let rid = seed_recipe(&pool, "detail-recipe").await;
    let mut d = default_insert(cid);
    d.amount = 5.0;
    d.recipe_id = Some(rid);
    let order = orders::create(&pool, d).await.expect("create");

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let detail: OrderDetailRow =
        decode_ok(get_admin(&server, &user.id, &format!("/api/orders/{}", order.id)).await).await;

    let r = detail.recipe.expect("recipe present");
    assert_eq!(r.id, rid);
    assert_eq!(r.name, "detail-recipe");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_returns_amount_as_number(pool: PgPool) {
    let cid = seed_company(&pool, "amount-co").await;
    let mut d = default_insert(cid);
    d.amount = 42.5;
    let order = orders::create(&pool, d).await.expect("create");

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    // Hit the raw JSON so we can see what the wire shape actually is —
    // this is the parity-with-TS smoke test ("typeof amount === number").
    let res = get_admin(&server, &user.id, &format!("/api/orders/{}", order.id)).await;
    let body: Value = res.json().await.expect("json");

    assert!(body["amount"].is_number(), "amount serializes as number");
    assert!((body["amount"].as_f64().unwrap() - 42.5).abs() < 1e-9);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// counts_by_status — GET /api/orders/counts
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn counts_returns_zero_for_all_when_empty(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let counts: OrderStatusCounts =
        decode_ok(get_admin(&server, &user.id, "/api/orders/counts").await).await;

    assert_eq!(counts.pending, 0);
    assert_eq!(counts.confirmed, 0);
    assert_eq!(counts.fulfilled, 0);
    assert_eq!(counts.cancelled, 0);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn counts_buckets_each_status(pool: PgPool) {
    let o1 = seed_order(&pool, "cnt-1").await;
    let o2 = seed_order(&pool, "cnt-2").await;
    let o3 = seed_order(&pool, "cnt-3").await;
    let _ = o1; // stays pending
    orders::confirm(&pool, o2.id).await.unwrap().unwrap();
    orders::confirm(&pool, o3.id).await.unwrap().unwrap();
    orders::fulfill(&pool, o3.id).await.unwrap().unwrap();

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let counts: OrderStatusCounts =
        decode_ok(get_admin(&server, &user.id, "/api/orders/counts").await).await;

    assert_eq!(counts.pending, 1);
    assert_eq!(counts.confirmed, 1);
    assert_eq!(counts.fulfilled, 1);
    assert_eq!(counts.cancelled, 0);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn counts_filters_by_serving_date(pool: PgPool) {
    let cid = seed_company(&pool, "cnt-range-co").await;

    let mut early = default_insert(cid);
    early.amount = 5.0;
    early.serving_date = NaiveDate::from_ymd_opt(2025, 1, 10).unwrap();
    early.shipment_date = NaiveDate::from_ymd_opt(2025, 1, 5).unwrap();
    orders::create(&pool, early).await.expect("early");

    let mut mid = default_insert(cid);
    mid.amount = 5.0;
    orders::create(&pool, mid).await.expect("mid");

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let counts: OrderStatusCounts = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/orders/counts?serving_date_from=2025-06-01&serving_date_to=2025-06-30",
        )
        .await,
    )
    .await;

    assert_eq!(counts.pending, 1, "only the mid-June row falls in range");

    server.stop().await;
}

// ---------------------------------------------------------------------------
// create — POST /api/orders
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn create_defaults_to_pending(pool: PgPool) {
    let cid = seed_company(&pool, "create-default-co").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let body = json!({
        "company_id": cid,
        "order_source": "admin_manual",
        "amount": 10.0,
        "serving_date": BASE_SERVING,
        "shipment_date": BASE_SHIPMENT,
    });
    let res = post_admin(&server, &user.id, "/api/orders", &body).await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let order: OrderRow = decode_ok(res).await;

    assert_eq!(order.status, "pending");
    assert!(!order.frozen);
    assert!(!order.chilled);
    assert!(!order.recurring);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_respects_explicit_status(pool: PgPool) {
    let cid = seed_company(&pool, "create-status-co").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let body = json!({
        "company_id": cid,
        "order_source": "admin_manual",
        "status": "confirmed",
        "amount": 10.0,
        "serving_date": BASE_SERVING,
        "shipment_date": BASE_SHIPMENT,
    });
    let order: OrderRow = decode_ok(post_admin(&server, &user.id, "/api/orders", &body).await).await;

    assert_eq!(order.status, "confirmed");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_rejects_bad_shipment_date_as_400(pool: PgPool) {
    let cid = seed_company(&pool, "create-badship-co").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let body = json!({
        "company_id": cid,
        "order_source": "admin_manual",
        "amount": 10.0,
        "serving_date": "2025-06-10",
        "shipment_date": "2025-06-15", // after serving_date — violates CHECK
    });
    let res = post_admin(&server, &user.id, "/api/orders", &body).await;

    assert_eq!(
        res.status(),
        StatusCode::BAD_REQUEST,
        "CHECK violation surfaces as 400"
    );

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_allows_shipment_date_equal_to_serving_date(pool: PgPool) {
    let cid = seed_company(&pool, "create-sameday-co").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let body = json!({
        "company_id": cid,
        "order_source": "admin_manual",
        "amount": 10.0,
        "serving_date": "2025-06-15",
        "shipment_date": "2025-06-15",
    });
    let res = post_admin(&server, &user.id, "/api/orders", &body).await;
    assert_eq!(res.status(), StatusCode::CREATED);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_rejects_bad_company_fk_as_400(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let body = json!({
        "company_id": "00000000-0000-0000-0000-000000000000",
        "order_source": "admin_manual",
        "amount": 10.0,
        "serving_date": BASE_SERVING,
        "shipment_date": BASE_SHIPMENT,
    });
    let res = post_admin(&server, &user.id, "/api/orders", &body).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_rejects_invalid_order_source_as_400(pool: PgPool) {
    let cid = seed_company(&pool, "bad-src-co").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let body = json!({
        "company_id": cid,
        "order_source": "not_a_real_source",
        "amount": 10.0,
        "serving_date": BASE_SERVING,
        "shipment_date": BASE_SHIPMENT,
    });
    let res = post_admin(&server, &user.id, "/api/orders", &body).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// update — PATCH /api/orders/:id
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn update_returns_404_for_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let body = json!({ "amount": 5.0 });
    let res = patch_admin(
        &server,
        &user.id,
        "/api/orders/00000000-0000-0000-0000-000000000000",
        &body,
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_patches_amount_only(pool: PgPool) {
    let cid = seed_company(&pool, "update-amount-co").await;
    let mut d = default_insert(cid);
    d.frozen = Some(true);
    let order = orders::create(&pool, d).await.expect("create");

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let body = json!({ "amount": 99.0 });
    let updated: OrderRow =
        decode_ok(patch_admin(&server, &user.id, &format!("/api/orders/{}", order.id), &body).await)
            .await;

    assert!(updated.frozen, "unrelated field unchanged (COALESCE)");
    assert!((updated.amount - 99.0).abs() < 1e-9);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_patches_multiple_fields(pool: PgPool) {
    let order = seed_order(&pool, "update-multi-co").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let body = json!({
        "frozen": true,
        "chilled": true,
        "client": "Handle with care",
    });
    let updated: OrderRow =
        decode_ok(patch_admin(&server, &user.id, &format!("/api/orders/{}", order.id), &body).await)
            .await;

    assert!(updated.frozen);
    assert!(updated.chilled);
    assert_eq!(updated.client.as_deref(), Some("Handle with care"));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_change_persists_across_reads(pool: PgPool) {
    let order = seed_order(&pool, "update-persist-co").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let body = json!({ "client": "Updated note" });
    let _: OrderRow =
        decode_ok(patch_admin(&server, &user.id, &format!("/api/orders/{}", order.id), &body).await)
            .await;

    let detail: OrderDetailRow =
        decode_ok(get_admin(&server, &user.id, &format!("/api/orders/{}", order.id)).await).await;
    assert_eq!(detail.client.as_deref(), Some("Updated note"));

    server.stop().await;
}

// ---------------------------------------------------------------------------
// confirm — POST /api/orders/:id/confirm
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn confirm_transitions_pending_to_confirmed(pool: PgPool) {
    let order = seed_order(&pool, "confirm-ok-co").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let out: OrderRow = decode_ok(
        post_admin_empty(&server, &user.id, &format!("/api/orders/{}/confirm", order.id)).await,
    )
    .await;
    assert_eq!(out.status, "confirmed");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn confirm_returns_404_when_not_pending(pool: PgPool) {
    let order = seed_order(&pool, "confirm-twice-co").await;
    orders::confirm(&pool, order.id).await.unwrap().unwrap();

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin_empty(&server, &user.id, &format!("/api/orders/{}/confirm", order.id)).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn confirm_returns_404_for_fulfilled(pool: PgPool) {
    let order = seed_order(&pool, "confirm-fulfilled-co").await;
    orders::confirm(&pool, order.id).await.unwrap().unwrap();
    orders::fulfill(&pool, order.id).await.unwrap().unwrap();

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin_empty(&server, &user.id, &format!("/api/orders/{}/confirm", order.id)).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn confirm_returns_404_for_cancelled(pool: PgPool) {
    let order = seed_order(&pool, "confirm-cancelled-co").await;
    orders::cancel(&pool, order.id).await.unwrap().unwrap();

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin_empty(&server, &user.id, &format!("/api/orders/{}/confirm", order.id)).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn confirm_returns_404_for_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin_empty(
        &server,
        &user.id,
        "/api/orders/00000000-0000-0000-0000-000000000000/confirm",
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// fulfill — POST /api/orders/:id/fulfill
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn fulfill_transitions_confirmed_to_fulfilled(pool: PgPool) {
    let order = seed_order(&pool, "fulfill-ok-co").await;
    orders::confirm(&pool, order.id).await.unwrap().unwrap();

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let out: OrderRow = decode_ok(
        post_admin_empty(&server, &user.id, &format!("/api/orders/{}/fulfill", order.id)).await,
    )
    .await;
    assert_eq!(out.status, "fulfilled");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn fulfill_returns_404_when_still_pending(pool: PgPool) {
    let order = seed_order(&pool, "fulfill-pending-co").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin_empty(&server, &user.id, &format!("/api/orders/{}/fulfill", order.id)).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn fulfill_returns_404_when_already_fulfilled(pool: PgPool) {
    let order = seed_order(&pool, "fulfill-twice-co").await;
    orders::confirm(&pool, order.id).await.unwrap().unwrap();
    orders::fulfill(&pool, order.id).await.unwrap().unwrap();

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin_empty(&server, &user.id, &format!("/api/orders/{}/fulfill", order.id)).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn fulfill_returns_404_for_cancelled(pool: PgPool) {
    let order = seed_order(&pool, "fulfill-cancelled-co").await;
    orders::cancel(&pool, order.id).await.unwrap().unwrap();

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin_empty(&server, &user.id, &format!("/api/orders/{}/fulfill", order.id)).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// cancel — POST /api/orders/:id/cancel
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn cancel_cancels_pending(pool: PgPool) {
    let order = seed_order(&pool, "cancel-pending-co").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let out: OrderRow = decode_ok(
        post_admin_empty(&server, &user.id, &format!("/api/orders/{}/cancel", order.id)).await,
    )
    .await;
    assert_eq!(out.status, "cancelled");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancel_cancels_confirmed(pool: PgPool) {
    let order = seed_order(&pool, "cancel-confirmed-co").await;
    orders::confirm(&pool, order.id).await.unwrap().unwrap();

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let out: OrderRow = decode_ok(
        post_admin_empty(&server, &user.id, &format!("/api/orders/{}/cancel", order.id)).await,
    )
    .await;
    assert_eq!(out.status, "cancelled");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancel_returns_404_when_already_cancelled(pool: PgPool) {
    let order = seed_order(&pool, "cancel-twice-co").await;
    orders::cancel(&pool, order.id).await.unwrap().unwrap();

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin_empty(&server, &user.id, &format!("/api/orders/{}/cancel", order.id)).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancel_returns_404_for_fulfilled(pool: PgPool) {
    let order = seed_order(&pool, "cancel-fulfilled-co").await;
    orders::confirm(&pool, order.id).await.unwrap().unwrap();
    orders::fulfill(&pool, order.id).await.unwrap().unwrap();

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin_empty(&server, &user.id, &format!("/api/orders/{}/cancel", order.id)).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// set_status_batch — POST /api/orders/batch/status
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn batch_sets_many_statuses(pool: PgPool) {
    let o1 = seed_order(&pool, "batch-1").await;
    let o2 = seed_order(&pool, "batch-2").await;
    let o3 = seed_order(&pool, "batch-3").await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let body = json!({
        "ids": [o1.id, o2.id, o3.id],
        "status": "confirmed",
    });
    let rows: Vec<OrderRow> =
        decode_ok(post_admin(&server, &user.id, "/api/orders/batch/status", &body).await).await;

    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|r| r.status == "confirmed"));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn batch_empty_ids_returns_empty(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let body = json!({ "ids": [], "status": "confirmed" });
    let rows: Vec<OrderRow> =
        decode_ok(post_admin(&server, &user.id, "/api/orders/batch/status", &body).await).await;

    assert!(rows.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn batch_rejects_invalid_status(pool: PgPool) {
    let o = seed_order(&pool, "batch-bad-status").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let body = json!({ "ids": [o.id], "status": "not_a_status" });
    let res = post_admin(&server, &user.id, "/api/orders/batch/status", &body).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// Client scoping — the one behavior that diverges from admin.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn client_list_sees_only_own_company(pool: PgPool) {
    let mine = seed_company(&pool, "scope-mine").await;
    let theirs = seed_company(&pool, "scope-theirs").await;

    let mut d1 = default_insert(mine);
    d1.amount = 5.0;
    let my_order = orders::create(&pool, d1).await.expect("mine");
    let mut d2 = default_insert(theirs);
    d2.amount = 5.0;
    let their_order = orders::create(&pool, d2).await.expect("theirs");

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    // No `company_id` query param — scope filter must still apply.
    let rows: Vec<OrderSummaryRow> =
        decode_ok(get_client(&server, &user.id, mine, "/api/orders").await).await;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();

    assert!(ids.contains(&my_order.id));
    assert!(!ids.contains(&their_order.id));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_filter_into_other_company(pool: PgPool) {
    let mine = seed_company(&pool, "scope2-mine").await;
    let theirs = seed_company(&pool, "scope2-theirs").await;

    let mut d1 = default_insert(mine);
    d1.amount = 5.0;
    orders::create(&pool, d1).await.expect("mine");
    let mut d2 = default_insert(theirs);
    d2.amount = 5.0;
    orders::create(&pool, d2).await.expect("theirs");

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    // Client passes `company_id=<theirs>` — the handler must override.
    let rows: Vec<OrderSummaryRow> = decode_ok(
        get_client(&server, &user.id, mine, &format!("/api/orders?company_id={theirs}")).await,
    )
    .await;

    // Every row returned belongs to `mine`; none belong to `theirs`.
    assert!(rows.iter().all(|r| r.company_id == mine));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_get_by_id_404s_for_other_company_row(pool: PgPool) {
    let mine = seed_company(&pool, "detail-scope-mine").await;
    let theirs = seed_company(&pool, "detail-scope-theirs").await;

    let mut d = default_insert(theirs);
    d.amount = 5.0;
    let their_order = orders::create(&pool, d).await.expect("theirs");

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    // Client's company is `mine` — asking for a row in `theirs` must
    // 404 (lie about existence), not 403.
    let res = get_client(&server, &user.id, mine, &format!("/api/orders/{}", their_order.id)).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_create_for_other_company(pool: PgPool) {
    let mine = seed_company(&pool, "create-scope-mine").await;
    let theirs = seed_company(&pool, "create-scope-theirs").await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    // Client claims role=client, scope=mine, but posts company_id=theirs.
    let body = json!({
        "company_id": theirs,
        "order_source": "client_portal",
        "amount": 10.0,
        "serving_date": BASE_SERVING,
        "shipment_date": BASE_SHIPMENT,
    });
    let mut req = Client::new()
        .post(format!("{}/api/orders", server.base_url))
        .json(&body);
    for (k, v) in signed_headers(&user.id, "client", &mine.to_string()) {
        req = req.header(k, v);
    }
    let res = req.send().await.expect("POST");
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // Suppress unused on `theirs` beyond the json body.
    let _ = theirs;

    server.stop().await;
}

// ---------------------------------------------------------------------------
// Keep the helper imports alive — `UpdateOrderData` would be unused
// otherwise. Not a test; references the type so clippy stays quiet.
// ---------------------------------------------------------------------------

#[allow(dead_code)]
fn _ref_types() {
    let _ = InsertOrderData {
        company_id: Uuid::nil(),
        recipe_id: None,
        created_by: None,
        order_source: String::new(),
        status: None,
        meal_category: None,
        amount: 0.0,
        portion_size: None,
        day: None,
        serving_date: BASE_SERVING,
        shipment_date: BASE_SHIPMENT,
        frozen: None,
        chilled: None,
        recurring: None,
        client: None,
    };
    let _ = UpdateOrderData::default();
}
