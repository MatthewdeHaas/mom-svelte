#!/usr/bin/env bash
# Copy scripts/migrations/*.sql into admin-operations-rust/migrations/.
#
# Why a copy, not a symlink: `sqlx::migrate!` embeds the SQL at compile
# time by reading from a path relative to the crate's `CARGO_MANIFEST_DIR`.
# A symlink pointing outside the Docker build context breaks silently in
# CI — the macro sees an empty directory and emits a migrator with zero
# migrations. A real copy is boring and works everywhere.
#
# `admin-operations-rust/migrations/` is gitignored; this script is the
# only writer locally, and the Dockerfile does the same copy at build
# time. Source of truth stays `scripts/migrations/`.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
src="$repo_root/scripts/migrations"
dst="$repo_root/admin-operations-rust/migrations"

if [[ ! -d "$src" ]]; then
    echo "sync-rust-migrations: no $src — nothing to do" >&2
    exit 1
fi

mkdir -p "$dst"

# `rsync --delete` keeps the destination exactly in sync — removing a
# migration file in the source removes it here too. Use rsync if we
# have it (universal on macOS/Linux); fall back to a scripted copy for
# minimal containers.
if command -v rsync >/dev/null 2>&1; then
    rsync -a --delete --include='*.sql' --include='*/' --exclude='*' \
        "$src/" "$dst/"
else
    find "$dst" -maxdepth 1 -type f -name '*.sql' -delete
    find "$src" -maxdepth 1 -type f -name '*.sql' -exec cp {} "$dst/" \;
fi

count=$(find "$dst" -maxdepth 1 -type f -name '*.sql' | wc -l | tr -d ' ')
echo "sync-rust-migrations: $count migration(s) staged at $dst"
