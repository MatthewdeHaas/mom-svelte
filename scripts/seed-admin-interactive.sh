#!/usr/bin/env bash
#
# seed-admin-interactive.sh — prompt-driven wrapper around `pnpm seed:admin`.
#
# Prompts for an email + password one admin at a time and runs the seed
# script against the `mom-svelte` Railway service, which has Node + the
# TS tooling on disk and the Postgres env vars already wired in.
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
#
# Usage:
#   ./scripts/seed-admin-interactive.sh
#
# Env overrides:
#   RAILWAY_SERVICE   service to run against (default: mom-svelte).

set -euo pipefail

SERVICE="${RAILWAY_SERVICE:-mom-svelte}"

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

echo "Interactive admin seeder — target service: $SERVICE"
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

  echo "Seeding $email via '$SERVICE'..."
  # Pass creds as env vars (not argv) so they don't appear in `ps`.
  # `railway run` injects the service's environment (DATABASE_URL,
  # BETTER_AUTH_SECRET, etc.) into the child shell, so seed-admin.ts
  # connects to the same Postgres the live service uses.
  if ADMIN_EMAIL="$email" ADMIN_INITIAL_PASSWORD="$pw1" \
       railway run --service "$SERVICE" -- \
       bash -c 'ADMIN_EMAIL="$ADMIN_EMAIL" ADMIN_INITIAL_PASSWORD="$ADMIN_INITIAL_PASSWORD" pnpm --silent seed:admin'; then
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
