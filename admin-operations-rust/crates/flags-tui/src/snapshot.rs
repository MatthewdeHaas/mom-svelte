//! HTTP client for `GET /api/flags/snapshot`.
//!
//! Mirrors the header-building in `crates/api/src/auth.rs` byte-for-byte.
//! If the HMAC canonical payload or any header name changes in `auth.rs`,
//! this file must change in the same commit — the server will 401
//! otherwise. The 20 lines of overlap aren't worth a shared crate, but
//! treat the two files as one unit of change.

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use rand::RngCore;
use reqwest::Client;
use serde::Deserialize;
use sha2::Sha256;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// A `feature_flag` row (global default + metadata). Shape matches the
/// server's `FlagRow` — see `crates/api/src/flags_handler.rs:113`.
#[derive(Debug, Clone, Deserialize)]
pub struct FlagRow {
    pub name: String,
    /// Carried through so the TUI could show it in a detail pane — not
    /// rendered today, but kept so the deserializer tolerates the
    /// field the server emits.
    #[allow(dead_code)]
    pub description: String,
    pub enabled_global: bool,
    #[allow(dead_code)]
    pub created_at: DateTime<Utc>,
    #[allow(dead_code)]
    pub updated_at: DateTime<Utc>,
}

/// A `feature_flag_company` row (per-company override).
#[derive(Debug, Clone, Deserialize)]
pub struct OverrideRow {
    pub flag_name: String,
    pub company_id: Uuid,
    pub enabled: bool,
    #[allow(dead_code)]
    pub created_at: DateTime<Utc>,
    #[allow(dead_code)]
    pub updated_at: DateTime<Utc>,
}

/// A trimmed `company` row.
#[derive(Debug, Clone, Deserialize)]
pub struct SnapshotCompanyRow {
    pub id: Uuid,
    pub name: String,
    pub active: bool,
}

/// Compile-time flag descriptor echoed from the server's `catalog()`.
#[derive(Debug, Clone, Deserialize)]
pub struct FlagDescriptor {
    pub name: String,
    pub description: String,
    pub default: bool,
}

/// Full payload returned by `GET /api/flags/snapshot`.
#[derive(Debug, Clone, Deserialize)]
pub struct FlagSnapshot {
    pub catalog: Vec<FlagDescriptor>,
    pub flags: Vec<FlagRow>,
    pub overrides: Vec<OverrideRow>,
    pub companies: Vec<SnapshotCompanyRow>,
}

/// Fetch the snapshot. `base_url` is `RUST_API_URL` (no trailing slash
/// required). `user_id` is the operator's id that the server's gate
/// expects (`FLAGS_SNAPSHOT_USER_ID`). `secret` is the shared
/// `INTERNAL_AUTH_SECRET`.
pub async fn fetch(base_url: &str, user_id: &str, secret: &[u8]) -> Result<FlagSnapshot> {
    let url = format!(
        "{}/api/flags/snapshot",
        base_url.trim_end_matches('/'),
    );

    let headers = signed_headers(user_id, secret)?;

    let client = Client::builder()
        .build()
        .context("build reqwest client")?;

    let mut req = client.get(&url);
    for (k, v) in headers {
        req = req.header(k, v);
    }

    let res = req.send().await.context("send snapshot request")?;
    let status = res.status();
    if !status.is_success() {
        let body = res.text().await.unwrap_or_default();
        return Err(anyhow!(
            "snapshot request failed: HTTP {} — {}",
            status,
            body.trim()
        ));
    }

    res.json::<FlagSnapshot>()
        .await
        .context("decode snapshot JSON")
}

/// Build the six `X-Internal-*` headers for a GET /api/flags/snapshot
/// request. Mirrors `common::signed_headers` in
/// `crates/api/tests/common/mod.rs` and the SvelteKit signer in
/// `src/lib/server/api-client.ts`. Admin role, no company scope.
fn signed_headers(user_id: &str, secret: &[u8]) -> Result<Vec<(&'static str, String)>> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("clock before epoch")?
        .as_secs() as i64;

    // 16 random bytes = 32 hex chars — matches the server's
    // `nonce.len() < 32` guard.
    let mut nonce_bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = hex::encode(nonce_bytes);

    // Canonical payload: user|role|company|ts|nonce. The TUI is always
    // an admin with no company scope; any change here must land in
    // `crates/api/src/auth.rs` in the same commit.
    let role = "admin";
    let company = "";
    let payload = format!("{user_id}|{role}|{company}|{ts}|{nonce}");

    let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| anyhow!("bad secret length"))?;
    mac.update(payload.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());

    Ok(vec![
        ("x-internal-user", user_id.to_string()),
        ("x-internal-role", role.to_string()),
        ("x-internal-company", company.to_string()),
        ("x-internal-timestamp", ts.to_string()),
        ("x-internal-nonce", nonce),
        ("x-internal-auth", signature),
    ])
}
