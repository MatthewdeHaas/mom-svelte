//! Parity integration tests for `/api/exports/*`.
//!
//! Scope — happy path + admin-reject, per endpoint. Six tests total.
//! The content-correctness / byte-identical oracle lives in the next
//! sub-session; this file just proves the handlers wire up, the
//! generators run end-to-end against a real DB, and the admin gate
//! holds. Format shape is checked at the magic-byte level only:
//!   - xlsx responses: body starts with the ZIP local-file header
//!     `PK\x03\x04` (same shape rust_xlsxwriter emits).
//!   - packing-slip JSON: decodes to `Vec<PackingSlipJson>`, each
//!     `pdf_base64` decodes to bytes beginning with `%PDF`.
//!
//! Each test uses `#[sqlx::test(migrations = "../../migrations")]` so
//! every case gets a fresh DB and tests are embarrassingly parallel.

mod common;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::NaiveDate;
use common::{seed_user, signed_headers, spawn_server, TestServer};
use db::queries::orders::{self, InsertOrderData};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
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

    // Content-Disposition should carry an attachment filename framed on
    // the from/to window. Loose match — exact spelling is a handler
    // implementation detail that B2's oracle run will lock down.
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

// ---------------------------------------------------------------------------
// GET /api/exports/packing-slips
// ---------------------------------------------------------------------------

/// Mirror of `PackingSlipJson` in `crates/api/src/exports.rs`. Kept
/// local so the test can decode the response without exporting the
/// handler's type publicly.
#[derive(Debug, Deserialize)]
struct PackingSlipJsonLocal {
    #[allow(dead_code)]
    filename: String,
    #[allow(dead_code)]
    company_name: String,
    pdf_base64: String,
}

#[sqlx::test(migrations = "../../migrations")]
async fn packing_slips_happy_path_returns_json_pdfs(pool: PgPool) {
    seed_minimal_order(&pool).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    // Any date inside the EST Mon-Sun week containing SERVING (2026-06-17,
    // a Wednesday) works. Use SERVING itself.
    let res = get_admin(
        &server,
        &user.id,
        "/api/exports/packing-slips?date=2026-06-17",
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "expected 200");

    let slips: Vec<PackingSlipJsonLocal> = res.json().await.expect("decode JSON");
    assert!(
        !slips.is_empty(),
        "expected at least one packing slip for the seeded order"
    );

    let first_pdf = STANDARD
        .decode(&slips[0].pdf_base64)
        .expect("base64 decodes");
    assert!(
        first_pdf.len() >= 4,
        "decoded pdf must be non-trivial, got {} bytes",
        first_pdf.len()
    );
    assert_eq!(
        &first_pdf[..4],
        b"%PDF",
        "decoded body must start with %PDF, got {:x?}",
        &first_pdf[..first_pdf.len().min(8)]
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
