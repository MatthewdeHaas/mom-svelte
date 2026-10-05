//! Admin-operations Rust API — binary entry point.
//!
//! Everything substantive lives in `lib.rs` (router, state, submodules)
//! so integration tests under `crates/api/tests/` can mount the exact
//! same wiring. This file only loads config, opens the pool, runs
//! migrations, and hands off to [`api::router`].

use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::Context;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use api::{router, AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let cfg = Config::from_env()?;
    tracing::info!(port = cfg.port, "starting admin-operations-rust");

    let pool = db::connect(&cfg.database_url)
        .await
        .context("open Postgres pool")?;

    // Migrations run on startup. Idempotent — the sqlx migrator skips
    // anything already applied by the Node side. See `db/src/lib.rs`
    // for why we embed from `./migrations` and sync from
    // `../scripts/migrations` at build time.
    db::migrate(&pool).await.context("run migrations")?;

    let state = AppState::new(pool, cfg.internal_auth_secret.clone());

    let app = router(state.clone());

    let addr = SocketAddr::from(([0, 0, 0, 0], cfg.port));
    tracing::info!(%addr, "listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/// Env-driven config. Missing required vars = immediate crash with a
/// clear message, not a mysterious 500 at first request.
struct Config {
    port: u16,
    database_url: String,
    internal_auth_secret: Arc<Vec<u8>>,
}

impl Config {
    fn from_env() -> anyhow::Result<Self> {
        let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?;

        let secret = std::env::var("INTERNAL_AUTH_SECRET")
            .context("INTERNAL_AUTH_SECRET must be set (shared with SvelteKit)")?;
        if secret.len() < 32 {
            anyhow::bail!(
                "INTERNAL_AUTH_SECRET must be at least 32 bytes ({} provided)",
                secret.len()
            );
        }

        let port = std::env::var("PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8080);

        Ok(Self {
            port,
            database_url,
            internal_auth_secret: Arc::new(secret.into_bytes()),
        })
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sqlx::query=warn"));
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_target(true))
        .init();
}

/// Graceful shutdown on SIGINT / SIGTERM so Railway deploys don't
/// drop in-flight requests. 10s drain is a reasonable default — long
/// enough for a PDF generation, short enough that a stuck handler
/// doesn't block the platform's own kill timer.
async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c().await.expect("install ctrl-c handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }

    tracing::info!("shutdown signal received; draining");
    tokio::time::sleep(Duration::from_millis(100)).await;
}
