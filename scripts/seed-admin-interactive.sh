#!/usr/bin/env bash
#
# seed-admin-interactive.sh — prompt-driven wrapper around `pnpm seed:admin`.
#
# Prompts for an email + password one admin at a time and runs the seed
# script LOCALLY (your Node + the repo's tsx) against the production
# Postgres through Railway's TCP proxy (DATABASE_PUBLIC_URL).
#
# Why `railway run --service Postgres` and not --service mom-svelte:
#   The SvelteKit service's DATABASE_URL points at
#   `postgres.railway.internal:5432` — a DNS zone that only resolves
#   inside Railway's VPC. Running the TS script on your laptop with
#   that env fails with ENOTFOUND. The `Postgres` service exposes
#   DATABASE_PUBLIC_URL, which resolves anywhere and tunnels through
#   Railway's TCP proxy to the same database.
#
#   The script reads POSTGRES_* and DATABASE_URL from env (see
#   scripts/seed-admin.ts:5-15) — this wrapper remaps
#   DATABASE_PUBLIC_URL → DATABASE_URL for the child process so no
#   script change is needed.
#
# Why wrap `pnpm seed:admin` instead of writing SQL directly:
#   `scripts/seed-admin.ts` hashes the password via Better Auth's
#   configured `ctx.password.hash` (same hasher Better Auth uses at
#   login time) and writes the credential row via
#   `internalAdapter.createAccount` / `updatePassword`. A raw
#   `INSERT INTO account` would bypass that hashing and the user
#   wouldn't be able to log in. Wrapping the canonical script keeps
#   the hashing path identical to what prod uses.
#
# Who can run this:
#   Only someone with your Railway account login. `railway run`
#   requires `railway login` (email + magic-link), and the proxy
#   URL embeds the Postgres credentials — which only Railway's API
#   releases to an authenticated session. The URL never gets echoed
#   to the terminal or logged anywhere.
#
# Idempotency:
#   `seed-admin.ts` upserts the user row (`ON CONFLICT (email) DO
#   UPDATE`) and either creates or `updatePassword`s the credential
#   account. Re-running with the same email + a new password rotates
#   the password; re-running with the same email + same password is
#   a no-op (modulo hash salt).
#
# Password handling:
#   - Password prompt uses `read -s` so the input is NOT echoed to the
#     terminal and does NOT land in shell history.
#   - The password is passed to the child process via env var
#     (ADMIN_INITIAL_PASSWORD), never as an argv — so it doesn't show
#     in `ps`.
#   - We never print the password back to the terminal, log file, or
#     anywhere else.
#   - Same for the DB URL — redirected into the child's env, never
#     printed or persisted.
#
# Usage:
#   ./scripts/seed-admin-interactive.sh
#
# Env overrides:
#   RAILWAY_DB_SERVICE   Postgres service name on Railway (default: Postgres).

set -euo pipefail

DB_SERVICE="${RAILWAY_DB_SERVICE:-Postgres}"

# Hard dependency check: the Railway CLI must be installed and the
# repo must be linked (`railway link`) to a project. Fail fast rather
# than letting the first `railway run` cough up a cryptic error.
if ! command -v railway >/dev/null 2>&1; then
  echo "error: railway CLI not found on PATH" >&2
  echo "  install: https://docs.railway.com/guides/cli" >&2
  exit 1
fi

if ! railway status >/dev/null 2>&1; then
  echo "error: this directory is not linked to a Railway project" >&2
  echo "  fix: run 'railway link' from the repo root first" >&2
  exit 1
fi

# pnpm is needed locally — the TS script runs via `tsx` which is in
# devDependencies. Fail fast with a clear message rather than a
# cryptic "pnpm: command not found" from the child shell.
if ! command -v pnpm >/dev/null 2>&1; then
  echo "error: pnpm not found on PATH" >&2
  echo "  install: npm install -g pnpm@10" >&2
  exit 1
fi

echo "Interactive admin seeder — connecting to Postgres via Railway proxy ($DB_SERVICE)"
echo "Ctrl-C or empty email to quit."
echo

while true; do
  echo "---"
  # -r so backslashes aren't munged; -p for the prompt.
  read -rp "Admin email (empty to quit): " email
  if [ -z "$email" ]; then
    echo "No email entered — exiting."
    break
  fi

  # Minimal sanity check: contains an @. Not a full RFC-5322 validator;
  # Better Auth will reject malformed addresses downstream anyway.
  if [[ "$email" != *@* ]]; then
    echo "  '$email' doesn't look like an email — try again."
    continue
  fi

  # -s hides the input. Prompt twice; bail on mismatch rather than
  # silently seeding a typo'd password.
  read -rsp "Password for $email: " pw1
  echo
  read -rsp "Confirm password:     " pw2
  echo
  if [ -z "$pw1" ]; then
    echo "  Empty password — skipping."
    unset pw1 pw2
    continue
  fi
  if [ "$pw1" != "$pw2" ]; then
    echo "  Passwords did not match — skipping."
    unset pw1 pw2
    continue
  fi

  echo "Seeding $email via Postgres proxy..."
  # Pass creds as env vars (not argv) so they don't appear in `ps`.
  # `railway run --service Postgres` injects the Postgres service's own
  # env, which includes DATABASE_PUBLIC_URL (reachable from this laptop
  # through Railway's TCP proxy) and also the private DATABASE_URL
  # (`postgres.railway.internal:...`, unreachable from outside Railway).
  #
  # Two pools get created in this process:
  #   1. scripts/seed-admin.ts's own pg Pool — reads DATABASE_URL.
  #   2. src/lib/server/db/index.ts (loaded via auth.ts) — builds its
  #      connection string from POSTGRES_HOST/USER/PASSWORD/DB and
  #      IGNORES DATABASE_URL entirely.
  #
  # Both pools must be redirected to the public proxy, so we parse
  # DATABASE_PUBLIC_URL inside the child shell and set every variable
  # either pool reads:
  #   DATABASE_URL              ← PUBLIC URL             (pool #1)
  #   POSTGRES_HOST/USER/...    ← parsed from PUBLIC URL (pool #2)
  #   BETTER_AUTH_URL           ← harmless placeholder   (silences warn)
  #
  # The internal DATABASE_URL stays set in env but is never read.
  if ADMIN_EMAIL="$email" ADMIN_INITIAL_PASSWORD="$pw1" \
       railway run --service "$DB_SERVICE" -- \
       bash -c '
         set -euo pipefail
         if [ -z "${DATABASE_PUBLIC_URL:-}" ]; then
           echo "error: Postgres service does not expose DATABASE_PUBLIC_URL" >&2
           echo "  check the service in the Railway dashboard: Networking → public TCP proxy must be ON" >&2
           exit 1
         fi

         # Parse the proxy URL into components. Node resolves this one-
         # liner safely: WHATWG URL constructor rejects anything that is
         # not a valid absolute URL. Values flow back as a KEY=VALUE
         # block that we eval into the current shell.
         #
         # Why not shell regex: the password may contain any URL-safe
         # character including %-encoded ones; letting Node unescape
         # avoids a class of parsing bugs.
         creds=$(node -e "
           const u = new URL(process.env.DATABASE_PUBLIC_URL);
           const q = s => \"\x27\" + s.replace(/\x27/g, \"\x27\\\\\x27\x27\") + \"\x27\";
           process.stdout.write(
             \"PGHOST=\"     + q(u.hostname)                 + \"\n\" +
             \"PGPORT=\"     + q(u.port || \"5432\")         + \"\n\" +
             \"PGUSER=\"     + q(decodeURIComponent(u.username)) + \"\n\" +
             \"PGPASSWORD=\" + q(decodeURIComponent(u.password)) + \"\n\" +
             \"PGDATABASE=\" + q(u.pathname.replace(/^\\//, \"\"))
           );
         ")
         eval "$creds"

         DATABASE_URL="$DATABASE_PUBLIC_URL" \
           POSTGRES_HOST="$PGHOST" \
           POSTGRES_PORT="$PGPORT" \
           POSTGRES_USER="$PGUSER" \
           POSTGRES_PASSWORD="$PGPASSWORD" \
           POSTGRES_DB="$PGDATABASE" \
           BETTER_AUTH_URL="${BETTER_AUTH_URL:-http://localhost}" \
           BETTER_AUTH_SECRET="${BETTER_AUTH_SECRET:-seed-admin-placeholder-not-used-at-login}" \
           ADMIN_EMAIL="$ADMIN_EMAIL" \
           ADMIN_INITIAL_PASSWORD="$ADMIN_INITIAL_PASSWORD" \
           pnpm --silent seed:admin
       '; then
    echo "  ✓ $email seeded."
  else
    echo "  ✗ seed failed for $email (see output above)." >&2
  fi

  # Scrub password vars from this shell so a later `env` dump or
  # accidental echo can't surface them.
  unset pw1 pw2

  read -rp "Seed another? [y/N] " again
  case "$again" in
    y|Y|yes|YES) continue ;;
    *) echo "Done."; break ;;
  esac
done
