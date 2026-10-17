#!/bin/bash
# Tear down the local dev stack.
#
# Stops the host-side Rust API (via pidfile written by scripts/rust.sh)
# and the compose Postgres container. Default: preserve the postgres
# volume. Re-running ./scripts/up.sh afterwards brings everything
# back with data intact.
#
# Flags:
#   --volumes   Also remove the postgres volume (destroys all local
#               data). Equivalent to `./scripts/up.sh --fresh` on
#               the next bring-up.
#   -h | --help Show this help.
#
# Examples:
#   ./scripts/down.sh            # stop everything, keep data
#   ./scripts/down.sh --volumes  # stop everything, wipe data

set -euo pipefail

VOLUMES=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --volumes) VOLUMES=true; shift ;;
    -h|--help)
      sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "Unknown flag: $1" >&2
      echo "Usage: $0 [--volumes]" >&2
      exit 1
      ;;
  esac
done

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUST_PID_PATH="$REPO_ROOT/admin-operations-rust/.rust-api.pid"

# ── Rust API (host-side) ────────────────────────────────────────────
if [ -f "$RUST_PID_PATH" ]; then
  RUST_PID="$(cat "$RUST_PID_PATH")"
  if kill -0 "$RUST_PID" 2>/dev/null; then
    echo "Stopping Rust API (pid $RUST_PID)..."
    # SIGTERM first — cargo-run spawns the api binary as a child and
    # cargo forwards signals. 5s grace, then SIGKILL if it's still up.
    kill "$RUST_PID" 2>/dev/null || true
    for _ in 1 2 3 4 5; do
      if ! kill -0 "$RUST_PID" 2>/dev/null; then break; fi
      sleep 1
    done
    kill -9 "$RUST_PID" 2>/dev/null || true
  else
    echo "Rust API pidfile stale (pid $RUST_PID not running) — cleaning up."
  fi
  rm -f "$RUST_PID_PATH"
else
  echo "No Rust API pidfile — assuming it's not running."
fi

# ── Compose (Postgres) ──────────────────────────────────────────────
if [ "$VOLUMES" = true ]; then
  echo "⚠️  --volumes: stopping Postgres and removing volume..."
  docker compose down -v
else
  docker compose down
fi

echo "Stack down."
