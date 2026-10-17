#!/bin/bash
# Run the Rust API locally via the nix devShell.
#
# Uses the SAME toolchain the flake pins for CI (`rust-bin.stable.latest`),
# so there is no cargo-version drift between local and CI, and no stale
# `admin-operations-rust/target/release/api` landmine from a prior
# cross-arch build. Reads DATABASE_URL + INTERNAL_AUTH_SECRET + PORT
# from the inherited env (up.sh sources .env before invoking us).
#
# Called by scripts/up.sh — writes its pidfile + log so down.sh can
# stop it cleanly. Also runnable on its own if you want foreground
# logs; just invoke `./scripts/rust.sh` directly.
#
# Flags:
#   --background   Daemonize: redirect stdout/stderr to .rust-api.log,
#                  write .rust-api.pid, exit 0 after the API binds :8080.
#                  Used by up.sh. Default is foreground.
#   -h | --help    Print this header.

set -euo pipefail

BACKGROUND=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    --background) BACKGROUND=true; shift ;;
    -h|--help)
      sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "Unknown flag: $1" >&2
      exit 1
      ;;
  esac
done

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUST_DIR="$REPO_ROOT/admin-operations-rust"
LOG="$RUST_DIR/.rust-api.log"
PID="$RUST_DIR/.rust-api.pid"

# Local-dev DATABASE_URL points at the compose db container's host-mapped
# port (:5432 on localhost), NOT the in-network `db:5432` hostname the
# compose rust-api service used to use. Compose only runs Postgres now.
export DATABASE_URL="${DATABASE_URL:-postgresql://${POSTGRES_USER}:${POSTGRES_PASSWORD}@localhost:5432/${POSTGRES_DB}}"
export PORT="${PORT:-8080}"
export RUST_LOG="${RUST_LOG:-info,sqlx::query=warn}"

if [ -z "${INTERNAL_AUTH_SECRET:-}" ]; then
  echo "rust.sh: INTERNAL_AUTH_SECRET not set (export it in .env)" >&2
  exit 1
fi

CMD=(nix develop "$REPO_ROOT#default" --command bash -c "cd '$RUST_DIR' && cargo run --release -p api")

if [ "$BACKGROUND" = true ]; then
  # Clean any stale pidfile from a prior run that didn't shut down
  # cleanly (crash, kill -9, laptop sleep). An orphan pid would make
  # down.sh think the API is running and send signals into the void.
  if [ -f "$PID" ] && ! kill -0 "$(cat "$PID")" 2>/dev/null; then
    rm -f "$PID"
  fi
  if [ -f "$PID" ]; then
    echo "rust.sh: already running (pid $(cat "$PID")). Run ./scripts/down.sh first."
    exit 1
  fi
  : > "$LOG"
  "${CMD[@]}" >>"$LOG" 2>&1 &
  echo $! > "$PID"
  echo "rust.sh: starting Rust API in background (pid $(cat "$PID"), log: $LOG)"
else
  exec "${CMD[@]}"
fi
