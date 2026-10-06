//! Parity integration tests for `/api/invites/*`.
//!
//! The TS surface has no existing test suite — we're adding the first
//! one in this port (new `tests/integration/core/invites.test.ts`
//! lands in the same commit). This Rust suite covers the HTTP surface
//! end-to-end, with the Rust-only client-scoping tests (403 on every
//! endpoint) the TS suite can't express.
//!
//! Each test uses `#[sqlx::test(migrations = "../../migrations")]` so
//! every case gets a fresh DB — tests are embarrassingly parallel.
//!
//! Admin-only across the board. The one unauthenticated TS consumer
//! (`routes/invite/[token]/+page.server.ts`) keeps the SQL path in
//! this commit; threading `caller` through that route is a follow-up.

mod common;

use chrono::{DateTime, Duration, Utc};
use common::{companies::seed_company, seed_user, signed_headers, spawn_server, TestServer};
use db::queries::invites::InviteRow;
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Tiny HTTP helpers — same shape as allergens.rs. Reproduced locally so each
// test file is self-contained (ingredients/allergens precedent).
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

async fn decode_ok<T: serde::de::DeserializeOwned>(res: reqwest::Response) -> T {
    let status = res.status();
    let text = res.text().await.expect("body");
    assert!(status.is_success(), "non-2xx: status={status} body={text}");
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("decode: {e}; body={text}"))
}

const BOGUS: &str = "00000000-0000-0000-0000-000000000000";

// ---------------------------------------------------------------------------
// Seed helper — insert an invite directly into the table so read tests
// don't depend on the create endpoint. Mirrors
// `tests/common/companies.rs::seed_company` pattern.
// ---------------------------------------------------------------------------

async fn seed_invite(
    pool: &PgPool,
    company_id: Uuid,
    email: &str,
    token: &str,
    expires_at: DateTime<Utc>,
) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO invite (company_id, email, token, expires_at) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(company_id)
    .bind(email)
    .bind(token)
    .bind(expires_at)
    .fetch_one(pool)
    .await
    .expect("seed invite")
}

fn future_expiry() -> DateTime<Utc> {
    // Can't use `Utc::now()` directly in tests expecting determinism — but
    // the sqlx test harness runs each case against a fresh DB and we only
    // assert `expires_at > created_at` indirectly, so wall-clock is fine.
    Utc::now() + Duration::hours(48)
}

// ---------------------------------------------------------------------------
// GET /api/invites?company_id=…
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn list_returns_empty_when_none_exist(pool: PgPool) {
    let company = seed_company(&pool, "EmptyCo").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<InviteRow> = decode_ok(
        get_admin(
            &server,
            &user.id,
            &format!("/api/invites?company_id={company}"),
        )
        .await,
    )
    .await;
    assert!(rows.is_empty());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_returns_only_rows_for_requested_company(pool: PgPool) {
    let a = seed_company(&pool, "CoA").await;
    let b = seed_company(&pool, "CoB").await;
    seed_invite(&pool, a, "a1@x.com", "tok-a1", future_expiry()).await;
    seed_invite(&pool, a, "a2@x.com", "tok-a2", future_expiry()).await;
    seed_invite(&pool, b, "b1@x.com", "tok-b1", future_expiry()).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<InviteRow> =
        decode_ok(get_admin(&server, &user.id, &format!("/api/invites?company_id={a}")).await)
            .await;
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.company_id == a));

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_orders_rows_by_created_at_desc(pool: PgPool) {
    let c = seed_company(&pool, "OrderCo").await;
    // Three invites, created in order first → second → third. Postgres
    // `now()` is same-txn-stable but monotonic across txns within a test;
    // separate inserts suffice for strict DESC ordering.
    seed_invite(&pool, c, "first@x.com", "tok-first", future_expiry()).await;
    seed_invite(&pool, c, "second@x.com", "tok-second", future_expiry()).await;
    seed_invite(&pool, c, "third@x.com", "tok-third", future_expiry()).await;

    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let rows: Vec<InviteRow> =
        decode_ok(get_admin(&server, &user.id, &format!("/api/invites?company_id={c}")).await)
            .await;
    let emails: Vec<&str> = rows.iter().map(|r| r.email.as_str()).collect();
    assert_eq!(emails, vec!["third@x.com", "second@x.com", "first@x.com"]);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_rejects_missing_company_id_param(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    // Query extractor rejects missing required `company_id` with 400.
    let res = get_admin(&server, &user.id, "/api/invites").await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// GET /api/invites/by-token?token=…
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_token_404_for_missing(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(&server, &user.id, "/api/invites/by-token?token=ghost").await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_token_returns_the_row(pool: PgPool) {
    let c = seed_company(&pool, "TokenCo").await;
    let id = seed_invite(&pool, c, "lookup@x.com", "tok-lookup", future_expiry()).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let row: InviteRow = decode_ok(
        get_admin(&server, &user.id, "/api/invites/by-token?token=tok-lookup").await,
    )
    .await;
    assert_eq!(row.id, id);
    assert_eq!(row.token, "tok-lookup");
    assert_eq!(row.email, "lookup@x.com");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn get_by_token_is_case_sensitive(pool: PgPool) {
    // Token lookup uses `WHERE token = $1`, not case-insensitive — the TS
    // path matches. Mirrors `invites.ts:11`.
    let c = seed_company(&pool, "CaseCo").await;
    seed_invite(&pool, c, "x@x.com", "AbcDef", future_expiry()).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_admin(&server, &user.id, "/api/invites/by-token?token=abcdef").await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// POST /api/invites — create
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn create_returns_inserted_row(pool: PgPool) {
    let c = seed_company(&pool, "CreateCo").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let expires = future_expiry();
    let res = post_admin(
        &server,
        &user.id,
        "/api/invites",
        &json!({
            "company_id": c,
            "email": "new@x.com",
            "token": "tok-create",
            "expires_at": expires,
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let row: InviteRow = decode_ok(res).await;
    assert_eq!(row.company_id, c);
    assert_eq!(row.email, "new@x.com");
    assert_eq!(row.token, "tok-create");
    assert!(row.used_at.is_none());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_lowercases_and_trims_email(pool: PgPool) {
    // Mirrors `invites.ts:36` (`data.email.toLowerCase().trim()`).
    let c = seed_company(&pool, "EmailCo").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin(
        &server,
        &user.id,
        "/api/invites",
        &json!({
            "company_id": c,
            "email": "  MixedCase@EXAMPLE.com  ",
            "token": "tok-email",
            "expires_at": future_expiry(),
        }),
    )
    .await;
    let row: InviteRow = decode_ok(res).await;
    assert_eq!(row.email, "mixedcase@example.com");

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_rejects_duplicate_token_with_409(pool: PgPool) {
    let c = seed_company(&pool, "DupCo").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let body = json!({
        "company_id": c,
        "email": "first@x.com",
        "token": "tok-dup",
        "expires_at": future_expiry(),
    });
    let first = post_admin(&server, &user.id, "/api/invites", &body).await;
    assert_eq!(first.status(), StatusCode::CREATED);

    let dup = json!({
        "company_id": c,
        "email": "second@x.com",
        "token": "tok-dup",
        "expires_at": future_expiry(),
    });
    let second = post_admin(&server, &user.id, "/api/invites", &dup).await;
    assert_eq!(second.status(), StatusCode::CONFLICT);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_rejects_bad_company_id_with_400(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin(
        &server,
        &user.id,
        "/api/invites",
        &json!({
            "company_id": BOGUS,
            "email": "orphan@x.com",
            "token": "tok-orphan",
            "expires_at": future_expiry(),
        }),
    )
    .await;
    // 23503 (FK violation) → 400 Validation per handler mapping.
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_persists_so_appears_in_list(pool: PgPool) {
    let c = seed_company(&pool, "PersistCo").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    post_admin(
        &server,
        &user.id,
        "/api/invites",
        &json!({
            "company_id": c,
            "email": "persist@x.com",
            "token": "tok-persist",
            "expires_at": future_expiry(),
        }),
    )
    .await;

    let rows: Vec<InviteRow> =
        decode_ok(get_admin(&server, &user.id, &format!("/api/invites?company_id={c}")).await)
            .await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].email, "persist@x.com");

    server.stop().await;
}

// ---------------------------------------------------------------------------
// POST /api/invites/:id/mark-used
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn mark_used_stamps_used_at(pool: PgPool) {
    let c = seed_company(&pool, "MarkCo").await;
    let id = seed_invite(&pool, c, "mark@x.com", "tok-mark", future_expiry()).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin_empty(&server, &user.id, &format!("/api/invites/{id}/mark-used")).await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // Confirm via by-token read.
    let row: InviteRow = decode_ok(
        get_admin(&server, &user.id, "/api/invites/by-token?token=tok-mark").await,
    )
    .await;
    assert!(row.used_at.is_some());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn mark_used_404_for_missing_id(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin_empty(
        &server,
        &user.id,
        &format!("/api/invites/{BOGUS}/mark-used"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// POST /api/invites/:id/revoke
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn revoke_stamps_used_at(pool: PgPool) {
    // Same SQL as mark-used — kept as a separate route for TS parity.
    let c = seed_company(&pool, "RevokeCo").await;
    let id = seed_invite(&pool, c, "rev@x.com", "tok-rev", future_expiry()).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin_empty(&server, &user.id, &format!("/api/invites/{id}/revoke")).await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let row: InviteRow = decode_ok(
        get_admin(&server, &user.id, "/api/invites/by-token?token=tok-rev").await,
    )
    .await;
    assert!(row.used_at.is_some());

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn revoke_404_for_missing_id(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_admin_empty(
        &server,
        &user.id,
        &format!("/api/invites/{BOGUS}/revoke"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    server.stop().await;
}

// ---------------------------------------------------------------------------
// Client scoping — Rust-only. Invites are admin-only everywhere; a client
// gets 403 uniformly across every endpoint. The one unauthenticated TS
// consumer (`routes/invite/[token]/+page.server.ts`) keeps the SQL path
// in this commit, so no "lie about existence" case applies here.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_list_invites(pool: PgPool) {
    let c = seed_company(&pool, "ScopeListCo").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        c,
        &format!("/api/invites?company_id={c}"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_get_invite_by_token(pool: PgPool) {
    let c = seed_company(&pool, "ScopeTokCo").await;
    seed_invite(&pool, c, "x@x.com", "tok-scope", future_expiry()).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = get_client(
        &server,
        &user.id,
        c,
        "/api/invites/by-token?token=tok-scope",
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_create_invite(pool: PgPool) {
    let c = seed_company(&pool, "ScopeCreateCo").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_client(
        &server,
        &user.id,
        c,
        "/api/invites",
        &json!({
            "company_id": c,
            "email": "deny@x.com",
            "token": "tok-deny",
            "expires_at": future_expiry(),
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_mark_invite_used(pool: PgPool) {
    let c = seed_company(&pool, "ScopeMarkCo").await;
    let id = seed_invite(&pool, c, "x@x.com", "tok-scope-mark", future_expiry()).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_client_empty(
        &server,
        &user.id,
        c,
        &format!("/api/invites/{id}/mark-used"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_cannot_revoke_invite(pool: PgPool) {
    let c = seed_company(&pool, "ScopeRevCo").await;
    let id = seed_invite(&pool, c, "x@x.com", "tok-scope-rev", future_expiry()).await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let res = post_client_empty(
        &server,
        &user.id,
        c,
        &format!("/api/invites/{id}/revoke"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}
