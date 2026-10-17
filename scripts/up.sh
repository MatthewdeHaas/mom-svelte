#!/bin/bash
# Bring up the local docker-compose stack (db + rust-api).
#
# Idempotent by default: re-running does not touch data. SvelteKit
# runs on the host via `pnpm dev` — this script does not manage it.
# Run `pnpm dev` separately in another terminal.
#
# Migrations: the Rust API runs `sqlx::migrate!` on its own startup
# (see admin-operations-rust/crates/api/src/main.rs). Nothing to do
# from this script.
#
# Flags:
#   --fresh       Nuke the postgres volume before starting (destroys
#                 all local data). Does NOT imply --seed; a fresh db
#                 gets migrations + the admin upsert only.
#   --seed        Run seed:dev after admin seed (destructive wipe +
#                 reseed of app data; prompts before running). Can
#                 combine with --fresh.
#   --no-follow   Skip tailing rust-api logs at the end. Useful when
#                 scripting or when you want the terminal back.
#   -h | --help   Show this help.
#
# Examples:
#   ./scripts/up.sh                 # idempotent bring-up, tail logs
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
      sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "Unknown flag: $1" >&2
      echo "Usage: $0 [--fresh] [--seed] [--no-follow]" >&2
      exit 1
      ;;
  esac
done

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

# Rebuild the Rust API image, start/keep db healthy. `up -d` is
# idempotent — containers already running are left alone unless the
# image changed.
docker compose up --build -d

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

echo ""
echo "Local stack up:"
echo "  db       → :5432"
echo "  rust-api → :8080"
echo "  svelte   → run 'pnpm dev' in another terminal (:5173)"
echo ""

if [ "$FOLLOW" = true ]; then
  echo "Tailing rust-api logs (Ctrl-C to detach; stack keeps running)..."
  docker compose logs -f rust-api
fi
