//! Database access layer: connection pool + migrations runner.
//!
//! Business-logic queries will live under `src/queries/<domain>.rs` in
//! later phases of the Rust migration (one file per TS `queries/*.ts`
//! file). This scaffold wires only the pool and the `migrate!` macro.

use std::time::Duration;

use sqlx::postgres::{PgPool, PgPoolOptions};

pub use sqlx;

/// Business-logic queries, grouped by domain. One module per TS
/// `src/lib/server/queries/<domain>.ts` so a reviewer can hop between
/// the two paths without a map.
pub mod queries;

/// Open a Postgres pool suitable for a web service.
///
/// - `max_connections` matches Node's typical pg pool size (10). Tune
///   via env (`DATABASE_MAX_CONNECTIONS`) once we're under real load.
/// - `acquire_timeout` is intentionally short — a slow pool should
///   surface as 503 at `/healthz` rather than request pile-up.
pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    let max_connections = std::env::var("DATABASE_MAX_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);

    PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(3))
        .connect(database_url)
        .await
}

/// Run embedded migrations.
///
/// The macro reads `./migrations/` at compile time — the directory is
/// populated by `scripts/sync-rust-migrations.sh` locally and by
/// `COPY scripts/migrations/ /app/admin-operations-rust/migrations/`
/// in the Dockerfile. Real copy, not symlink: `sqlx::migrate!`
/// embeds the file bytes at build time, so a symlink that points
/// outside the Docker build context would fail silently in CI.
pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./../../migrations").run(pool).await
}

/// Cheap health-check: `SELECT 1`. Returns `Ok(())` when the pool can
/// hand out a connection and the DB answers inside `timeout`.
pub async fn ping(pool: &PgPool, timeout: Duration) -> Result<(), sqlx::Error> {
    tokio::time::timeout(
        timeout,
        sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(pool),
    )
    .await
    .map_err(|_| sqlx::Error::PoolTimedOut)?
    .map(|_| ())
}
