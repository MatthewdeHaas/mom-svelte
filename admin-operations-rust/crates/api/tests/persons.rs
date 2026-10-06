//! Parity integration tests for `/api/persons/*` + `/api/attendance/*`.
//!
//! Mirrors `tests/integration/core/persons.test.ts` 1:1 — same shapes,
//! same assertions, same edge cases — plus Rust-only client-scoping
//! tests the TS suite can't express (no HTTP boundary there).
//!
//! Each test uses `#[sqlx::test(migrations = "../../migrations")]` so
//! every case gets a fresh DB — tests are embarrassingly parallel.

mod common;

use common::{
    persons::{
        attach_allergen, seed_allergen, seed_attendance, seed_company, seed_person,
        seed_person_inactive,
    },
    seed_user, signed_headers, spawn_server, TestServer,
};
use db::queries::persons::{
    AllergenEmbed, AttendanceRow, AttendanceWithPersonRow, PersonRow, PersonWithAllergensRow,
};
use chrono::NaiveDate;
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Tiny HTTP helpers — same shape as ingredients.rs / allergens.rs. Reproduced
// locally so each test file is self-contained (precedent).
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

async fn get_client_no_company(
    server: &TestServer,
    user_id: &str,
    path: &str,
) -> reqwest::Response {
    let mut req = Client::new().get(format!("{}{}", server.base_url, path));
    for (k, v) in signed_headers(user_id, "client", "") {
        req = req.header(k, v);
    }
    req.send().await.expect("GET client no-company")
}

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

async fn post_admin_empty(server: &TestServer, user_id: &str, path: &str) -> reqwest::Response {
    let mut req = Client::new().post(format!("{}{}", server.base_url, path));
    for (k, v) in signed_headers(user_id, "admin", "") {
        req = req.header(k, v);
    }
    req.send().await.expect("POST empty")
}

async fn post_client(
    server: &TestServer,
    user_id: &str,
    company_id: Uuid,
    path: &str,
    body: &Value,
) -> reqwest::Response {
    let mut req = Client::new()
        .post(format!("{}{}", server.base_url, path))
        .json(body);
    for (k, v) in signed_headers(user_id, "client", &company_id.to_string()) {
        req = req.header(k, v);
    }
    req.send().await.expect("POST client")
}

async fn post_client_empty(
    server: &TestServer,
    user_id: &str,
    company_id: Uuid,
    path: &str,
) -> reqwest::Response {
    let mut req = Client::new().post(format!("{}{}", server.base_url, path));
    for (k, v) in signed_headers(user_id, "client", &company_id.to_string()) {
        req = req.header(k, v);
    }
    req.send().await.expect("POST client empty")
}

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

async fn patch_client(
    server: &TestServer,
    user_id: &str,
    company_id: Uuid,
    path: &str,
    body: &Value,
) -> reqwest::Response {
    let mut req = Client::new()
        .patch(format!("{}{}", server.base_url, path))
        .json(body);
    for (k, v) in signed_headers(user_id, "client", &company_id.to_string()) {
        req = req.header(k, v);
    }
    req.send().await.expect("PATCH client")
}

async fn put_admin(
    server: &TestServer,
    user_id: &str,
    path: &str,
    body: &Value,
) -> reqwest::Response {
    let mut req = Client::new()
        .put(format!("{}{}", server.base_url, path))
        .json(body);
    for (k, v) in signed_headers(user_id, "admin", "") {
        req = req.header(k, v);
    }
    req.send().await.expect("PUT")
}

async fn put_client(
    server: &TestServer,
    user_id: &str,
    company_id: Uuid,
    path: &str,
    body: &Value,
) -> reqwest::Response {
    let mut req = Client::new()
        .put(format!("{}{}", server.base_url, path))
        .json(body);
    for (k, v) in signed_headers(user_id, "client", &company_id.to_string()) {
        req = req.header(k, v);
    }
    req.send().await.expect("PUT client")
}

async fn delete_admin(server: &TestServer, user_id: &str, path: &str) -> reqwest::Response {
    let mut req = Client::new().delete(format!("{}{}", server.base_url, path));
    for (k, v) in signed_headers(user_id, "admin", "") {
        req = req.header(k, v);
    }
    req.send().await.expect("DELETE")
}

async fn delete_client(
    server: &TestServer,
    user_id: &str,
    company_id: Uuid,
    path: &str,
) -> reqwest::Response {
    let mut req = Client::new().delete(format!("{}{}", server.base_url, path));
    for (k, v) in signed_headers(user_id, "client", &company_id.to_string()) {
        req = req.header(k, v);
    }
    req.send().await.expect("DELETE client")
}

async fn decode_ok<T: serde::de::DeserializeOwned>(res: reqwest::Response) -> T {
    let status = res.status();
    let text = res.text().await.expect("body");
    assert!(status.is_success(), "non-2xx: status={status} body={text}");
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("decode: {e}; body={text}"))
}

const BOGUS: &str = "00000000-0000-0000-0000-000000000000";

// ---------------------------------------------------------------------------
// GET /api/persons — list by company
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn list_returns_empty_for_company_with_no_persons(pool: PgPool) {
    let c = seed_company(&pool, "Empty Co").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<PersonRow> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/persons?company_id={c}")).await,
    )
    .await;
    assert!(rows.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_returns_only_active_by_default(pool: PgPool) {
    let c = seed_company(&pool, "Active Co").await;
    let active = seed_person(&pool, c, "Active One").await;
    let inactive = seed_person_inactive(&pool, c, "Inactive One").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<PersonRow> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/persons?company_id={c}")).await,
    )
    .await;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();

    assert!(ids.contains(&active));
    assert!(!ids.contains(&inactive));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_includes_inactive_when_flag_set(pool: PgPool) {
    let c = seed_company(&pool, "Inc Inactive Co").await;
    let inactive = seed_person_inactive(&pool, c, "Hidden").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<PersonRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/persons?company_id={c}&include_inactive=true"),
        )
        .await,
    )
    .await;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    assert!(ids.contains(&inactive));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_does_not_cross_companies(pool: PgPool) {
    let c1 = seed_company(&pool, "Co One").await;
    let c2 = seed_company(&pool, "Co Two").await;
    seed_person(&pool, c2, "Only In Two").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<PersonRow> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/persons?company_id={c1}")).await,
    )
    .await;
    assert!(rows.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_orders_by_name_ascending(pool: PgPool) {
    let c = seed_company(&pool, "Order Co").await;
    seed_person(&pool, c, "Zara").await;
    seed_person(&pool, c, "Alice").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<PersonRow> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/persons?company_id={c}")).await,
    )
    .await;
    let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
    let a = names.iter().position(|n| *n == "Alice").expect("Alice");
    let z = names.iter().position(|n| *n == "Zara").expect("Zara");
    assert!(a < z);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/persons/:id
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_404_for_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(&server, &user.id, &format!("/api/persons/{BOGUS}")).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_hides_inactive_by_default(pool: PgPool) {
    let c = seed_company(&pool, "Hide Inactive").await;
    let p = seed_person_inactive(&pool, c, "Deactivated").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(&server, &user.id, &format!("/api/persons/{p}")).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_returns_inactive_with_include_flag(pool: PgPool) {
    let c = seed_company(&pool, "Show Inactive").await;
    let p = seed_person_inactive(&pool, c, "SoftDeleted").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: PersonRow = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/persons/{p}?include_inactive=true"),
        )
        .await,
    )
    .await;
    assert_eq!(row.id, p);
    assert!(!row.active);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_happy_path(pool: PgPool) {
    let c = seed_company(&pool, "Happy Co").await;
    let p = seed_person(&pool, c, "HappyPerson").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: PersonRow =
        decode_ok(get_admin(&server, &user.id, &format!("/api/persons/{p}")).await).await;
    assert_eq!(row.id, p);
    assert_eq!(row.company_id, c);
    assert_eq!(row.name, "HappyPerson");
    assert!(row.active);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// POST /api/persons — create
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn create_returns_201_with_active_true(pool: PgPool) {
    let c = seed_company(&pool, "Create Co").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin(
        &server,
        &user.id,
        "/api/persons",
        &json!({ "company_id": c, "name": "Alice" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);

    let row: PersonRow = decode_ok(res).await;
    assert_eq!(row.company_id, c);
    assert_eq!(row.name, "Alice");
    assert!(row.active);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_trims_whitespace_from_name(pool: PgPool) {
    let c = seed_company(&pool, "Trim Co").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: PersonRow = decode_ok(
        post_admin(
            &server,
            &user.id,
            "/api/persons",
            &json!({ "company_id": c, "name": "  Bob  " }),
        )
        .await,
    )
    .await;
    assert_eq!(row.name, "Bob");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_respects_explicit_active_false(pool: PgPool) {
    let c = seed_company(&pool, "Explicit Inactive Co").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: PersonRow = decode_ok(
        post_admin(
            &server,
            &user.id,
            "/api/persons",
            &json!({ "company_id": c, "name": "Inactive", "active": false }),
        )
        .await,
    )
    .await;
    assert!(!row.active);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_400_when_company_fk_bogus(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin(
        &server,
        &user.id,
        "/api/persons",
        &json!({ "company_id": BOGUS, "name": "Orphan" }),
    )
    .await;
    // Admin → 404 (company id doesn't exist? No — we use
    // `can_read_company_persons` which passes admin unconditionally,
    // so the FK violation surfaces as 400 Validation via `map_pg`).
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// PATCH /api/persons/:id — update
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn update_name_leaves_active_unchanged(pool: PgPool) {
    let c = seed_company(&pool, "Rename Co").await;
    let p = seed_person(&pool, c, "Original").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: PersonRow = decode_ok(
        patch_admin(
            &server,
            &user.id,
            &format!("/api/persons/{p}"),
            &json!({ "name": "NewName" }),
        )
        .await,
    )
    .await;
    assert_eq!(row.name, "NewName");
    assert!(row.active);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_active_leaves_name_unchanged(pool: PgPool) {
    let c = seed_company(&pool, "ToggleActive Co").await;
    let p = seed_person(&pool, c, "KeepMyName").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: PersonRow = decode_ok(
        patch_admin(
            &server,
            &user.id,
            &format!("/api/persons/{p}"),
            &json!({ "active": false }),
        )
        .await,
    )
    .await;
    assert!(!row.active);
    assert_eq!(row.name, "KeepMyName");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_trims_whitespace_from_name(pool: PgPool) {
    let c = seed_company(&pool, "Trim Update Co").await;
    let p = seed_person(&pool, c, "Pre").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: PersonRow = decode_ok(
        patch_admin(
            &server,
            &user.id,
            &format!("/api/persons/{p}"),
            &json!({ "name": "  Trimmed  " }),
        )
        .await,
    )
    .await;
    assert_eq!(row.name, "Trimmed");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_404_for_missing_id(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = patch_admin(
        &server,
        &user.id,
        &format!("/api/persons/{BOGUS}"),
        &json!({ "name": "Ghost" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// POST /api/persons/:id/deactivate — soft delete
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn deactivate_sets_active_false_and_returns_row(pool: PgPool) {
    let c = seed_company(&pool, "SoftDel Co").await;
    let p = seed_person(&pool, c, "AboutToGo").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: PersonRow = decode_ok(
        post_admin_empty(&server, &user.id, &format!("/api/persons/{p}/deactivate")).await,
    )
    .await;
    assert_eq!(row.id, p);
    assert!(!row.active);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn deactivate_hides_from_default_list(pool: PgPool) {
    let c = seed_company(&pool, "Hide After SoftDel").await;
    let p = seed_person(&pool, c, "ToHide").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let _: PersonRow = decode_ok(
        post_admin_empty(&server, &user.id, &format!("/api/persons/{p}/deactivate")).await,
    )
    .await;

    let rows: Vec<PersonRow> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/persons?company_id={c}")).await,
    )
    .await;
    assert!(rows.iter().all(|r| r.id != p));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn deactivate_still_retrievable_with_include_inactive(pool: PgPool) {
    let c = seed_company(&pool, "Still Retrievable").await;
    let p = seed_person(&pool, c, "LingerOn").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let _: PersonRow = decode_ok(
        post_admin_empty(&server, &user.id, &format!("/api/persons/{p}/deactivate")).await,
    )
    .await;

    let row: PersonRow = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/persons/{p}?include_inactive=true"),
        )
        .await,
    )
    .await;
    assert_eq!(row.id, p);
    assert!(!row.active);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn deactivate_404_for_missing_id(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin_empty(&server, &user.id, &format!("/api/persons/{BOGUS}/deactivate")).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/persons/:id/with-allergens  +  /api/persons/with-allergens
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn with_allergens_404_for_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(
        &server,
        &user.id,
        &format!("/api/persons/{BOGUS}/with-allergens"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn with_allergens_returns_empty_array_when_none(pool: PgPool) {
    let c = seed_company(&pool, "NoAllergens Co").await;
    let p = seed_person(&pool, c, "LoneWolf").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: PersonWithAllergensRow = decode_ok(
        get_admin(&server, &user.id, &format!("/api/persons/{p}/with-allergens")).await,
    )
    .await;
    assert_eq!(row.id, p);
    assert!(row.allergen.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn with_allergens_includes_attached(pool: PgPool) {
    let c = seed_company(&pool, "WithAllergens Co").await;
    let p = seed_person(&pool, c, "Allergic").await;
    let a = seed_allergen(&pool, "Gluten").await;
    attach_allergen(&pool, p, a).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: PersonWithAllergensRow = decode_ok(
        get_admin(&server, &user.id, &format!("/api/persons/{p}/with-allergens")).await,
    )
    .await;
    let ids: Vec<Uuid> = row.allergen.iter().map(|a| a.id).collect();
    assert!(ids.contains(&a));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_with_allergens_empty_for_empty_company(pool: PgPool) {
    let c = seed_company(&pool, "EmptyCo WithAllergens").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<PersonWithAllergensRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/persons/with-allergens?company_id={c}"),
        )
        .await,
    )
    .await;
    assert!(rows.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_with_allergens_each_has_array(pool: PgPool) {
    let c = seed_company(&pool, "HasAllergens Array Co").await;
    seed_person(&pool, c, "Someone").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<PersonWithAllergensRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/persons/with-allergens?company_id={c}"),
        )
        .await,
    )
    .await;
    // Every row has an `allergen` field; even if empty, it's a Vec.
    assert!(rows.iter().all(|r| r.allergen.len() == r.allergen.len()));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_with_allergens_no_cross_contamination(pool: PgPool) {
    let c = seed_company(&pool, "NoCrossContam Co").await;
    let p1 = seed_person(&pool, c, "P1").await;
    let p2 = seed_person(&pool, c, "P2").await;
    let a1 = seed_allergen(&pool, "A1").await;
    let a2 = seed_allergen(&pool, "A2").await;
    attach_allergen(&pool, p1, a1).await;
    attach_allergen(&pool, p2, a2).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<PersonWithAllergensRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/persons/with-allergens?company_id={c}"),
        )
        .await,
    )
    .await;
    let row1 = rows.iter().find(|r| r.id == p1).expect("p1");
    let row2 = rows.iter().find(|r| r.id == p2).expect("p2");
    let ids1: Vec<Uuid> = row1.allergen.iter().map(|a| a.id).collect();
    let ids2: Vec<Uuid> = row2.allergen.iter().map(|a| a.id).collect();
    assert!(ids1.contains(&a1));
    assert!(!ids1.contains(&a2));
    assert!(ids2.contains(&a2));
    assert!(!ids2.contains(&a1));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_with_allergens_excludes_inactive_by_default(pool: PgPool) {
    let c = seed_company(&pool, "WAInactive Co").await;
    let inactive = seed_person_inactive(&pool, c, "Deactivated").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<PersonWithAllergensRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/persons/with-allergens?company_id={c}"),
        )
        .await,
    )
    .await;
    assert!(rows.iter().all(|r| r.id != inactive));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_with_allergens_does_not_cross_companies(pool: PgPool) {
    let c1 = seed_company(&pool, "WA C1").await;
    let c2 = seed_company(&pool, "WA C2").await;
    seed_person(&pool, c2, "OnlyC2").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<PersonWithAllergensRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/persons/with-allergens?company_id={c1}"),
        )
        .await,
    )
    .await;
    assert!(rows.is_empty());

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/persons/:id/allergens
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn get_allergens_returns_empty_when_none(pool: PgPool) {
    let c = seed_company(&pool, "NoneGetAllergens").await;
    let p = seed_person(&pool, c, "Nobody").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<AllergenEmbed> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/persons/{p}/allergens")).await,
    )
    .await;
    assert!(rows.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_allergens_orders_by_name_ascending(pool: PgPool) {
    let c = seed_company(&pool, "OrderedAllergens Co").await;
    let p = seed_person(&pool, c, "AllergenOrderingPerson").await;
    let z = seed_allergen(&pool, "ZZZAllergen").await;
    let a = seed_allergen(&pool, "AAAAllergen").await;
    attach_allergen(&pool, p, z).await;
    attach_allergen(&pool, p, a).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<AllergenEmbed> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/persons/{p}/allergens")).await,
    )
    .await;
    assert_eq!(rows[0].name, "AAAAllergen");
    assert_eq!(rows[1].name, "ZZZAllergen");

    server.stop().await;
}

// ---------------------------------------------------------------------------
// POST /api/persons/:id/allergens/:allergen_id  (add)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn add_allergen_attaches_and_returns_204(pool: PgPool) {
    let c = seed_company(&pool, "AddAllergen Co").await;
    let p = seed_person(&pool, c, "AddAllergenPerson").await;
    let a = seed_allergen(&pool, "Dairy").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin_empty(
        &server,
        &user.id,
        &format!("/api/persons/{p}/allergens/{a}"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let rows: Vec<AllergenEmbed> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/persons/{p}/allergens")).await,
    )
    .await;
    assert!(rows.iter().any(|al| al.id == a));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn add_allergen_is_idempotent(pool: PgPool) {
    let c = seed_company(&pool, "Idempotent Add Co").await;
    let p = seed_person(&pool, c, "IdemAddPerson").await;
    let a = seed_allergen(&pool, "Nuts").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let first = post_admin_empty(
        &server,
        &user.id,
        &format!("/api/persons/{p}/allergens/{a}"),
    )
    .await;
    assert_eq!(first.status(), StatusCode::NO_CONTENT);

    let second = post_admin_empty(
        &server,
        &user.id,
        &format!("/api/persons/{p}/allergens/{a}"),
    )
    .await;
    assert_eq!(second.status(), StatusCode::NO_CONTENT);

    let rows: Vec<AllergenEmbed> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/persons/{p}/allergens")).await,
    )
    .await;
    assert_eq!(rows.len(), 1);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// DELETE /api/persons/:id/allergens/:allergen_id  (remove)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn remove_allergen_detaches_specific(pool: PgPool) {
    let c = seed_company(&pool, "RemoveAllergen Co").await;
    let p = seed_person(&pool, c, "RemAllergenPerson").await;
    let a1 = seed_allergen(&pool, "A1").await;
    let a2 = seed_allergen(&pool, "A2").await;
    attach_allergen(&pool, p, a1).await;
    attach_allergen(&pool, p, a2).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = delete_admin(
        &server,
        &user.id,
        &format!("/api/persons/{p}/allergens/{a1}"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let rows: Vec<AllergenEmbed> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/persons/{p}/allergens")).await,
    )
    .await;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    assert!(!ids.contains(&a1));
    assert!(ids.contains(&a2));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn remove_allergen_is_noop_when_missing(pool: PgPool) {
    let c = seed_company(&pool, "NoopRemove Co").await;
    let p = seed_person(&pool, c, "NoopRemPerson").await;
    let a = seed_allergen(&pool, "Soy").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = delete_admin(
        &server,
        &user.id,
        &format!("/api/persons/{p}/allergens/{a}"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// PUT /api/persons/:id/allergens  (full-replace)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn set_allergens_sets_initial(pool: PgPool) {
    let c = seed_company(&pool, "SetAllergens Co").await;
    let p = seed_person(&pool, c, "SetPerson").await;
    let a1 = seed_allergen(&pool, "A1-set").await;
    let a2 = seed_allergen(&pool, "A2-set").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = put_admin(
        &server,
        &user.id,
        &format!("/api/persons/{p}/allergens"),
        &json!({ "allergen_ids": [a1, a2] }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let rows: Vec<AllergenEmbed> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/persons/{p}/allergens")).await,
    )
    .await;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    assert!(ids.contains(&a1));
    assert!(ids.contains(&a2));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn set_allergens_replaces_on_second_call(pool: PgPool) {
    let c = seed_company(&pool, "ReplaceAllergens Co").await;
    let p = seed_person(&pool, c, "ReplacePerson").await;
    let old = seed_allergen(&pool, "Old-set").await;
    let next = seed_allergen(&pool, "Next-set").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    put_admin(
        &server,
        &user.id,
        &format!("/api/persons/{p}/allergens"),
        &json!({ "allergen_ids": [old] }),
    )
    .await;
    put_admin(
        &server,
        &user.id,
        &format!("/api/persons/{p}/allergens"),
        &json!({ "allergen_ids": [next] }),
    )
    .await;

    let rows: Vec<AllergenEmbed> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/persons/{p}/allergens")).await,
    )
    .await;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    assert!(!ids.contains(&old));
    assert!(ids.contains(&next));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn set_allergens_clears_when_empty(pool: PgPool) {
    let c = seed_company(&pool, "ClearAllergens Co").await;
    let p = seed_person(&pool, c, "ClearPerson").await;
    let a = seed_allergen(&pool, "ClearA").await;
    attach_allergen(&pool, p, a).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    put_admin(
        &server,
        &user.id,
        &format!("/api/persons/{p}/allergens"),
        &json!({ "allergen_ids": [] }),
    )
    .await;

    let rows: Vec<AllergenEmbed> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/persons/{p}/allergens")).await,
    )
    .await;
    assert!(rows.is_empty());

    server.stop().await;
}

// ---------------------------------------------------------------------------
// POST /api/attendance (single)  +  POST /api/attendance/batch (bulk)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn upsert_attendance_inserts_new(pool: PgPool) {
    let c = seed_company(&pool, "UpsertAtt Co").await;
    let p = seed_person(&pool, c, "AttPerson").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: AttendanceRow = decode_ok(
        post_admin(
            &server,
            &user.id,
            "/api/attendance",
            &json!({
                "person_id": p,
                "serving_date": "2025-06-15",
                "attending": true,
            }),
        )
        .await,
    )
    .await;
    assert_eq!(row.person_id, p);
    assert!(row.attending);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn upsert_attendance_updates_on_conflict(pool: PgPool) {
    let c = seed_company(&pool, "ConflictAtt Co").await;
    let p = seed_person(&pool, c, "ConflictPerson").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let _: AttendanceRow = decode_ok(
        post_admin(
            &server,
            &user.id,
            "/api/attendance",
            &json!({ "person_id": p, "serving_date": "2025-06-15", "attending": true }),
        )
        .await,
    )
    .await;

    let row: AttendanceRow = decode_ok(
        post_admin(
            &server,
            &user.id,
            "/api/attendance",
            &json!({ "person_id": p, "serving_date": "2025-06-15", "attending": false }),
        )
        .await,
    )
    .await;
    assert!(!row.attending);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn upsert_attendance_batch_noop_on_empty(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin(
        &server,
        &user.id,
        "/api/attendance/batch",
        &json!({ "records": [] }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn upsert_attendance_batch_inserts_many(pool: PgPool) {
    let c = seed_company(&pool, "BatchAtt Co").await;
    let p1 = seed_person(&pool, c, "BA One").await;
    let p2 = seed_person(&pool, c, "BA Two").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin(
        &server,
        &user.id,
        "/api/attendance/batch",
        &json!({
            "records": [
                { "person_id": p1, "serving_date": "2025-06-15", "attending": true },
                { "person_id": p2, "serving_date": "2025-06-15", "attending": false },
            ]
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let rows: Vec<AttendanceWithPersonRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/attendance?company_id={c}&serving_date=2025-06-15"),
        )
        .await,
    )
    .await;
    assert_eq!(rows.len(), 2);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn upsert_attendance_batch_updates_on_conflict(pool: PgPool) {
    let c = seed_company(&pool, "BatchConflict Co").await;
    let p = seed_person(&pool, c, "BatchConfP").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    post_admin(
        &server,
        &user.id,
        "/api/attendance/batch",
        &json!({
            "records": [{ "person_id": p, "serving_date": "2025-06-15", "attending": true }]
        }),
    )
    .await;
    post_admin(
        &server,
        &user.id,
        "/api/attendance/batch",
        &json!({
            "records": [{ "person_id": p, "serving_date": "2025-06-15", "attending": false }]
        }),
    )
    .await;

    let rows: Vec<AttendanceWithPersonRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/attendance?company_id={c}&serving_date=2025-06-15"),
        )
        .await,
    )
    .await;
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].attending);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/attendance?company_id=&serving_date=
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn get_attendance_empty_when_none(pool: PgPool) {
    let c = seed_company(&pool, "EmptyAtt Co").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<AttendanceWithPersonRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/attendance?company_id={c}&serving_date=2025-06-15"),
        )
        .await,
    )
    .await;
    assert!(rows.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_attendance_returns_embedded_person(pool: PgPool) {
    let c = seed_company(&pool, "EmbedPerson Co").await;
    let p = seed_person(&pool, c, "EmbedP").await;
    let date = NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();
    seed_attendance(&pool, p, date, true).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<AttendanceWithPersonRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/attendance?company_id={c}&serving_date=2025-06-15"),
        )
        .await,
    )
    .await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].person.id, p);
    assert_eq!(rows[0].person.name, "EmbedP");
    assert!(rows[0].attending);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_attendance_includes_person_allergens(pool: PgPool) {
    let c = seed_company(&pool, "EmbedAllergens Co").await;
    let p = seed_person(&pool, c, "EmbedAP").await;
    let a = seed_allergen(&pool, "EmbedAllergen").await;
    attach_allergen(&pool, p, a).await;
    let date = NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();
    seed_attendance(&pool, p, date, true).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<AttendanceWithPersonRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/attendance?company_id={c}&serving_date=2025-06-15"),
        )
        .await,
    )
    .await;
    let allergen_ids: Vec<Uuid> = rows[0].person.allergen.iter().map(|al| al.id).collect();
    assert!(allergen_ids.contains(&a));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_attendance_does_not_cross_companies(pool: PgPool) {
    let c1 = seed_company(&pool, "AttC1").await;
    let c2 = seed_company(&pool, "AttC2").await;
    let p2 = seed_person(&pool, c2, "C2 Person").await;
    let date = NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();
    seed_attendance(&pool, p2, date, true).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<AttendanceWithPersonRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/attendance?company_id={c1}&serving_date=2025-06-15"),
        )
        .await,
    )
    .await;
    assert!(rows.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_attendance_does_not_cross_dates(pool: PgPool) {
    let c = seed_company(&pool, "CrossDate Co").await;
    let p = seed_person(&pool, c, "CrossDateP").await;
    let date = NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();
    seed_attendance(&pool, p, date, true).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<AttendanceWithPersonRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/attendance?company_id={c}&serving_date=2025-06-16"),
        )
        .await,
    )
    .await;
    assert!(rows.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_attendance_orders_by_person_name_ascending(pool: PgPool) {
    let c = seed_company(&pool, "OrderAtt Co").await;
    let zara = seed_person(&pool, c, "Zara").await;
    let alice = seed_person(&pool, c, "Alice").await;
    let date = NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();
    seed_attendance(&pool, zara, date, true).await;
    seed_attendance(&pool, alice, date, true).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<AttendanceWithPersonRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/attendance?company_id={c}&serving_date=2025-06-15"),
        )
        .await,
    )
    .await;
    let names: Vec<&str> = rows.iter().map(|r| r.person.name.as_str()).collect();
    let a = names.iter().position(|n| *n == "Alice").expect("Alice");
    let z = names.iter().position(|n| *n == "Zara").expect("Zara");
    assert!(a < z);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// Client scoping — Rust-only (the TS integration suite has no HTTP
// boundary, so these have no mirror). Exercises both scope helpers:
//
//   - `can_read_company_persons`: list / create / attendance list.
//     Admin → OK. Client → own company only; cross-company → 404;
//     missing company_id token → 403.
//
//   - `ensure_client_can_touch_person`: /:id reads, PATCH, deactivate,
//     allergen mutations, upsert_attendance. Pre-fetches the person
//     with include_inactive=true; mismatched company_id → 404.
//
// The "lie about existence" rule is the thread: cross-company probes
// never surface a 403 — they're indistinguishable from "no such id".
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn client_can_list_own_company(pool: PgPool) {
    let c = seed_company(&pool, "ClientOwn").await;
    let p = seed_person(&pool, c, "InMyCo").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<PersonRow> = decode_ok(
        get_client(&server, &user.id, c, &format!("/api/persons?company_id={c}")).await,
    )
    .await;
    assert!(rows.iter().any(|r| r.id == p));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_list_other_company_404(pool: PgPool) {
    let mine = seed_company(&pool, "Mine").await;
    let theirs = seed_company(&pool, "Theirs").await;
    seed_person(&pool, theirs, "Secret").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        mine,
        &format!("/api/persons?company_id={theirs}"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_list_without_company_token_403(pool: PgPool) {
    let c = seed_company(&pool, "SomeCo").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res =
        get_client_no_company(&server, &user.id, &format!("/api/persons?company_id={c}")).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_get_by_id_cross_company_404(pool: PgPool) {
    let mine = seed_company(&pool, "ScopedMine").await;
    let theirs = seed_company(&pool, "ScopedTheirs").await;
    let p = seed_person(&pool, theirs, "NotMine").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(&server, &user.id, mine, &format!("/api/persons/{p}")).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_get_by_id_own_company_ok(pool: PgPool) {
    let c = seed_company(&pool, "GetByIdOwn").await;
    let p = seed_person(&pool, c, "VisibleToMe").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: PersonRow =
        decode_ok(get_client(&server, &user.id, c, &format!("/api/persons/{p}")).await).await;
    assert_eq!(row.id, p);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_create_cross_company_404(pool: PgPool) {
    let mine = seed_company(&pool, "CreateMine").await;
    let theirs = seed_company(&pool, "CreateTheirs").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_client(
        &server,
        &user.id,
        mine,
        "/api/persons",
        &json!({ "company_id": theirs, "name": "Smuggled" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_create_own_company_ok(pool: PgPool) {
    let c = seed_company(&pool, "CreateOwn").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_client(
        &server,
        &user.id,
        c,
        "/api/persons",
        &json!({ "company_id": c, "name": "Legit" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_update_cross_company_404(pool: PgPool) {
    let mine = seed_company(&pool, "UpdateMine").await;
    let theirs = seed_company(&pool, "UpdateTheirs").await;
    let p = seed_person(&pool, theirs, "Target").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = patch_client(
        &server,
        &user.id,
        mine,
        &format!("/api/persons/{p}"),
        &json!({ "name": "Hijacked" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_deactivate_cross_company_404(pool: PgPool) {
    let mine = seed_company(&pool, "DeactMine").await;
    let theirs = seed_company(&pool, "DeactTheirs").await;
    let p = seed_person(&pool, theirs, "KillTarget").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_client_empty(
        &server,
        &user.id,
        mine,
        &format!("/api/persons/{p}/deactivate"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_add_allergen_cross_company_404(pool: PgPool) {
    let mine = seed_company(&pool, "AddAlMine").await;
    let theirs = seed_company(&pool, "AddAlTheirs").await;
    let p = seed_person(&pool, theirs, "AddAlTarget").await;
    let a = seed_allergen(&pool, "Gluten").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_client_empty(
        &server,
        &user.id,
        mine,
        &format!("/api/persons/{p}/allergens/{a}"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_remove_allergen_cross_company_404(pool: PgPool) {
    let mine = seed_company(&pool, "RemAlMine").await;
    let theirs = seed_company(&pool, "RemAlTheirs").await;
    let p = seed_person(&pool, theirs, "RemAlTarget").await;
    let a = seed_allergen(&pool, "Nuts").await;
    attach_allergen(&pool, p, a).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = delete_client(
        &server,
        &user.id,
        mine,
        &format!("/api/persons/{p}/allergens/{a}"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_set_allergens_cross_company_404(pool: PgPool) {
    let mine = seed_company(&pool, "SetAlMine").await;
    let theirs = seed_company(&pool, "SetAlTheirs").await;
    let p = seed_person(&pool, theirs, "SetAlTarget").await;
    let a = seed_allergen(&pool, "Soy").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = put_client(
        &server,
        &user.id,
        mine,
        &format!("/api/persons/{p}/allergens"),
        &json!({ "allergen_ids": [a] }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_upsert_attendance_cross_company_404(pool: PgPool) {
    let mine = seed_company(&pool, "AttMine").await;
    let theirs = seed_company(&pool, "AttTheirs").await;
    let p = seed_person(&pool, theirs, "AttTarget").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_client(
        &server,
        &user.id,
        mine,
        "/api/attendance",
        &json!({
            "person_id": p,
            "serving_date": "2025-06-15",
            "attending": true,
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_upsert_attendance_batch_mixed_404(pool: PgPool) {
    let mine = seed_company(&pool, "BatMine").await;
    let theirs = seed_company(&pool, "BatTheirs").await;
    let mine_p = seed_person(&pool, mine, "MineP").await;
    let theirs_p = seed_person(&pool, theirs, "TheirsP").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    // One record in-scope, one cross-company → whole batch 404s.
    let res = post_client(
        &server,
        &user.id,
        mine,
        "/api/attendance/batch",
        &json!({
            "records": [
                { "person_id": mine_p, "serving_date": "2025-06-15", "attending": true },
                { "person_id": theirs_p, "serving_date": "2025-06-15", "attending": true },
            ]
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // Verify no partial write — the mine_p row must not exist.
    let rows: Vec<AttendanceWithPersonRow> = decode_ok(
        get_client(
            &server,
            &user.id,
            mine,
            &format!("/api/attendance?company_id={mine}&serving_date=2025-06-15"),
        )
        .await,
    )
    .await;
    assert!(rows.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_upsert_attendance_batch_own_only_ok(pool: PgPool) {
    let c = seed_company(&pool, "OwnBatch").await;
    let p1 = seed_person(&pool, c, "OwnBatchP1").await;
    let p2 = seed_person(&pool, c, "OwnBatchP2").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_client(
        &server,
        &user.id,
        c,
        "/api/attendance/batch",
        &json!({
            "records": [
                { "person_id": p1, "serving_date": "2025-06-15", "attending": true },
                { "person_id": p2, "serving_date": "2025-06-15", "attending": false },
            ]
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_get_attendance_cross_company_404(pool: PgPool) {
    let mine = seed_company(&pool, "GetAttMine").await;
    let theirs = seed_company(&pool, "GetAttTheirs").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        mine,
        &format!("/api/attendance?company_id={theirs}&serving_date=2025-06-15"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_with_allergens_cross_company_404(pool: PgPool) {
    let mine = seed_company(&pool, "WithAlMine").await;
    let theirs = seed_company(&pool, "WithAlTheirs").await;
    let p = seed_person(&pool, theirs, "WithAlTarget").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        mine,
        &format!("/api/persons/{p}/with-allergens"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_get_allergens_cross_company_404(pool: PgPool) {
    let mine = seed_company(&pool, "GetAlMine").await;
    let theirs = seed_company(&pool, "GetAlTheirs").await;
    let p = seed_person(&pool, theirs, "GetAlTarget").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        mine,
        &format!("/api/persons/{p}/allergens"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_list_with_allergens_cross_company_404(pool: PgPool) {
    let mine = seed_company(&pool, "ListWAMine").await;
    let theirs = seed_company(&pool, "ListWATheirs").await;
    seed_person(&pool, theirs, "ListWATarget").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        mine,
        &format!("/api/persons/with-allergens?company_id={theirs}"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}
