//! Shared app state passed through axum's `State` extractor.

use std::sync::Arc;

use flags::PgFlagResolver;
use sqlx::PgPool;

use crate::auth::NonceCache;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub flags: Arc<PgFlagResolver>,
    pub auth_secret: Arc<Vec<u8>>,
    pub nonce_cache: Arc<NonceCache>,
    /// User id authorised to call `GET /api/flags/snapshot` (the TUI
    /// hydration endpoint). Loaded from `FLAGS_SNAPSHOT_USER_ID` in
    /// `main.rs` at startup — `None` here means the env var was unset
    /// and the handler must 503 (fail-closed).
    ///
    /// Carried on `AppState` rather than re-read from env per request
    /// because tests mutate this and `std::env::set_var` across
    /// parallel `#[sqlx::test]` runs is a documented data race.
    pub flags_snapshot_user_id: Option<Arc<String>>,
}

impl AppState {
    pub fn new(pool: PgPool, auth_secret: Arc<Vec<u8>>) -> Self {
        Self {
            flags: Arc::new(PgFlagResolver::new(pool.clone())),
            pool,
            auth_secret,
            nonce_cache: Arc::new(NonceCache::new()),
            flags_snapshot_user_id: None,
        }
    }

    /// Builder-style setter — used by `main.rs` after reading the env
    /// var. Tests build `AppState::new(..)` and pass `Some(id.into())`
    /// via this method, which keeps the common case terse.
    pub fn with_flags_snapshot_user_id(mut self, user_id: Option<String>) -> Self {
        self.flags_snapshot_user_id = user_id.map(Arc::new);
        self
    }
}
