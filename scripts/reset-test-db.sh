#!/usr/bin/env bash
# Wipe and rebuild the test database (meals_test).
# Run once on first setup, or any time you want a clean slate.
#
# Usage: pnpm db:reset-test
#   Reads connection details from .env.test.

set -euo pipefail

# Load .env.test
if [ ! -f .env.test ]; then
  echo "error: .env.test not found" >&2
  exit 1
fi

export $(grep -v '^#' .env.test | grep -v '^$' | xargs)

DB_URL="postgresql://${POSTGRES_USER}:${POSTGRES_PASSWORD}@${POSTGRES_HOST}:5432"

echo "[db:reset-test] Dropping and recreating ${POSTGRES_DB}..."
psql "${DB_URL}/postgres" -c "DROP DATABASE IF EXISTS ${POSTGRES_DB};"
psql "${DB_URL}/postgres" -c "CREATE DATABASE ${POSTGRES_DB};"

echo "[db:reset-test] Running migrations..."
pnpm run migrate

echo "[db:reset-test] Done — ${POSTGRES_DB} is ready."
