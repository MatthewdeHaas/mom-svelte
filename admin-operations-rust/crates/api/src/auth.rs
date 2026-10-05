//! HMAC-signed internal auth between SvelteKit and this service.
//!
//! Threat model: anyone with network access to the Rust service inside
//! the Railway private network. The private network is not public, but
//! a leaked SvelteKit image or a misconfigured egress rule could
//! surface the service — belt and braces.
//!
//! Protocol (also documented in `docs/internal-auth.md`):
//! ```text
//! X-Internal-User:      <user_id>              (opaque string, Better Auth id)
//! X-Internal-Role:      admin | client
//! X-Internal-Company:   <uuid> | ""            (empty when not scoped)
//! X-Internal-Timestamp: <unix seconds, integer>
//! X-Internal-Nonce:     <>= 16 bytes of hex entropy>
//! X-Internal-Auth:      hex(HMAC-SHA256(secret,
//!                              "user|role|company|ts|nonce"))
//! ```
//!
//! Verifier enforces:
//! 1. All six headers present.
//! 2. `|timestamp - now| <= 60s`.
//! 3. HMAC matches (constant-time compare via `subtle`).
//! 4. Nonce not seen in the last `TTL` seconds — rejects replays
//!    even inside the 60s skew window.
//!
//! On success, a `AuthCtx` is attached to request extensions so
//! handlers can `Extension<AuthCtx>` it out.

use std::{
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use dashmap::DashMap;
use domain::{FlagContext, Role};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::state::AppState;

type HmacSha256 = Hmac<Sha256>;

/// Max allowed clock skew between SvelteKit and Rust clocks. Must be
/// large enough to absorb Railway container start skew and NTP drift,
/// small enough that a captured header is uninteresting by the time
/// an attacker could use it. 60s matches the handoff spec.
const SKEW_TOLERANCE: Duration = Duration::from_secs(60);

/// Nonce cache TTL. Match `SKEW_TOLERANCE` — a nonce can only be
/// replayed inside the window where its signature is still valid.
const NONCE_TTL: Duration = Duration::from_secs(60);

/// Sweep interval for expired nonce entries. Sweeping on every insert
/// is cheap in `DashMap` and avoids a background task.
const SWEEP_EVERY: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------
// Nonce cache
// ---------------------------------------------------------------

/// In-memory nonce cache. Keyed by the full nonce hex string; value
/// is the instant it expires. Sweeping is amortized over inserts.
pub struct NonceCache {
    seen: DashMap<String, Instant>,
    last_sweep: parking_lot_lite::Mutex<Instant>,
}

// Minimal mutex to avoid pulling in `parking_lot` just for one lock.
mod parking_lot_lite {
    use std::sync::Mutex as StdMutex;
    pub struct Mutex<T>(StdMutex<T>);
    impl<T> Mutex<T> {
        pub fn new(v: T) -> Self {
            Self(StdMutex::new(v))
        }
        pub fn lock(&self) -> std::sync::MutexGuard<'_, T> {
            // Poisoning is irrelevant here — the inner value is a
            // timestamp, no invariants to protect.
            self.0.lock().unwrap_or_else(|e| e.into_inner())
        }
    }
}

impl NonceCache {
    pub fn new() -> Self {
        Self {
            seen: DashMap::new(),
            last_sweep: parking_lot_lite::Mutex::new(Instant::now()),
        }
    }

    /// Returns `true` if the nonce is fresh (never seen inside TTL)
    /// and records it. Returns `false` on a duplicate.
    pub fn check_and_insert(&self, nonce: &str) -> bool {
        self.maybe_sweep();

        let now = Instant::now();
        let expiry = now + NONCE_TTL;

        // `entry` is atomic over the map; this is the critical section
        // that keeps two parallel requests with the same nonce from
        // both succeeding.
        use dashmap::mapref::entry::Entry;
        match self.seen.entry(nonce.to_string()) {
            Entry::Occupied(mut occ) => {
                if *occ.get() > now {
                    false // still live — duplicate
                } else {
                    occ.insert(expiry);
                    true
                }
            }
            Entry::Vacant(vac) => {
                vac.insert(expiry);
                true
            }
        }
    }

    fn maybe_sweep(&self) {
        let now = Instant::now();
        let mut last = self.last_sweep.lock();
        if now.duration_since(*last) < SWEEP_EVERY {
            return;
        }
        *last = now;
        drop(last);

        self.seen.retain(|_, expiry| *expiry > now);
    }

    #[cfg(test)]
    #[allow(dead_code)]
    fn len(&self) -> usize {
        self.seen.len()
    }
}

impl Default for NonceCache {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------
// Auth context — attached to request extensions on success
// ---------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AuthCtx {
    pub user_id: String,
    pub role: Role,
    pub company_id: Option<Uuid>,
}

impl AuthCtx {
    pub fn to_flag_context(&self) -> FlagContext {
        FlagContext {
            user_id: self.user_id.clone(),
            role: self.role,
            company_id: self.company_id,
        }
    }
}

// ---------------------------------------------------------------
// Middleware
// ---------------------------------------------------------------

/// Axum middleware applied to the `/api/*` subtree.
pub async fn verify_internal_auth(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    match verify(state.auth_secret.as_ref(), &state.nonce_cache, req.headers()) {
        Ok(ctx) => {
            req.extensions_mut().insert(ctx);
            next.run(req).await
        }
        Err(reason) => {
            tracing::warn!(reason = %reason, "internal auth rejected");
            (StatusCode::UNAUTHORIZED, "unauthorized").into_response()
        }
    }
}

/// Core verifier, pulled out so tests can call it without an axum
/// request. Returns the resolved `AuthCtx` or a short reason string.
fn verify(
    secret: &[u8],
    nonces: &NonceCache,
    headers: &HeaderMap,
) -> Result<AuthCtx, &'static str> {
    let user = header_str(headers, "x-internal-user")?;
    let role_raw = header_str(headers, "x-internal-role")?;
    let company_raw = header_str(headers, "x-internal-company")?;
    let ts_raw = header_str(headers, "x-internal-timestamp")?;
    let nonce = header_str(headers, "x-internal-nonce")?;
    let signature = header_str(headers, "x-internal-auth")?;

    // Nonces short enough to be guessable aren't nonces. 16 raw bytes
    // = 32 hex chars.
    if nonce.len() < 32 {
        return Err("nonce too short");
    }

    let ts: i64 = ts_raw.parse().map_err(|_| "timestamp not an integer")?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "clock before epoch")?
        .as_secs() as i64;
    if (now - ts).unsigned_abs() > SKEW_TOLERANCE.as_secs() {
        return Err("timestamp outside skew window");
    }

    // Canonical signed payload — pipe-delimited, same order and
    // encoding as the SvelteKit signer. Any change here must land in
    // both places simultaneously or every request will 401.
    let payload = format!("{user}|{role_raw}|{company_raw}|{ts_raw}|{nonce}");

    let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| "bad secret")?;
    mac.update(payload.as_bytes());
    let expected = mac.finalize().into_bytes();

    let provided = hex::decode(signature).map_err(|_| "signature not hex")?;
    if provided.ct_eq(&expected).unwrap_u8() != 1 {
        return Err("signature mismatch");
    }

    // HMAC is good → check replay. If we did this *before* HMAC we'd
    // happily cache nonces for attackers; order matters.
    if !nonces.check_and_insert(nonce) {
        return Err("nonce replay");
    }

    let company_id = if company_raw.is_empty() {
        None
    } else {
        Some(Uuid::parse_str(company_raw).map_err(|_| "company not a uuid")?)
    };

    Ok(AuthCtx {
        user_id: user.to_string(),
        role: Role::from_header(role_raw),
        company_id,
    })
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str, &'static str> {
    headers
        .get(name)
        .ok_or("missing header")?
        .to_str()
        .map_err(|_| "non-ascii header")
}

// Keep clippy happy with Arc<T> parameter forms used by callers.
#[allow(dead_code)]
fn _assert_send_sync() {
    fn is_send_sync<T: Send + Sync>() {}
    is_send_sync::<Arc<NonceCache>>();
    is_send_sync::<AuthCtx>();
}

// ---------------------------------------------------------------
// Tests
// ---------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    const SECRET: &[u8] = b"a-very-long-development-only-secret-32b";

    fn sign(user: &str, role: &str, company: &str, ts: i64, nonce: &str) -> String {
        let payload = format!("{user}|{role}|{company}|{ts}|{nonce}");
        let mut mac = HmacSha256::new_from_slice(SECRET).unwrap();
        mac.update(payload.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    fn headers(user: &str, role: &str, company: &str, ts: i64, nonce: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("x-internal-user", HeaderValue::from_str(user).unwrap());
        h.insert("x-internal-role", HeaderValue::from_str(role).unwrap());
        h.insert("x-internal-company", HeaderValue::from_str(company).unwrap());
        h.insert(
            "x-internal-timestamp",
            HeaderValue::from_str(&ts.to_string()).unwrap(),
        );
        h.insert("x-internal-nonce", HeaderValue::from_str(nonce).unwrap());
        let sig = sign(user, role, company, ts, nonce);
        h.insert("x-internal-auth", HeaderValue::from_str(&sig).unwrap());
        h
    }

    fn now_secs() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    #[test]
    fn accepts_valid_request() {
        let nonces = NonceCache::new();
        let h = headers("u1", "admin", "", now_secs(), &"a".repeat(32));
        let ctx = verify(SECRET, &nonces, &h).expect("valid");
        assert_eq!(ctx.user_id, "u1");
        assert_eq!(ctx.role, Role::Admin);
        assert_eq!(ctx.company_id, None);
    }

    #[test]
    fn rejects_tampered_payload() {
        let nonces = NonceCache::new();
        let mut h = headers("u1", "admin", "", now_secs(), &"a".repeat(32));
        h.insert("x-internal-user", HeaderValue::from_static("someone-else"));
        verify(SECRET, &nonces, &h).unwrap_err();
    }

    #[test]
    fn rejects_old_timestamp() {
        let nonces = NonceCache::new();
        let h = headers("u1", "admin", "", now_secs() - 120, &"a".repeat(32));
        assert_eq!(
            verify(SECRET, &nonces, &h).unwrap_err(),
            "timestamp outside skew window"
        );
    }

    #[test]
    fn rejects_replay() {
        let nonces = NonceCache::new();
        let nonce = "b".repeat(32);
        let h = headers("u1", "admin", "", now_secs(), &nonce);
        verify(SECRET, &nonces, &h).expect("first ok");
        assert_eq!(verify(SECRET, &nonces, &h).unwrap_err(), "nonce replay");
    }

    #[test]
    fn parses_company_uuid() {
        let nonces = NonceCache::new();
        let company = "11111111-2222-3333-4444-555555555555";
        let h = headers("u1", "client", company, now_secs(), &"c".repeat(32));
        let ctx = verify(SECRET, &nonces, &h).unwrap();
        assert_eq!(ctx.company_id.unwrap().to_string(), company);
        assert_eq!(ctx.role, Role::Client);
    }

    #[test]
    fn rejects_missing_header() {
        let nonces = NonceCache::new();
        let mut h = headers("u1", "admin", "", now_secs(), &"d".repeat(32));
        h.remove("x-internal-nonce");
        verify(SECRET, &nonces, &h).unwrap_err();
    }
}
