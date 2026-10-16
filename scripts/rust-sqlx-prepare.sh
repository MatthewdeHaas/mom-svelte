#!/usr/bin/env bash
# Regenerate the sqlx offline-mode cache under
# `admin-operations-rust/.sqlx/`.
#
# `cargo sqlx prepare` connects to a live Postgres, runs each
# `query!` / `query_as!` call site against the real schema, and
# writes one JSON file per call into `.sqlx/`. CI then builds with
# `SQLX_OFFLINE=1` and no DB — the macros read the cache instead
# of touching Postgres. We commit `.sqlx/` for exactly this reason.
#
# The macro type-checks against the LIVE schema, not migrations on
# disk, so this script also ensures migrations are applied before
# calling `sqlx prepare`. Run after you add/change any query.
#
# Dev loop: edit → pnpm rust:check (fails with "query is stale") →
# pnpm rust:prepare → pnpm rust:check (green) → commit the .sqlx/
# diff alongside the code change.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# The flake declares `pkgs.sqlx-cli`, but a shell started before that
# edit won't have it on PATH. Walk /nix/store for the built derivation
# as a fallback so this script is idempotent across re-shells.
if ! command -v sqlx >/dev/null 2>&1; then
    sqlx_bin="$(ls -d /nix/store/*-sqlx-cli-*/bin/sqlx 2>/dev/null | head -n1 || true)"
    if [[ -n "${sqlx_bin}" ]]; then
        export PATH="$(dirname "${sqlx_bin}"):${PATH}"
    else
        echo "rust-sqlx-prepare: sqlx-cli not found. Re-enter the nix shell (\`nix develop\`) or run \`cargo install sqlx-cli\` into a project-local CARGO_INSTALL_ROOT." >&2
        exit 1
    fi
fi

# Load DATABASE_URL from .env — same shape pnpm migrate uses.
# Parse by hand (not `source`): .env values can contain unquoted `<`,
# which `source` would treat as a redirect and bail. Mirrors the
# parser loop in scripts/reset-test-db.sh.
if [[ -f .env ]]; then
    while IFS='=' read -r key value; do
        [[ -z "$key" || "$key" =~ ^[[:space:]]*# ]] && continue
        value="${value#\"}"; value="${value%\"}"
        value="${value#\'}"; value="${value%\'}"
        # Expand ${VAR} references against already-exported keys — the
        # original `source` path did this for free (DATABASE_URL inlines
        # POSTGRES_USER/PASSWORD/HOST/DB). Hand-roll the expansion so
        # we don't shell out and don't re-interpret unquoted `<` chars.
        while [[ "$value" =~ \$\{([A-Za-z_][A-Za-z0-9_]*)\} ]]; do
            var="${BASH_REMATCH[1]}"
            value="${value//\$\{$var\}/${!var:-}}"
        done
        export "$key=$value"
    done < .env
fi

if [[ -z "${DATABASE_URL:-}" ]]; then
    echo "rust-sqlx-prepare: DATABASE_URL not set. Put it in .env." >&2
    exit 1
fi

# sqlx-cli resolves 'db' in DATABASE_URL via Docker's internal DNS,
# which doesn't exist outside the container. Rewrite 'db' → '127.0.0.1'
# so the host-side sqlx-cli can reach the exposed port. `migrate.ts`
# builds its own URL from POSTGRES_HOST, so rewrite that too.
DATABASE_URL="${DATABASE_URL//@db:/@127.0.0.1:}"
export DATABASE_URL
if [[ "${POSTGRES_HOST:-}" == "db" ]]; then
    export POSTGRES_HOST=127.0.0.1
fi

# Make sure the Rust workspace sees the latest migrations.
bash scripts/sync-rust-migrations.sh

# Ensure the DB is actually up and migrated before probing types.
# The macro checks column names + nullability against the live
# schema, so an un-migrated DB silently produces wrong cache rows.
if ! docker compose ps db --format '{{.Status}}' 2>/dev/null | grep -q 'Up'; then
    echo "rust-sqlx-prepare: starting Docker postgres" >&2
    docker compose up -d db
fi

echo "rust-sqlx-prepare: applying migrations"
# Rust is the sole migrator (see project_migration_architecture).
# Use sqlx-cli directly rather than the api binary — this script's
# whole purpose is to prep the api binary's sqlx offline cache, so
# depending on a compiled binary here would be a bootstrap
# chicken-and-egg. sqlx-cli is PATH-guarded at the top of this file.
(cd admin-operations-rust && sqlx migrate run --source migrations)

echo "rust-sqlx-prepare: regenerating .sqlx/ cache"
# `--workspace` walks every crate; `-- --tests` includes query
# macros that only exist in `#[cfg(test)]` blocks so CI builds of
# the test binary stay offline-clean too.
cd admin-operations-rust
cargo sqlx prepare --workspace -- --tests

echo "rust-sqlx-prepare: done — review and commit .sqlx/ changes"
