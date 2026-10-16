#!/bin/bash
# Bring up the local docker-compose stack (db + rust-api).
#
# SvelteKit runs on the host via `pnpm dev` — the former `web`
# compose service was removed when CI switched to host-side Node
# (see docker-compose.yaml). This script no longer manages the
# SvelteKit side; run `pnpm dev` separately in another terminal.
#
# Migrations: the Rust API runs `sqlx::migrate!` on its own startup
# (see admin-operations-rust/crates/api/src/main.rs). Nothing to do
# from this script.
#
# Seed admin / dev data runs host-side after compose is up.

set -e

SEED=false
while getopts "s" opt; do
  case $opt in
    s) SEED=true ;;
    *) echo "Usage: $0 [-s]" >&2; exit 1 ;;
  esac
done

# Rebuild the Rust API image, start/keep db healthy.
docker compose up --build -d

echo "Waiting for db to be healthy..."
until [ "$(docker compose ps db --format '{{.Health}}' 2>/dev/null)" = "healthy" ]; do
  sleep 1
done

echo "Seeding admin..."
pnpm run seed:admin

if [ "$SEED" = true ]; then
  echo "Seeding dev data..."
  pnpm run seed:dev
fi

echo "Local stack up: db on :5432, rust-api on :8080. Run 'pnpm dev' for SvelteKit."
docker compose logs -f rust-api
