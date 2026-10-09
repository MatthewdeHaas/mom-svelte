//! Local flags editor — reads prod state via `GET /api/flags/snapshot`,
//! renders a flag × company grid, writes a migration file on `w`.
//!
//! See `docs/feature-flags.md` for the user-facing invocation + keyboard
//! reference, and `docs/handoff-track7.md` (deleted in commit 3) for the
//! design rationale.

// Modules live in `lib.rs` so integration tests under `tests/` can
// import them. The binary re-uses the same code via the crate's
// library name.
use std::{env, path::PathBuf};

use anyhow::{anyhow, Context, Result};
use clap::Parser;

use flags_tui::{emit, model::Grid, snapshot, ui};

/// `cargo run -p flags-tui -- --help`. Everything has an env-var
/// fallback so the common case is just `cargo run -p flags-tui`.
#[derive(Debug, Parser)]
#[command(
    name = "flags-tui",
    about = "Local editor for feature flags — reads prod, emits migrations."
)]
struct Args {
    /// Base URL of the Rust API (defaults to $RUST_API_URL).
    #[arg(long, env = "RUST_API_URL")]
    snapshot_url: String,

    /// Directory to write the emitted migration into. Defaults to
    /// `<repo>/scripts/migrations/`, resolved relative to this crate.
    #[arg(long)]
    migrations_dir: Option<PathBuf>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Secrets never land on argv — read them from env only. The handoff
    // doc's env-var cheat sheet documents these; both must be set.
    let secret = env::var("INTERNAL_AUTH_SECRET")
        .context("INTERNAL_AUTH_SECRET must be set (same value as the Rust service)")?;
    if secret.len() < 32 {
        return Err(anyhow!(
            "INTERNAL_AUTH_SECRET must be ≥32 bytes ({} provided)",
            secret.len()
        ));
    }
    let user_id = env::var("FLAGS_SNAPSHOT_USER_ID_LOCAL").context(
        "FLAGS_SNAPSHOT_USER_ID_LOCAL must be set — same value as the server's \
         FLAGS_SNAPSHOT_USER_ID",
    )?;

    // Resolve migrations dir. `CARGO_MANIFEST_DIR` is set at build time
    // for `cargo run`, so the default lands at `scripts/migrations/`
    // in the repo root. For released binaries (unused today) the user
    // can `--migrations-dir`.
    let migrations_dir = args.migrations_dir.unwrap_or_else(|| {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // crates/flags-tui -> ../../.. -> repo root / scripts / migrations
        manifest
            .join("..")
            .join("..")
            .join("..")
            .join("scripts")
            .join("migrations")
    });
    if !migrations_dir.is_dir() {
        return Err(anyhow!(
            "migrations dir not found: {} (pass --migrations-dir to override)",
            migrations_dir.display()
        ));
    }

    // Fetch snapshot before entering raw mode — a 403/503 printed to
    // stderr is far more useful than a flash of raw-mode terminal.
    let snap = snapshot::fetch(&args.snapshot_url, &user_id, secret.as_bytes())
        .await
        .context("fetch snapshot")?;
    let grid = Grid::from_snapshot(snap);

    // Hand off to the ratatui loop; it owns the terminal until the
    // user quits or writes.
    let outcome = ui::run(grid)?;

    match outcome {
        ui::Outcome::QuitWithoutWriting => {
            eprintln!("No changes written.");
        }
        ui::Outcome::WriteRequested(grid) => {
            let now = chrono::Utc::now();
            match emit::emit(&grid, &migrations_dir, now, &args.snapshot_url, &user_id)? {
                Some(path) => println!("Wrote migration: {}", path.display()),
                None => println!("No changes — nothing to write."),
            }
        }
    }
    Ok(())
}
