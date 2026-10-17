#!/bin/bash
# Bring up the local dev stack.
#
# Two processes:
#   - Postgres  → docker compose up -d db
#   - Rust API  → nix develop + cargo run -p api  (host-side, backgrounded)
#
# SvelteKit stays on the host; run `pnpm dev` in another terminal.
# That lets you separate Vite's logs from the Rust API's logs — the
# stack is three windows: this one (Rust API tail), pnpm dev, and
# your editor.
#
# Why host-side for Rust: the Dockerfile at admin-operations-rust/
# is runtime-only (expects a CI-produced Linux ELF at
# `target/release/api`). On a dev Mac, a local `cargo build` puts a
# Mach-O there and the container crashloops with `exec format error`.
# Running Rust via `nix develop` + `cargo run` uses the exact
# toolchain CI pins, no cross-arch binary ever enters the picture.
#
# Migrations: the Rust API runs `sqlx::migrate!` on its own startup
# (see admin-operations-rust/crates/api/src/main.rs). Nothing to do
# from this script.
#
# Admin login: pnpm run seed:admin runs on every up.sh (idempotent).
# Credentials come from .env:
#   ADMIN_EMAILS / ADMIN_EMAIL   — who to seed as admin
#   ADMIN_INITIAL_PASSWORD       — their initial password (REQUIRED
#                                  for password login; if unset,
#                                  seed:admin creates the user row
#                                  but no credential account, and
#                                  you can't log in).
#
# Flags:
#   --fresh       Nuke the postgres volume before starting (destroys
#                 all local data). Does NOT imply --seed; a fresh db
#                 gets migrations + the admin upsert only.
#   --seed        Run seed:dev after admin seed (destructive wipe +
#                 reseed of app data; prompts before running). Can
#                 combine with --fresh.
#   --no-follow   Don't tail the Rust API log; return the terminal
#                 after the API binds :8080.
#   -h | --help   Show this help.
#
# Examples:
#   ./scripts/up.sh                 # idempotent bring-up, tail Rust log
#   ./scripts/up.sh --fresh         # wipe db, migrate, seed admin
#   ./scripts/up.sh --fresh --seed  # wipe db, migrate, seed admin + dev
#   ./scripts/up.sh --no-follow     # bring up, return terminal

set -euo pipefail

FRESH=false
SEED=false
FOLLOW=true

while [[ $# -gt 0 ]]; do
  case "$1" in
    --fresh)     FRESH=true;  shift ;;
    --seed)      SEED=true;   shift ;;
    --no-follow) FOLLOW=false; shift ;;
    -h|--help)
      sed -n '2,50p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "Unknown flag: $1" >&2
      echo "Usage: $0 [--fresh] [--seed] [--no-follow]" >&2
      exit 1
      ;;
  esac
done

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUST_LOG_PATH="$REPO_ROOT/admin-operations-rust/.rust-api.log"
RUST_PID_PATH="$REPO_ROOT/admin-operations-rust/.rust-api.pid"

# .env is the single source of truth for POSTGRES_* / INTERNAL_AUTH_SECRET
# / ADMIN_*. Export it into this script so docker compose + rust.sh +
# seed:admin all see the same values. Node scripts load .env via
# dotenv themselves, so the export is for the shell + compose only.
if [ -f "$REPO_ROOT/.env" ]; then
  set -a
  # shellcheck disable=SC1091
  . "$REPO_ROOT/.env"
  set +a
fi

if [ "$FRESH" = true ]; then
  echo "⚠️  --fresh: dropping compose stack + postgres volume..."
  docker compose down -v
fi

if [ "$SEED" = true ]; then
  echo "⚠️  --seed will wipe + reseed app data (seed:dev is destructive)."
  read -r -p "    Continue? [y/N] " reply
  case "$reply" in
    y|Y|yes|YES) ;;
    *) echo "Aborted."; exit 1 ;;
  esac
fi

# ── Postgres (compose) ──────────────────────────────────────────────
echo "Starting Postgres..."
docker compose up -d db

echo "Waiting for db to be healthy..."
until [ "$(docker compose ps db --format '{{.Health}}' 2>/dev/null)" = "healthy" ]; do
  sleep 1
done

echo "Seeding admin (idempotent upsert)..."
pnpm run seed:admin

if [ "$SEED" = true ]; then
  echo "Seeding dev data..."
  pnpm run seed:dev
fi

# ── Rust API (host-side, backgrounded) ──────────────────────────────
echo "Starting Rust API via nix develop (first run compiles — can take a few min)..."
"$REPO_ROOT/scripts/rust.sh" --background

# Poll :8080 until the API is reachable. nix+cargo first-run can take
# a while; be patient but bail if the pidfile'd process dies (startup
# error — bad env, port in use, migration failure).
echo "Waiting for Rust API on :8080 (log: $RUST_LOG_PATH)..."
RUST_PID="$(cat "$RUST_PID_PATH")"
tries=0
until curl -sf -o /dev/null "http://localhost:8080/healthz" 2>/dev/null; do
  if ! kill -0 "$RUST_PID" 2>/dev/null; then
    echo ""
    echo "❌ Rust API process died during startup. Last log lines:"
    echo "────────────────────────────────────────────────────────"
    tail -40 "$RUST_LOG_PATH" || true
    echo "────────────────────────────────────────────────────────"
    rm -f "$RUST_PID_PATH"
    exit 1
  fi
  sleep 2
  tries=$((tries + 1))
  if [ $((tries % 15)) -eq 0 ]; then
    echo "  still compiling/starting... (${tries}×2s elapsed; tail $RUST_LOG_PATH for progress)"
  fi
done

echo ""
echo "Local stack up:"
echo "  db       → :5432 (docker compose)"
echo "  rust-api → :8080 (host, pid $RUST_PID)"
echo "  svelte   → run 'pnpm dev' in another terminal (:5173)"
echo ""
echo "Stop everything with: ./scripts/down.sh"
echo ""

if [ "$FOLLOW" = true ]; then
  echo "Tailing Rust API log (Ctrl-C to detach; API keeps running — use down.sh to stop)..."
  tail -f "$RUST_LOG_PATH"
fi
