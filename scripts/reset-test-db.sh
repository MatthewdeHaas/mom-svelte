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

# Load .env.test by hand — `xargs` chokes on values with spaces, and
# `source` runs the file as shell so unquoted `<` becomes a redirect.
# Parse each KEY=value line ourselves and export without re-interpretation.
while IFS='=' read -r key value; do
  # Skip blanks and comments
  [[ -z "$key" || "$key" =~ ^[[:space:]]*# ]] && continue
  # Strip surrounding quotes if present
  value="${value#\"}"; value="${value%\"}"
  value="${value#\'}"; value="${value%\'}"
  export "$key=$value"
done < .env.test

DB_URL="postgresql://${POSTGRES_USER}:${POSTGRES_PASSWORD}@${POSTGRES_HOST}:5432"

echo "[db:reset-test] Dropping and recreating ${POSTGRES_DB}..."
psql "${DB_URL}/postgres" -c "DROP DATABASE IF EXISTS ${POSTGRES_DB};"
psql "${DB_URL}/postgres" -c "CREATE DATABASE ${POSTGRES_DB};"

echo "[db:reset-test] Running migrations..."
pnpm run migrate

echo "[db:reset-test] Done — ${POSTGRES_DB} is ready."
