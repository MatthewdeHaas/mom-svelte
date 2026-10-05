//! Parity integration tests for `/api/companies/*`.
//!
//! Mirrors `tests/integration/core/companies.test.ts` 1:1 — same shapes,
//! same assertions, same edge cases. Plus Rust-only client-scoping
//! tests the TS suite can't express (no HTTP boundary there).
//!
//! Each test uses `#[sqlx::test(migrations = "../../migrations")]` so
//! every case gets a fresh DB — tests are embarrassingly parallel.

mod common;

use common::{
    companies::{
        seed_company, seed_company_full, seed_company_inactive, seed_user_in_company,
    },
    seed_user, signed_headers, spawn_server, TestServer,
};
use db::queries::companies::{CompanyRow, UserWithCompanyRow};
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Tiny HTTP helpers — same shape as recipes.rs. Reproduced locally so each
// test file is self-contained.
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

async fn decode_ok<T: serde::de::DeserializeOwned>(res: reqwest::Response) -> T {
    let status = res.status();
    let text = res.text().await.expect("body");
    assert!(status.is_success(), "non-2xx: status={status} body={text}");
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("decode: {e}; body={text}"))
}

const BOGUS: &str = "00000000-0000-0000-0000-000000000000";

// ---------------------------------------------------------------------------
// GET /api/companies — list
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn list_excludes_inactive_by_default(pool: PgPool) {
    let active = seed_company(&pool, "c-active").await;
    let inactive = seed_company_inactive(&pool, "c-inactive").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<CompanyRow> =
        decode_ok(get_admin(&server, &user.id, "/api/companies").await).await;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();

    assert!(ids.contains(&active));
    assert!(!ids.contains(&inactive));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_includes_inactive_when_flag_set(pool: PgPool) {
    let inactive = seed_company_inactive(&pool, "c-inactive").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<CompanyRow> = decode_ok(
        get_admin(&server, &user.id, "/api/companies?include_inactive=true").await,
    )
    .await;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();

    assert!(ids.contains(&inactive));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_orders_rows_by_name_ascending(pool: PgPool) {
    seed_company(&pool, "Zeta Corp").await;
    seed_company(&pool, "Alpha Corp").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<CompanyRow> =
        decode_ok(get_admin(&server, &user.id, "/api/companies").await).await;
    let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();

    let alpha = names.iter().position(|n| *n == "Alpha Corp").expect("Alpha");
    let zeta = names.iter().position(|n| *n == "Zeta Corp").expect("Zeta");
    assert!(alpha < zeta);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_rows_have_expected_shape(pool: PgPool) {
    let id = seed_company_full(&pool, "ShapeCo", Some("a@b.com"), None, None).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<CompanyRow> =
        decode_ok(get_admin(&server, &user.id, "/api/companies").await).await;
    let found = rows.iter().find(|r| r.id == id).expect("found");

    assert_eq!(found.name, "ShapeCo");
    assert_eq!(found.contact_email.as_deref(), Some("a@b.com"));
    assert!(found.active);
    // DateTime<Utc> decoded without panic — shape is sound.
    let _ = found.created_at;

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/companies/:id — get_by_id
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_returns_404_for_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(&server, &user.id, &format!("/api/companies/{BOGUS}")).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_returns_inactive_rows(pool: PgPool) {
    // TS: "returns inactive companies (no active filter on this query)"
    let id = seed_company_inactive(&pool, "inactive-c").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: CompanyRow =
        decode_ok(get_admin(&server, &user.id, &format!("/api/companies/{id}")).await).await;

    assert_eq!(row.id, id);
    assert!(!row.active);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_id_returns_full_row(pool: PgPool) {
    let id = seed_company_full(
        &pool,
        "FullCo",
        Some("full@example.com"),
        Some("555-1234"),
        Some("123 Main St"),
    )
    .await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: CompanyRow =
        decode_ok(get_admin(&server, &user.id, &format!("/api/companies/{id}")).await).await;

    assert_eq!(row.id, id);
    assert_eq!(row.contact_email.as_deref(), Some("full@example.com"));
    assert_eq!(row.phone.as_deref(), Some("555-1234"));
    assert_eq!(row.shipping_address.as_deref(), Some("123 Main St"));

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/companies/by-name — get_by_name
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn by_name_returns_404_for_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(
        &server,
        &user.id,
        "/api/companies/by-name?name=Definitely+Not+Real",
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn by_name_returns_exact_match(pool: PgPool) {
    let id = seed_company(&pool, "Exact Match Co").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: CompanyRow = decode_ok(
        get_admin(
            &server,
            &user.id,
            "/api/companies/by-name?name=Exact+Match+Co",
        )
        .await,
    )
    .await;

    assert_eq!(row.id, id);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn by_name_is_case_insensitive(pool: PgPool) {
    let id = seed_company(&pool, "Case Test Co").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: CompanyRow = decode_ok(
        get_admin(&server, &user.id, "/api/companies/by-name?name=case+test+co").await,
    )
    .await;

    assert_eq!(row.id, id);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// POST /api/companies — create
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn create_returns_inserted_row(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin(
        &server,
        &user.id,
        "/api/companies",
        &json!({ "name": "NewCo" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let row: CompanyRow = decode_ok(res).await;

    assert_eq!(row.name, "NewCo");
    assert!(row.active);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_trims_whitespace(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin(
        &server,
        &user.id,
        "/api/companies",
        &json!({ "name": "  Trimmed Co  " }),
    )
    .await;
    let row: CompanyRow = decode_ok(res).await;

    assert_eq!(row.name, "Trimmed Co");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_nulls_optional_fields(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin(
        &server,
        &user.id,
        "/api/companies",
        &json!({ "name": "Minimal" }),
    )
    .await;
    let row: CompanyRow = decode_ok(res).await;

    assert!(row.contact_email.is_none());
    assert!(row.phone.is_none());
    assert!(row.shipping_address.is_none());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_respects_explicit_inactive(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin(
        &server,
        &user.id,
        "/api/companies",
        &json!({ "name": "InactiveFromStart", "active": false }),
    )
    .await;
    let row: CompanyRow = decode_ok(res).await;

    assert!(!row.active);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_rejects_duplicate_name_with_409(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let first = post_admin(
        &server,
        &user.id,
        "/api/companies",
        &json!({ "name": "Dup" }),
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);

    let second = post_admin(
        &server,
        &user.id,
        "/api/companies",
        &json!({ "name": "Dup" }),
    )
    .await;
    assert_eq!(second.status(), StatusCode::CONFLICT);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// PATCH /api/companies/:id — update
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn update_name_only_leaves_other_fields_unchanged(pool: PgPool) {
    let id = seed_company_full(&pool, "Before", None, Some("555-0000"), None).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = patch_admin(
        &server,
        &user.id,
        &format!("/api/companies/{id}"),
        &json!({ "name": "After" }),
    )
    .await;
    let row: CompanyRow = decode_ok(res).await;

    assert_eq!(row.name, "After");
    assert_eq!(row.phone.as_deref(), Some("555-0000"));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_optional_fields_independently(pool: PgPool) {
    let id = seed_company(&pool, "UpdateCo").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = patch_admin(
        &server,
        &user.id,
        &format!("/api/companies/{id}"),
        &json!({
            "contact_email": "new@example.com",
            "phone": "555-9999",
            "shipping_address": "99 New St",
        }),
    )
    .await;
    let row: CompanyRow = decode_ok(res).await;

    assert_eq!(row.contact_email.as_deref(), Some("new@example.com"));
    assert_eq!(row.phone.as_deref(), Some("555-9999"));
    assert_eq!(row.shipping_address.as_deref(), Some("99 New St"));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_trims_whitespace_from_name(pool: PgPool) {
    let id = seed_company(&pool, "Trimmy").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = patch_admin(
        &server,
        &user.id,
        &format!("/api/companies/{id}"),
        &json!({ "name": "  Trimmed  " }),
    )
    .await;
    let row: CompanyRow = decode_ok(res).await;

    assert_eq!(row.name, "Trimmed");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_404_for_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = patch_admin(
        &server,
        &user.id,
        &format!("/api/companies/{BOGUS}"),
        &json!({ "name": "Ghost" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_change_reflected_in_subsequent_read(pool: PgPool) {
    let id = seed_company(&pool, "ReadBackBefore").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    patch_admin(
        &server,
        &user.id,
        &format!("/api/companies/{id}"),
        &json!({ "name": "ReadBackAfter" }),
    )
    .await;

    let row: CompanyRow =
        decode_ok(get_admin(&server, &user.id, &format!("/api/companies/{id}")).await).await;
    assert_eq!(row.name, "ReadBackAfter");

    server.stop().await;
}

// ---------------------------------------------------------------------------
// POST /api/companies/:id/deactivate
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn deactivate_sets_active_false(pool: PgPool) {
    let id = seed_company(&pool, "ToDeactivate").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res =
        post_admin_empty(&server, &user.id, &format!("/api/companies/{id}/deactivate")).await;
    let row: CompanyRow = decode_ok(res).await;

    assert!(!row.active);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn deactivated_is_excluded_from_list(pool: PgPool) {
    let id = seed_company(&pool, "ToDeactivate2").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    post_admin_empty(&server, &user.id, &format!("/api/companies/{id}/deactivate")).await;

    let rows: Vec<CompanyRow> =
        decode_ok(get_admin(&server, &user.id, "/api/companies").await).await;
    assert!(!rows.iter().any(|r| r.id == id));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn deactivated_is_still_retrievable_by_id(pool: PgPool) {
    let id = seed_company(&pool, "ToDeactivate3").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    post_admin_empty(&server, &user.id, &format!("/api/companies/{id}/deactivate")).await;

    let row: CompanyRow =
        decode_ok(get_admin(&server, &user.id, &format!("/api/companies/{id}")).await).await;

    assert!(!row.active);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn deactivate_404_for_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin_empty(
        &server,
        &user.id,
        &format!("/api/companies/{BOGUS}/deactivate"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn deactivate_does_not_affect_other_companies(pool: PgPool) {
    let a = seed_company(&pool, "KeepA").await;
    let b = seed_company(&pool, "DropB").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    post_admin_empty(&server, &user.id, &format!("/api/companies/{b}/deactivate")).await;

    let row: CompanyRow =
        decode_ok(get_admin(&server, &user.id, &format!("/api/companies/{a}")).await).await;
    assert!(row.active);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/companies/:id/users — get_users_by_company
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn users_by_company_empty_when_none(pool: PgPool) {
    let id = seed_company(&pool, "NoUsers").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<UserWithCompanyRow> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/companies/{id}/users")).await,
    )
    .await;
    assert!(rows.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn users_by_company_returns_users_with_company_embedded(pool: PgPool) {
    let id = seed_company(&pool, "UserCo").await;
    seed_user_in_company(&pool, "test-user-1", "user@example.com", id).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<UserWithCompanyRow> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/companies/{id}/users")).await,
    )
    .await;

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].email, "user@example.com");
    let embed = rows[0].company.as_ref().expect("company embed");
    assert_eq!(embed.id, id);
    assert_eq!(embed.name, "UserCo");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn users_by_company_does_not_return_users_from_other_companies(pool: PgPool) {
    let c1 = seed_company(&pool, "C1").await;
    let c2 = seed_company(&pool, "C2").await;
    seed_user_in_company(&pool, "test-user-z", "other@example.com", c2).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<UserWithCompanyRow> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/companies/{c1}/users")).await,
    )
    .await;

    assert!(!rows.iter().any(|u| u.email == "other@example.com"));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn users_by_company_ordered_by_email_ascending(pool: PgPool) {
    let id = seed_company(&pool, "EmailOrderCo").await;
    seed_user_in_company(&pool, "test-user-z", "z@example.com", id).await;
    seed_user_in_company(&pool, "test-user-a", "a@example.com", id).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<UserWithCompanyRow> = decode_ok(
        get_admin(&server, &user.id, &format!("/api/companies/{id}/users")).await,
    )
    .await;
    let emails: Vec<&str> = rows.iter().map(|u| u.email.as_str()).collect();

    let a = emails.iter().position(|e| *e == "a@example.com").unwrap();
    let z = emails.iter().position(|e| *e == "z@example.com").unwrap();
    assert!(a < z);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// Client scoping — Rust-only. TS can't express these (no HTTP boundary).
// Companies have a stricter rule than recipes: clients can see their OWN
// company row only. Everything else is 403; by-id mismatch is 404 ("lie
// about existence" rule).
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_list_companies(pool: PgPool) {
    let own = seed_company(&pool, "OwnCo-list").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(&server, &user.id, own, "/api/companies").await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_use_by_name(pool: PgPool) {
    let own = seed_company(&pool, "OwnCo-byname").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        own,
        "/api/companies/by-name?name=OwnCo-byname",
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_can_read_own_company_by_id(pool: PgPool) {
    let own = seed_company(&pool, "OwnCo-get").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(&server, &user.id, own, &format!("/api/companies/{own}")).await;
    assert_eq!(res.status(), StatusCode::OK);
    let row: CompanyRow = decode_ok(res).await;
    assert_eq!(row.id, own);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_sees_other_company_as_404(pool: PgPool) {
    let own = seed_company(&pool, "OwnCo-404").await;
    let other = seed_company(&pool, "OtherCo-404").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        own,
        &format!("/api/companies/{other}"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_can_read_own_company_users(pool: PgPool) {
    let own = seed_company(&pool, "OwnCo-users").await;
    seed_user_in_company(&pool, "member-1", "member@example.com", own).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        own,
        &format!("/api/companies/{own}/users"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let rows: Vec<UserWithCompanyRow> = decode_ok(res).await;
    assert_eq!(rows.len(), 1);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_sees_other_company_users_as_404(pool: PgPool) {
    let own = seed_company(&pool, "OwnCo-users-404").await;
    let other = seed_company(&pool, "OtherCo-users-404").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        own,
        &format!("/api/companies/{other}/users"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_create_company(pool: PgPool) {
    let own = seed_company(&pool, "OwnCo-create").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_client(
        &server,
        &user.id,
        own,
        "/api/companies",
        &json!({ "name": "ClientTryCreate" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_update_even_own_company(pool: PgPool) {
    let own = seed_company(&pool, "OwnCo-update").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = patch_client(
        &server,
        &user.id,
        own,
        &format!("/api/companies/{own}"),
        &json!({ "name": "Blocked" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_deactivate(pool: PgPool) {
    let own = seed_company(&pool, "OwnCo-deactivate").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_client_empty(
        &server,
        &user.id,
        own,
        &format!("/api/companies/{own}/deactivate"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}
