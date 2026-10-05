//! Shared test harness for API integration tests.
//!
//! One place to:
//! - Build an `AppState` against the `#[sqlx::test]` pool.
//! - Spawn the live router on an ephemeral `TcpListener` so tests can
//!   speak real HTTP (same path production traffic takes).
//! - Mint HMAC-signed internal auth headers that match
//!   `crates/api/src/auth.rs` byte-for-byte.
//! - Seed a user + whatever companies/recipes a given fixture references.
//!
//! The seeder parses the xlsx with the `excel` crate's own `parse_workbook`
//! and extracts the Customer/Recipe cell values verbatim — no namespace
//! rewriting. Each `#[sqlx::test]` gets a fresh DB, so collisions are
//! impossible by construction.

#![allow(dead_code)] // helpers may not all be used by every test file

use std::{
    net::SocketAddr,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use api::{router, AppState};
use excel::parse::{parse_workbook, Cell};
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;
use sqlx::PgPool;
use tokio::task::JoinHandle;

type HmacSha256 = Hmac<Sha256>;

// Order-domain seed helpers (companies, recipes, defaulted orders).
// Kept in a sibling module so `tests/orders.rs` can `use common::orders::*`.
pub mod orders;

// Recipe-domain seed helpers (ingredients, allergens, join tables).
pub mod recipes;

// Company-domain seed helpers (companies + user attachments).
pub mod companies;

// Ingredient-domain seed helpers (ingredients + allergen attachments).
pub mod ingredients;

// Allergen-domain seed helpers (allergens + ingredient/person link seeds).
pub mod allergens;

/// Must stay in sync with `crates/api/src/auth.rs:SKEW_TOLERANCE` and
/// the SvelteKit signer — any divergence here produces mysterious 401s.
pub const TEST_SECRET: &[u8] = b"test-only-development-secret-32-bytes-min";

/// A test-side user that satisfies the `orders.created_by` FK onto
/// `"user"(id)`. Seeded once per test; the id is returned so the
/// caller can sign headers with it.
pub struct SeededUser {
    pub id: String,
}

/// Hands back the base URL (`http://127.0.0.1:<port>`) that the test
/// client should POST against, plus the server task so the test can
/// drop it (which triggers `axum::serve`'s graceful shutdown).
pub struct TestServer {
    pub base_url: String,
    pub shutdown: tokio::sync::oneshot::Sender<()>,
    pub handle: JoinHandle<()>,
}

impl TestServer {
    /// Stop the server and wait for the task to join. Call from the
    /// end of a test (or in a drop guard) so a panic doesn't leak a
    /// listening socket into the next test.
    pub async fn stop(self) {
        let _ = self.shutdown.send(());
        let _ = self.handle.await;
    }
}

/// Boot the live router against the given pool on `127.0.0.1:<random port>`.
pub async fn spawn_server(pool: PgPool) -> TestServer {
    let state = AppState::new(pool, Arc::new(TEST_SECRET.to_vec()));
    let app = router(state);

    // Port 0 → kernel assigns a free port. We read it back off the
    // bound listener so parallel `cargo test` runs don't collide.
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local_addr");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("serve");
    });

    TestServer {
        base_url: format!("http://{addr}"),
        shutdown: shutdown_tx,
        handle,
    }
}

/// Current unix seconds — the signer and verifier both work in whole
/// seconds, so we stay on `i64` to avoid any float drift.
fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock >= epoch")
        .as_secs() as i64
}

/// 32-hex-char nonce. Any shorter and the verifier rejects it as
/// "nonce too short".
fn fresh_nonce() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// The 6 internal-auth headers for a request. Returned as a `Vec` so
/// the caller can plug them into any client (reqwest's `HeaderMap` or
/// axum's `HeaderMap`) without pulling in a shared type.
pub fn signed_headers(user_id: &str, role: &str, company_id: &str) -> Vec<(&'static str, String)> {
    let ts = now_secs();
    let nonce = fresh_nonce();
    let payload = format!("{user_id}|{role}|{company_id}|{ts}|{nonce}");

    let mut mac = HmacSha256::new_from_slice(TEST_SECRET).expect("hmac key");
    mac.update(payload.as_bytes());
    let sig = hex::encode(mac.finalize().into_bytes());

    vec![
        ("x-internal-user", user_id.to_string()),
        ("x-internal-role", role.to_string()),
        ("x-internal-company", company_id.to_string()),
        ("x-internal-timestamp", ts.to_string()),
        ("x-internal-nonce", nonce),
        ("x-internal-auth", sig),
    ]
}

/// Seed a user row satisfying `orders.created_by` FK. The TS test uses
/// `role='user'` (a Better Auth-side value, no CHECK constraint); we
/// use `role='admin'` to mirror the production path the importer takes.
pub async fn seed_user(pool: &PgPool) -> SeededUser {
    let id = format!("test-import-user-{}", uuid::Uuid::new_v4());
    sqlx::query(
        r#"INSERT INTO "user" (id, name, email, "emailVerified", role, "createdAt", "updatedAt")
           VALUES ($1, 'Rust Import Test', $2, true, 'admin', now(), now())"#,
    )
    .bind(&id)
    .bind(format!("{id}@test.local"))
    .execute(pool)
    .await
    .expect("seed user");
    SeededUser { id }
}

/// Extracted Customer + Recipe names from a parsed workbook, ready to
/// `seed_companies_and_recipes` with. Returned as `Vec` with no dedup
/// — the seeder uses `ON CONFLICT DO NOTHING` so duplicates are free.
pub struct FixtureNames {
    pub companies: Vec<String>,
    pub recipes: Vec<String>,
}

/// Walk the parsed rows and pull Customer/Recipe cell values. Matches
/// the TS `loadFixtureNamespaced` logic but without the namespace
/// rewrite — we trust `#[sqlx::test]`'s fresh DB to keep tests
/// isolated.
pub fn extract_fixture_names(bytes: &[u8]) -> FixtureNames {
    let wb = parse_workbook(bytes).expect("parse fixture");
    let customer_key = wb
        .headers
        .iter()
        .find(|h| h.trim().eq_ignore_ascii_case("customer"))
        .or_else(|| {
            wb.headers
                .iter()
                .find(|h| h.trim().eq_ignore_ascii_case("company"))
        })
        .cloned();
    let recipe_key = wb
        .headers
        .iter()
        .find(|h| h.trim().eq_ignore_ascii_case("recipe"))
        .cloned();

    let (Some(customer_key), Some(recipe_key)) = (customer_key, recipe_key) else {
        panic!(
            "fixture missing Customer/Recipe header; got: {:?}",
            wb.headers
        );
    };

    // Collect BOTH the raw cell value and the trimmed form.
    //
    // The TS validator doesn't trim (`z.string()` with no `.trim()`),
    // so the Rust importer — mirroring it byte-for-byte — queries
    // with the trailing-space variant verbatim. The TS test gets away
    // with its untrimmed importer only because `loadFixtureNamespaced`
    // rewrites each cell with a trimmed value (see
    // `tests/integration/excel/real-client.test.ts:110`), so the
    // cells reaching the importer are already clean.
    //
    // Rather than reimplement xlsx cell-rewriting on the Rust side,
    // we seed both the raw and trimmed variants of every name. The
    // `UNIQUE (name)` constraint + `ON CONFLICT DO NOTHING` dedupes,
    // and the lookup hits whichever form actually appears in the
    // cell. Harmless when raw == trimmed (one insert + one no-op
    // conflict); corrects the Tuna-Salad trailing-space case
    // (rows 307-310).
    let mut companies = Vec::new();
    let mut recipes = Vec::new();
    for row in &wb.rows {
        if let Some(Cell::String(c)) = row.get(&customer_key) {
            if !c.trim().is_empty() {
                companies.push(c.to_string());
                companies.push(c.trim().to_string());
            }
        }
        if let Some(Cell::String(r)) = row.get(&recipe_key) {
            if !r.trim().is_empty() {
                recipes.push(r.to_string());
                recipes.push(r.trim().to_string());
            }
        }
    }
    FixtureNames { companies, recipes }
}

/// Insert the given company + recipe names into a fresh DB. Both tables
/// have `UNIQUE (name)`, so `ON CONFLICT DO NOTHING` makes this
/// idempotent — handy because the fixture's row list has repeats
/// (12 unique customers over 350 rows).
pub async fn seed_companies_and_recipes(pool: &PgPool, names: &FixtureNames) {
    for c in &names.companies {
        sqlx::query("INSERT INTO company (name) VALUES ($1) ON CONFLICT (name) DO NOTHING")
            .bind(c)
            .execute(pool)
            .await
            .expect("seed company");
    }
    for r in &names.recipes {
        sqlx::query(
            "INSERT INTO recipe (name, active) VALUES ($1, TRUE) \
             ON CONFLICT (name) DO NOTHING",
        )
        .bind(r)
        .execute(pool)
        .await
        .expect("seed recipe");
    }
}
