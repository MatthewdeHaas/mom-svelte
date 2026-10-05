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
}

impl AppState {
    pub fn new(pool: PgPool, auth_secret: Arc<Vec<u8>>) -> Self {
        Self {
            flags: Arc::new(PgFlagResolver::new(pool.clone())),
            pool,
            auth_secret,
            nonce_cache: Arc::new(NonceCache::new()),
        }
    }
}
