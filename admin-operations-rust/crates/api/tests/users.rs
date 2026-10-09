//! Parity integration tests for `/api/users/*`.
//!
//! No TS test counterpart — `queries/users.ts` doesn't exist on the TS
//! side (the pre-port state had inline SQL in route handlers). This is
//! the first test suite covering the users HTTP surface.
//!
//! Each test uses `#[sqlx::test(migrations = "../../migrations")]` so
//! every case gets a fresh DB — embarrassingly parallel.
//!
//! Coverage:
//! - Admin reads: hit / miss / cross-company (indistinguishable 404).
//! - `assign_company`: admin unrestricted; client self-stamp when
//!   unaffiliated; client cross-user / already-set → 403.
//! - 404 when the user id doesn't exist.
//! - Client-scoping 403s on reads (admin-only).

mod common;

use common::{
    companies::{seed_company, seed_user_in_company},
    seed_user, signed_headers, spawn_server, TestServer,
};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Local decode shapes — the handler returns `db::queries::users::UserIdRow`
// etc., but the test only cares about the JSON shape, so we decode into
// a small local struct and keep the test file dependency-light.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct IdOnly {
    id: String,
}

#[derive(Debug, Deserialize)]
struct IdEmail {
    id: String,
    email: String,
}

#[derive(Debug, Deserialize)]
struct IdCompany {
    id: String,
    company_id: Option<Uuid>,
}

// ---------------------------------------------------------------------------
// Tiny HTTP helpers — same shape as invites.rs.
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
    company_id: &str,
    path: &str,
) -> reqwest::Response {
    let mut req = Client::new().get(format!("{}{}", server.base_url, path));
    for (k, v) in signed_headers(user_id, "client", company_id) {
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

async fn post_client(
    server: &TestServer,
    user_id: &str,
    company_id: &str,
    path: &str,
    body: &Value,
) -> reqwest::Response {
    let mut req = Client::new()
        .post(format!("{}{}", server.base_url, path))
        .json(body);
    for (k, v) in signed_headers(user_id, "client", company_id) {
        req = req.header(k, v);
    }
    req.send().await.expect("POST client")
}

async fn decode_ok<T: serde::de::DeserializeOwned>(res: reqwest::Response) -> T {
    let status = res.status();
    let text = res.text().await.expect("body");
    assert!(status.is_success(), "non-2xx: status={status} body={text}");
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("decode: {e}; body={text}"))
}

const BOGUS_USER: &str = "ghost-user-id-does-not-exist";
const BOGUS_COMPANY: &str = "00000000-0000-0000-0000-000000000000";

// ---------------------------------------------------------------------------
// GET /api/users/by-email?email=…
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn by_email_hit(pool: PgPool) {
    let c = seed_company(&pool, "ByEmailCo").await;
    let seeded = seed_user_in_company(&pool, "u-byemail", "found@x.com", c).await;
    let admin = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: IdOnly =
        decode_ok(get_admin(&server, &admin.id, "/api/users/by-email?email=found@x.com").await)
            .await;
    assert_eq!(row.id, seeded);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn by_email_miss(pool: PgPool) {
    let admin = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(
        &server,
        &admin.id,
        "/api/users/by-email?email=missing@x.com",
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn by_email_client_forbidden(pool: PgPool) {
    let c = seed_company(&pool, "ClientForbidCo").await;
    seed_user_in_company(&pool, "u-forbid", "hidden@x.com", c).await;
    let server = spawn_server(pool).await;

    // A client (even one of the same company) can't probe the users
    // table — admin-only read.
    let res = get_client(
        &server,
        "u-forbid",
        &c.to_string(),
        "/api/users/by-email?email=hidden@x.com",
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/users/:id?company_id=…
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn by_id_in_company_hit(pool: PgPool) {
    let c = seed_company(&pool, "ByIdCo").await;
    let seeded = seed_user_in_company(&pool, "u-byid", "owned@x.com", c).await;
    let admin = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: IdEmail = decode_ok(
        get_admin(
            &server,
            &admin.id,
            &format!("/api/users/{seeded}?company_id={c}"),
        )
        .await,
    )
    .await;
    assert_eq!(row.id, seeded);
    assert_eq!(row.email, "owned@x.com");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn by_id_in_company_wrong_company(pool: PgPool) {
    // User exists but belongs to company A; admin asks about them with
    // company_id = B. Should be 404 (not 403) — ownership-scoped lookup
    // deliberately leaks nothing about existence.
    let a = seed_company(&pool, "OwningCo").await;
    let b = seed_company(&pool, "QueryingCo").await;
    let seeded = seed_user_in_company(&pool, "u-cross", "cross@x.com", a).await;
    let admin = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(
        &server,
        &admin.id,
        &format!("/api/users/{seeded}?company_id={b}"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn by_id_in_company_client_forbidden(pool: PgPool) {
    let c = seed_company(&pool, "ClientGetCo").await;
    let seeded = seed_user_in_company(&pool, "u-cget", "cget@x.com", c).await;
    let server = spawn_server(pool).await;

    // Even a client calling about THEMSELVES in their OWN company is
    // forbidden — this endpoint is admin-only (used by resetUserPassword).
    let res = get_client(
        &server,
        &seeded,
        &c.to_string(),
        &format!("/api/users/{seeded}?company_id={c}"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// POST /api/users/:id/assign-company
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn assign_company_admin_stamps(pool: PgPool) {
    // Admin takes an unaffiliated user (seeded via `seed_user`, which
    // leaves `company_id` NULL) and stamps a company on them.
    let c = seed_company(&pool, "AdminStampCo").await;
    let target = seed_user(&pool).await;
    let admin = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: IdCompany = decode_ok(
        post_admin(
            &server,
            &admin.id,
            &format!("/api/users/{}/assign-company", target.id),
            &json!({ "company_id": c }),
        )
        .await,
    )
    .await;
    assert_eq!(row.id, target.id);
    assert_eq!(row.company_id, Some(c));

    // Confirm it stuck — admin read-by-id in that company should hit.
    let read: IdEmail = decode_ok(
        get_admin(
            &server,
            &admin.id,
            &format!("/api/users/{}?company_id={c}", target.id),
        )
        .await,
    )
    .await;
    assert_eq!(read.id, target.id);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn assign_company_client_self_null(pool: PgPool) {
    // Invite-acceptance flow: a freshly-created client user with no
    // company_id stamps their own row with the invite's company_id.
    let c = seed_company(&pool, "SelfStampCo").await;
    let target = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    // Caller: role=client, company_id=empty (unaffiliated), user_id=target.
    let row: IdCompany = decode_ok(
        post_client(
            &server,
            &target.id,
            "", // unaffiliated — matches invite-acceptance state
            &format!("/api/users/{}/assign-company", target.id),
            &json!({ "company_id": c }),
        )
        .await,
    )
    .await;
    assert_eq!(row.id, target.id);
    assert_eq!(row.company_id, Some(c));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn assign_company_client_cross_user(pool: PgPool) {
    // Client tries to assign a company to a DIFFERENT user id — 403.
    let c = seed_company(&pool, "CrossUserCo").await;
    let target = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_client(
        &server,
        "some-other-client-id",
        "", // caller is unaffiliated — still 403 because wrong target id
        &format!("/api/users/{}/assign-company", target.id),
        &json!({ "company_id": c }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn assign_company_client_already_set(pool: PgPool) {
    // Client with an existing company_id tries to self-stamp (or
    // re-stamp) — 403. The gate is "unaffiliated only"; a client
    // already in a company can't move themselves anywhere.
    let existing = seed_company(&pool, "ExistingCo").await;
    let target_company = seed_company(&pool, "TargetCo").await;
    let seeded = seed_user_in_company(&pool, "u-already", "already@x.com", existing).await;
    let server = spawn_server(pool).await;

    let res = post_client(
        &server,
        &seeded,
        &existing.to_string(), // already affiliated — this is the gate
        &format!("/api/users/{seeded}/assign-company"),
        &json!({ "company_id": target_company }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn assign_company_missing_user(pool: PgPool) {
    // Admin assigns a company to a user id that doesn't exist — 404.
    let c = seed_company(&pool, "MissingUserCo").await;
    let admin = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin(
        &server,
        &admin.id,
        &format!("/api/users/{BOGUS_USER}/assign-company"),
        &json!({ "company_id": c }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // Keep the unused `BOGUS_COMPANY` suppression quiet — not every
    // test uses it, but we reference it here to document the shape.
    let _ = BOGUS_COMPANY;

    server.stop().await;
}
