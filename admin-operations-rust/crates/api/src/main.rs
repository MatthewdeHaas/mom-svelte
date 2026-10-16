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

/// Entry point. Dispatches on the first positional argument:
///
/// - no args → **serve path**. Prod/Railway's `CMD ["/usr/local/bin/api"]`
///   lands here. Runs migrations on startup (idempotent), then listens.
/// - `migrate` → **migration-only path**. Runs the sqlx migrator and
///   exits 0. Used by dev scripts + CI so dev/CI/prod all share one
///   migration codepath — Rust is the sole prod migrator as of the
///   dual-ledger collapse (see `project_migration_architecture`).
/// - anything else → usage error, exit 2.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match std::env::args().nth(1).as_deref() {
        None => serve().await,
        Some("migrate") => migrate_only().await,
        Some(other) => {
            eprintln!(
                "unknown subcommand: {other}\nusage: api               # serve\n       api migrate       # run migrations and exit",
            );
            std::process::exit(2);
        }
    }
}

/// Load config + open the pool. Shared by `serve` and `migrate_only`
/// so `DATABASE_URL`/`INTERNAL_AUTH_SECRET` validation stays in one
/// place and both paths log the same startup diagnostics.
async fn init() -> anyhow::Result<(Config, sqlx::PgPool)> {
    init_tracing();

    let cfg = Config::from_env()?;
    tracing::info!(port = cfg.port, "starting admin-operations-rust");
    // Diagnostic for the HMAC-mismatch failure class documented in
    // `docs/ops.md` — NOT the secret itself, just its length. A length
    // mismatch between the two services is the single most common
    // cause of 401s on `/api/*`.
    tracing::info!(
        internal_auth_secret_bytes = cfg.internal_auth_secret.len(),
        "INTERNAL_AUTH_SECRET loaded"
    );

    let pool = db::connect(&cfg.database_url)
        .await
        .context("open Postgres pool")?;

    Ok((cfg, pool))
}

/// `api migrate` — run migrations and exit. For dev scripts (`pnpm
/// db:reset-test`, `rust-sqlx-prepare.sh`) and the CI `Run migrations`
/// steps that previously called `pnpm run migrate` (the deleted Node
/// migrator). Identical codepath to `serve`'s startup migration, so
/// what dev sees is what prod runs.
async fn migrate_only() -> anyhow::Result<()> {
    let (_, pool) = init().await?;
    db::migrate(&pool).await.context("run migrations")?;
    tracing::info!("migrations complete");
    Ok(())
}

/// Default no-arg path — serve HTTP. Preserves current Railway
/// behaviour: migrate on boot, then listen. The migrate call is
/// idempotent so a prior `api migrate` in CI followed by a `serve` in
/// the same container is safe.
async fn serve() -> anyhow::Result<()> {
    let (cfg, pool) = init().await?;

    // Migrations run on startup. Idempotent — the sqlx migrator skips
    // anything already applied. See `db/src/lib.rs` for why we embed
    // from `./migrations` and sync from `../scripts/migrations` at
    // build time.
    db::migrate(&pool).await.context("run migrations")?;

    let state = AppState::new(pool, cfg.internal_auth_secret.clone())
        .with_flags_snapshot_user_id(cfg.flags_snapshot_user_id.clone());

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
    /// User id authorised to pull the full flag snapshot (TUI
    /// hydration). Optional on startup — `None` means the endpoint
    /// fail-closes with 503 until an operator sets the env var.
    /// Loaded here (not re-read per request) because mutating env in
    /// parallel `#[sqlx::test]` runs is a documented data race.
    flags_snapshot_user_id: Option<String>,
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

        // Trim + treat empty as unset — a blank Railway variable should
        // not accidentally authorise user_id "".
        let flags_snapshot_user_id = std::env::var("FLAGS_SNAPSHOT_USER_ID")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        Ok(Self {
            port,
            database_url,
            internal_auth_secret: Arc::new(secret.into_bytes()),
            flags_snapshot_user_id,
        })
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sqlx::query=warn"));

    // Stdout layer is always on — local dev reads it directly, CI
    // captures it via Playwright's `[WebServer]`-prefixed stream (when
    // the stream survives the `sh -c` spawn, which it sometimes
    // doesn't; hence the file layer below).
    let stdout_layer = tracing_subscriber::fmt::layer().with_target(true);

    // Optional file sink: when `RUST_LOG_FILE` is set, append every
    // event to that path in addition to stdout. CI sets this in the
    // Playwright `webServer.env` block so the full tracing stream
    // survives the native-binary stdio-capture gap; the GHA workflow
    // uploads the file as an artifact on E2E failure. Local dev
    // leaves it unset → behaviour unchanged.
    //
    // `tracing_appender::rolling::never` is a no-rotation file writer;
    // non_blocking wraps it so logging never stalls the request path.
    // The `WorkerGuard` must outlive `main` — `mem::forget` is the
    // documented pattern for the "log for the whole process lifetime"
    // case (see tracing_appender's docs).
    let file_layer = std::env::var("RUST_LOG_FILE").ok().map(|path| {
        let path = std::path::PathBuf::from(path);
        let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
        let name = path
            .file_name()
            .expect("RUST_LOG_FILE must include a filename");
        let appender = tracing_appender::rolling::never(dir, name);
        let (writer, guard) = tracing_appender::non_blocking(appender);
        std::mem::forget(guard);
        tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_ansi(false)
            .with_writer(writer)
    });

    tracing_subscriber::registry()
        .with(filter)
        .with(stdout_layer)
        .with(file_layer)
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
