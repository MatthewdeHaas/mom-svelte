//! Integration tests for `GET /api/flags/snapshot` — the TUI
//! hydration endpoint.
//!
//! Three cases, mirroring the handoff spec:
//! 1. HMAC valid + `auth.user_id` matches `state.flags_snapshot_user_id`
//!    → 200 with populated `flags` / `overrides` / `companies` /
//!    `catalog` vectors.
//! 2. HMAC valid but `auth.user_id` ≠ gated id → 403.
//! 3. `state.flags_snapshot_user_id` is `None` (operator never set the
//!    env var on the server) → 503.
//!
//! Each test uses `#[sqlx::test(migrations = "../../migrations")]` so
//! every case starts from a fresh DB with the migration-009 seed rows
//! already present (five flags, `enabled_global = FALSE`).
//!
//! Why no env-mutation: the handler reads the gate from `AppState`
//! (populated once in `main.rs`), not from `std::env::var` per
//! request. The alternative — mutating `FLAGS_SNAPSHOT_USER_ID` in
//! each test — would race across parallel `#[sqlx::test]` runs
//! because `std::env::set_var` is a process-global mutation. See
//! `AppState::with_flags_snapshot_user_id` for the state-based
//! injection point.

mod common;

use common::{
    companies::seed_company, seed_user, signed_headers, spawn_server_with, TestServer,
};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use sqlx::PgPool;

/// Hit `/api/flags/snapshot` with admin HMAC headers signed as `user_id`.
async fn get_snapshot(server: &TestServer, user_id: &str) -> reqwest::Response {
    let mut req = Client::new().get(format!("{}/api/flags/snapshot", server.base_url));
    for (k, v) in signed_headers(user_id, "admin", "") {
        req = req.header(k, v);
    }
    req.send().await.expect("GET /api/flags/snapshot")
}

#[sqlx::test(migrations = "../../migrations")]
async fn snapshot_200_when_user_matches_gate(pool: PgPool) {
    // Seed a user that will act as the operator and a company so the
    // `companies` vector is non-empty — the handler should return
    // every company (active or not), and we assert on count below.
    let operator = seed_user(&pool).await;
    let company_id = seed_company(&pool, "Acme Catering").await;

    // Plant one explicit per-company override so `overrides` is also
    // non-empty — exercises the second SELECT, not just the globals.
    sqlx::query(
        "INSERT INTO feature_flag_company (flag_name, company_id, enabled) \
         VALUES ('insights', $1, TRUE)",
    )
    .bind(company_id)
    .execute(&pool)
    .await
    .expect("seed override");

    let server = spawn_server_with(pool, Some(operator.id.clone())).await;
    let res = get_snapshot(&server, &operator.id).await;
    assert_eq!(res.status(), StatusCode::OK);

    let body: Value = res.json().await.expect("json");

    // Catalog comes from the compile-time flag registry — the five
    // ZSTs in `crates/flags/src/lib.rs`. If a flag is added/removed
    // this assertion will fire; update it alongside the catalog.
    let catalog = body.get("catalog").and_then(Value::as_array).expect("catalog array");
    assert_eq!(catalog.len(), 5, "catalog should list the 5 compile-time flags");

    // Migration 009 seeds 5 globals; migration 010 seeds 3 more
    // (`use_rust_backend_imports`, `_queries`, `_insights`). The
    // handler returns every row ordered by name — just assert at
    // least the catalog's 5 are visible so the test survives new
    // `use_rust_backend_*` additions.
    let flag_rows = body.get("flags").and_then(Value::as_array).expect("flags array");
    assert!(
        flag_rows.len() >= 5,
        "expected ≥5 feature_flag rows, got {}",
        flag_rows.len()
    );

    let overrides = body.get("overrides").and_then(Value::as_array).expect("overrides array");
    assert_eq!(overrides.len(), 1, "the one seeded override");
    assert_eq!(overrides[0]["flag_name"], "insights");
    assert_eq!(overrides[0]["enabled"], true);

    let companies = body.get("companies").and_then(Value::as_array).expect("companies array");
    assert_eq!(companies.len(), 1);
    assert_eq!(companies[0]["name"], "Acme Catering");
    assert_eq!(companies[0]["active"], true);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn snapshot_403_when_user_mismatches_gate(pool: PgPool) {
    // Operator the gate is configured for — never seeded as a real
    // user row because the gate check happens before any DB touch.
    let gated_id = "operator-with-tui-access";

    // A different user actually makes the call. HMAC still verifies
    // (middleware doesn't care whether the user exists in the DB),
    // but the handler-level gate check rejects them.
    let impostor = seed_user(&pool).await;

    let server = spawn_server_with(pool, Some(gated_id.to_string())).await;
    let res = get_snapshot(&server, &impostor.id).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn snapshot_503_when_gate_unset(pool: PgPool) {
    // Mirrors the Railway-side "operator forgot to set
    // FLAGS_SNAPSHOT_USER_ID" scenario — the state slot is `None`
    // and the handler must fail-closed, not inadvertently expose
    // the snapshot to the first caller that comes along.
    let caller = seed_user(&pool).await;

    let server = spawn_server_with(pool, None).await;
    let res = get_snapshot(&server, &caller.id).await;
    assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);

    server.stop().await;
}
