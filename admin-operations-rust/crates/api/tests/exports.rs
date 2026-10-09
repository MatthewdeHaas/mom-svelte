//! Parity + determinism integration tests for `/api/exports/*`.
//!
//! Six happy-path/admin-reject cases, plus three byte-identical tests
//! that pin the determinism contract across the whole response pipe
//! (xlsx post-processor for the two xlsx endpoints, ZIP assembly for the
//! packing-slip multi-slip variant, raw PDF stream for the single-slip
//! variant). Shape:
//!
//! - xlsx responses: body starts with the ZIP local-file header
//!   `PK\x03\x04`.
//! - packing-slip single-slip: body starts with `%PDF`.
//! - packing-slip multi-slip: body starts with `PK\x03\x04`
//!   (ZIP archive).
//!
//! Byte-identity tests seed data, hit the endpoint twice, and compare
//! sha256 digests. Guards against a `rust_xlsxwriter` default change
//! or a `zip`-crate upgrade silently regressing the determinism
//! contract that `pin_xlsx_bytes` + the ZIP assembly options are
//! responsible for.
//!
//! Each test uses `#[sqlx::test(migrations = "../../migrations")]` so
//! every case gets a fresh DB and tests are embarrassingly parallel.

mod common;

use chrono::NaiveDate;
use common::{seed_user, signed_headers, spawn_server, TestServer};
use db::queries::orders::{self, InsertOrderData};
use reqwest::{Client, StatusCode};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Tiny HTTP helpers — same shape as the other test files. Admin signed
// headers only; the client role is minted inline per-test.
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

/// Fetch `path` as admin twice and return `(bytes_1, bytes_2)` so a
/// byte-identical test can assert them equal. Each call runs against
/// the same spawned server so no DB-write skew sneaks in.
async fn fetch_twice(server: &TestServer, user_id: &str, path: &str) -> (Vec<u8>, Vec<u8>) {
    let a = get_admin(server, user_id, path).await;
    assert_eq!(a.status(), StatusCode::OK, "first fetch must 200");
    let a_bytes = a.bytes().await.expect("first body").to_vec();

    let b = get_admin(server, user_id, path).await;
    assert_eq!(b.status(), StatusCode::OK, "second fetch must 200");
    let b_bytes = b.bytes().await.expect("second body").to_vec();

    (a_bytes, b_bytes)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

// ---------------------------------------------------------------------------
// Shared seed — one company + recipe + ingredient + an order dated inside
// the test window. The window below (2026-06-01..2026-06-30 for the xlsx
// endpoints, with 2026-06-15 as the serving/shipment date for the
// packing-slip endpoint) is far enough in the future that the
// `week_bounds_est` math is unambiguous (Monday is 2026-06-15 — yes, by
// coincidence, which is handy).
// ---------------------------------------------------------------------------

const SERVING: NaiveDate = match NaiveDate::from_ymd_opt(2026, 6, 17) {
    Some(d) => d,
    None => unreachable!(),
};
const SHIPMENT: NaiveDate = match NaiveDate::from_ymd_opt(2026, 6, 15) {
    Some(d) => d,
    None => unreachable!(),
};

/// Seed one order with a recipe + one ingredient so the shopping-list
/// generator has a non-empty aggregation path to exercise. Returns the
/// company id so admin-reject tests can reuse it.
async fn seed_minimal_order(pool: &PgPool) -> Uuid {
    let company_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO company (name, active) VALUES ('Exports Test Co', TRUE) RETURNING id",
    )
    .fetch_one(pool)
    .await
    .expect("seed company");

    let recipe_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO recipe (name, active) VALUES ('Exports Test Recipe', TRUE) RETURNING id",
    )
    .fetch_one(pool)
    .await
    .expect("seed recipe");

    let ingredient_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO ingredient (name, active) VALUES ('Exports Test Ingredient', TRUE) RETURNING id",
    )
    .fetch_one(pool)
    .await
    .expect("seed ingredient");

    sqlx::query(
        "INSERT INTO recipe_ingredient (recipe_id, ingredient_id, amount, unit) \
         VALUES ($1, $2, 1.0::float8::numeric, 'g') \
         ON CONFLICT (recipe_id, ingredient_id) DO NOTHING",
    )
    .bind(recipe_id)
    .bind(ingredient_id)
    .execute(pool)
    .await
    .expect("attach ingredient");

    let data = InsertOrderData {
        company_id,
        recipe_id: Some(recipe_id),
        created_by: None,
        order_source: "admin_manual".to_string(),
        status: Some("confirmed".to_string()),
        meal_category: Some("lunch".to_string()),
        amount: 10.0,
        portion_size: Some(100.0),
        day: None,
        serving_date: SERVING,
        shipment_date: SHIPMENT,
        frozen: Some(false),
        chilled: Some(true),
        recurring: None,
        client: Some("Test Client".to_string()),
    };
    orders::create(pool, data).await.expect("seed order");

    company_id
}

/// Seed a second company + order dated into the same shipment week so
/// the packing-slip endpoint returns `len > 1` and the handler takes
/// the ZIP branch. Reuses the first-company recipe/ingredient (there's
/// no uniqueness constraint that forbids cross-company reuse) and only
/// mints a new `(company, shipment_date)` group.
async fn seed_second_company_order(pool: &PgPool) -> Uuid {
    let company_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO company (name, active) VALUES ('Exports Test Co 2', TRUE) RETURNING id",
    )
    .fetch_one(pool)
    .await
    .expect("seed company 2");

    let recipe_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM recipe WHERE name = 'Exports Test Recipe'",
    )
    .fetch_one(pool)
    .await
    .expect("look up recipe");

    let data = InsertOrderData {
        company_id,
        recipe_id: Some(recipe_id),
        created_by: None,
        order_source: "admin_manual".to_string(),
        status: Some("confirmed".to_string()),
        meal_category: Some("lunch".to_string()),
        amount: 5.0,
        portion_size: Some(100.0),
        day: None,
        serving_date: SERVING,
        shipment_date: SHIPMENT,
        frozen: Some(false),
        chilled: Some(true),
        recurring: None,
        client: Some("Test Client 2".to_string()),
    };
    orders::create(pool, data).await.expect("seed order 2");

    company_id
}

// ---------------------------------------------------------------------------
// GET /api/exports/production-order
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn production_order_happy_path_returns_xlsx(pool: PgPool) {
    seed_minimal_order(&pool).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(
        &server,
        &user.id,
        "/api/exports/production-order?from=2026-06-01&to=2026-06-30",
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "expected 200");

    let disposition = res
        .headers()
        .get("content-disposition")
        .expect("content-disposition")
        .to_str()
        .expect("ascii disposition")
        .to_string();
    assert!(
        disposition.contains("production-order"),
        "disposition should carry endpoint name, got {disposition}"
    );

    let bytes = res.bytes().await.expect("body");
    assert!(!bytes.is_empty(), "xlsx body must not be empty");
    assert_eq!(
        &bytes[..4],
        b"PK\x03\x04",
        "xlsx must start with ZIP magic, got {:x?}",
        &bytes[..bytes.len().min(8)]
    );

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn production_order_rejects_client(pool: PgPool) {
    let company_id = seed_minimal_order(&pool).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        company_id,
        "/api/exports/production-order?from=2026-06-01&to=2026-06-30",
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

/// Two calls to `/exports/production-order` with the same seed produce
/// byte-identical responses. Guards `pin_xlsx_bytes` + the metadata
/// pinning on the workbook builder against a silent regression.
#[sqlx::test(migrations = "../../migrations")]
async fn production_order_bytes_identical_across_runs(pool: PgPool) {
    seed_minimal_order(&pool).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let (a, b) = fetch_twice(
        &server,
        &user.id,
        "/api/exports/production-order?from=2026-06-01&to=2026-06-30",
    )
    .await;
    assert_eq!(
        sha256_hex(&a),
        sha256_hex(&b),
        "production-order bytes must be identical across runs"
    );

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/exports/shopping-list
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn shopping_list_happy_path_returns_xlsx(pool: PgPool) {
    seed_minimal_order(&pool).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(
        &server,
        &user.id,
        "/api/exports/shopping-list?from=2026-06-01&to=2026-06-30",
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "expected 200");

    let disposition = res
        .headers()
        .get("content-disposition")
        .expect("content-disposition")
        .to_str()
        .expect("ascii disposition")
        .to_string();
    assert!(
        disposition.contains("shopping-list"),
        "disposition should carry endpoint name, got {disposition}"
    );

    let bytes = res.bytes().await.expect("body");
    assert!(!bytes.is_empty(), "xlsx body must not be empty");
    assert_eq!(
        &bytes[..4],
        b"PK\x03\x04",
        "xlsx must start with ZIP magic, got {:x?}",
        &bytes[..bytes.len().min(8)]
    );

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn shopping_list_rejects_client(pool: PgPool) {
    let company_id = seed_minimal_order(&pool).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        company_id,
        "/api/exports/shopping-list?from=2026-06-01&to=2026-06-30",
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn shopping_list_bytes_identical_across_runs(pool: PgPool) {
    seed_minimal_order(&pool).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let (a, b) = fetch_twice(
        &server,
        &user.id,
        "/api/exports/shopping-list?from=2026-06-01&to=2026-06-30",
    )
    .await;
    assert_eq!(
        sha256_hex(&a),
        sha256_hex(&b),
        "shopping-list bytes must be identical across runs"
    );

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/exports/packing-slips
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn packing_slips_single_company_returns_raw_pdf(pool: PgPool) {
    seed_minimal_order(&pool).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(
        &server,
        &user.id,
        "/api/exports/packing-slips?date=2026-06-17",
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "expected 200");

    let content_type = res
        .headers()
        .get("content-type")
        .expect("content-type")
        .to_str()
        .expect("ascii")
        .to_string();
    assert!(
        content_type.starts_with("application/pdf"),
        "single-slip must be served as application/pdf, got {content_type}"
    );

    let bytes = res.bytes().await.expect("body");
    assert!(bytes.len() >= 4, "pdf body must be non-trivial");
    assert_eq!(
        &bytes[..4],
        b"%PDF",
        "pdf must start with %PDF, got {:x?}",
        &bytes[..bytes.len().min(8)]
    );

    server.stop().await;
}

/// Multi-slip variant — seed two companies into the same shipment week
/// and assert the handler returns a ZIP archive instead of a raw PDF.
#[sqlx::test(migrations = "../../migrations")]
async fn packing_slips_multi_company_returns_zip(pool: PgPool) {
    seed_minimal_order(&pool).await;
    seed_second_company_order(&pool).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(
        &server,
        &user.id,
        "/api/exports/packing-slips?date=2026-06-17",
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "expected 200");

    let content_type = res
        .headers()
        .get("content-type")
        .expect("content-type")
        .to_str()
        .expect("ascii")
        .to_string();
    assert!(
        content_type.starts_with("application/zip"),
        "multi-slip must be served as application/zip, got {content_type}"
    );

    let disposition = res
        .headers()
        .get("content-disposition")
        .expect("content-disposition")
        .to_str()
        .expect("ascii disposition")
        .to_string();
    assert!(
        disposition.contains("packing-slips-week-of-"),
        "disposition should name the week, got {disposition}"
    );

    let bytes = res.bytes().await.expect("body");
    assert!(bytes.len() >= 4, "zip body must be non-trivial");
    assert_eq!(
        &bytes[..4],
        b"PK\x03\x04",
        "zip must start with ZIP magic, got {:x?}",
        &bytes[..bytes.len().min(8)]
    );

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn packing_slips_empty_week_returns_404(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    // No seeded orders at all — the week should be empty.
    let res = get_admin(
        &server,
        &user.id,
        "/api/exports/packing-slips?date=2026-06-17",
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body = res.text().await.expect("body");
    assert!(
        body.contains("No orders found"),
        "empty-week body should name the condition, got {body}"
    );

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn packing_slips_rejects_client(pool: PgPool) {
    let company_id = seed_minimal_order(&pool).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        company_id,
        "/api/exports/packing-slips?date=2026-06-17",
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

/// Byte-identical over the single-slip (raw PDF) + multi-slip (ZIP)
/// branches. The ZIP branch is the one that catches `zip`-crate
/// upgrades flipping the default mtime away from the DOS epoch.
#[sqlx::test(migrations = "../../migrations")]
async fn packing_slips_single_bytes_identical_across_runs(pool: PgPool) {
    seed_minimal_order(&pool).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let (a, b) = fetch_twice(
        &server,
        &user.id,
        "/api/exports/packing-slips?date=2026-06-17",
    )
    .await;
    assert_eq!(
        sha256_hex(&a),
        sha256_hex(&b),
        "single-slip pdf bytes must be identical across runs"
    );

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn packing_slips_multi_bytes_identical_across_runs(pool: PgPool) {
    seed_minimal_order(&pool).await;
    seed_second_company_order(&pool).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let (a, b) = fetch_twice(
        &server,
        &user.id,
        "/api/exports/packing-slips?date=2026-06-17",
    )
    .await;
    assert_eq!(
        sha256_hex(&a),
        sha256_hex(&b),
        "multi-slip zip bytes must be identical across runs"
    );

    server.stop().await;
}
