# Handoff — Track 5a: boot Rust API in Playwright

Fresh-session setup doc. Picks up where `a4ef0cd6` left off.

## State at handoff

- **Branch:** `main` at `a4ef0cd6`, pushed to `origin/main`.
- **Last three commits:**
  - `a4ef0cd6` — chore(rust): clear workspace clippy warnings and run cargo fmt
  - `7ea49843` — fix(ui): use bits-ui child snippet in admin mobile sheet trigger
  - `f8bf2716` — refactor: port xlsx + pdf exports to Rust, delete TS fall-through
- **Green gates:** `cargo test --workspace` 524/524, `cargo clippy --workspace -- -D warnings` 0, `cargo fmt --check` 0, `pnpm check` 0/0/0, `pnpm test` 7/7.
- **Red gate:** `pnpm test:e2e` exits 1. Admin + client-admin specs 500 at SSR because every `+page.server.ts` calls `internalFetch(...)` → `RUST_API_URL is not set`.

## Standing user directives (verbatim — do NOT deviate)

- *"I would like everything to be in Rust. If it requires business logic or data layer interaction, it should be in Rust. Typescript should be for client side interaction and handling http interactions, and should not have any logic at all."*
- *"Please don't say nix develop. Just run cargo ... as it is on the repo's path with the flake."*
- *"no cleartext stored anywhere"*
- Commits stay local until the user explicitly asks to push. Never push without the ask.

## Flake / cargo invocation

The repo's flake populates `.direnv/`. Shell PATH already has `cargo` via direnv when the user runs commands. From Claude, use:

```sh
bash -c '. .direnv/flake-profile-*.rc && cd admin-operations-rust && cargo ...'
```

For the Rust integration tests, export `DATABASE_URL` before cargo:

```sh
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/meals_test
```

Postgres runs via `docker compose up -d db` — never raw `postgres`/`pg_ctl` (see memory `feedback_use_docker_for_postgres.md`).

## Track 5a — scope

**Goal:** `pnpm test:e2e` exit 0 against the real two-service topology (SvelteKit on 5174 + Rust API on 8081), matching production shape.

### Root cause

`playwright.config.ts` boots only the SvelteKit dev server. SvelteKit's `+page.server.ts` files call `internalFetch(path, caller)` which reads `env.RUST_API_URL`. Unset → `throw new Error('RUST_API_URL is not set — Rust service unavailable')` → SvelteKit 500 → every admin spec fails.

### What needs to happen

1. **Second `webServer` entry** in `playwright.config.ts` that boots the Rust API. Playwright supports an array — both start in parallel, both must become ready before specs run.

2. **Pre-build the Rust binary** so the webServer start isn't a cold compile. Add a `globalSetup` step (or extend the existing `tests/e2e/global-setup.ts`) that runs `cargo build --release -p api` once. First run: ~2 min; subsequent runs: instant (incremental).

3. **Shared `INTERNAL_AUTH_SECRET`** between the two webServers. Must be ≥32 bytes (Rust enforces this). Pick a stable test value like `e2e-internal-auth-secret-at-least-32-bytes-long` and pass to both.

4. **`RUST_API_URL`** set in the SvelteKit webServer env to `http://localhost:8081`.

5. **`DATABASE_URL`** set in the Rust webServer env to the same `meals_test` DB SvelteKit uses, so startup migrations are a no-op (TS side has already applied them via `pnpm run migrate` in `global-setup.ts`).

### Shape to aim for

```ts
// playwright.config.ts
const INTERNAL_AUTH_SECRET = process.env.INTERNAL_AUTH_SECRET
  ?? 'e2e-internal-auth-secret-at-least-32-bytes-long';
const RUST_API_URL = 'http://localhost:8081';

export default defineConfig({
  // ...existing...
  webServer: [
    {
      command: './admin-operations-rust/target/release/api',
      url: `${RUST_API_URL}/healthz`,
      reuseExistingServer: !process.env.CI,
      timeout: 30_000,
      env: {
        PORT: '8081',
        DATABASE_URL: `postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@${POSTGRES_HOST}:5432/${POSTGRES_DB}`,
        INTERNAL_AUTH_SECRET,
        RUST_LOG: 'warn',
      },
    },
    {
      command: 'pnpm run dev -- --port 5174',
      url: BASE_URL,
      reuseExistingServer: !process.env.CI,
      timeout: 60_000,
      env: {
        // ...existing env...
        RUST_API_URL,
        INTERNAL_AUTH_SECRET,
      },
    },
  ],
});
```

Then in `tests/e2e/global-setup.ts` add before any DB work:

```ts
import { execSync } from 'node:child_process';
execSync('cargo build --release -p api', {
  cwd: 'admin-operations-rust',
  stdio: 'inherit',
});
```

(Direnv's `.envrc` should already put `cargo` on PATH when Playwright inherits the shell env; verify by running `which cargo` from the test harness if the build fails.)

### Verify `/healthz` is actually a thing

`admin-operations-rust/crates/api/src/lib.rs` — confirm the router has a `/healthz` route and it returns 200 when the pool is up. If it doesn't exist yet, add it (axum handler that returns `StatusCode::OK` after `db::ping`). The main entry at `crates/api/src/main.rs` already runs `db::migrate` on startup, so by the time `/healthz` responds, migrations are applied.

### Edge: migrations race

Both SvelteKit (`pnpm run migrate` in `global-setup.ts`) and Rust (`db::migrate` on startup) hit the same DB. `sqlx::migrate` locks `_sqlx_migrations` row-by-row and is idempotent against already-applied TS migrations. **BUT** if Playwright starts both webServers in parallel before `global-setup.ts` finishes seeding, the TS seed and Rust startup can race on the same tables.

Solution: `tests/e2e/global-setup.ts` must `await` migrate+seed BEFORE returning. Playwright calls globalSetup before webServers start, so this ordering is guaranteed. Confirm by reading `tests/e2e/global-setup.ts` — if migrate/seed is already synchronous there, you're fine.

## Verification gates (all must pass before commit)

```sh
# From repo root
pnpm check                                          # 0 / 0 / 0
pnpm test                                           # 7/7
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/meals_test
bash -c '. .direnv/flake-profile-*.rc && cd admin-operations-rust && cargo test --workspace'
                                                    # 524/524
bash -c '. .direnv/flake-profile-*.rc && cd admin-operations-rust && cargo clippy --workspace --all-targets -- -D warnings'
                                                    # 0
bash -c '. .direnv/flake-profile-*.rc && cd admin-operations-rust && cargo fmt --check'
                                                    # 0
pnpm test:e2e                                       # exit 0  ← the new one
```

Spot-check a known-failing spec first before running the full ~12 min suite:

```sh
pnpm test:e2e --project chromium -g "orders list page loads"
```

## Commit

Single logical commit:

```
test(e2e): boot Rust API alongside SvelteKit in Playwright

Previously Playwright started only the SvelteKit dev server, so every
+page.server.ts that calls internalFetch() SSR-500'd on `RUST_API_URL
is not set`. Add a second webServer entry that runs the release-built
Rust binary on :8081, share INTERNAL_AUTH_SECRET between both, and
pre-build the binary once in globalSetup.
```

Stay local. Do NOT push.

## After 5a — what's next

Preserved from the plan file (`/Users/matthewdehaas/.claude/plans/i-need-to-do-fluttering-hellman.md`):

1. **Track 5b — Importer property tests** (~0.5 day). `admin-operations-rust/crates/excel/tests/import_proptest.rs` using `proptest`. Random valid rows → successful import; random invalid rows → structured `ImportRowError`; no partial inserts on failure.
2. **Track 6 — Docs** (~0.5–1 day). Inventory existing `docs/` first (`client-railway-setup.md`, `internal-auth.md`, `todo` already exist). Add `docs/auth.md`, `docs/excel-import.md`, `docs/architecture.md`, `docs/ops.md`.
3. **Track 7 — Feature-flag TUI** (deferred until Tracks 5 + 6 done). Design converged on:
   - `admin-operations-rust/crates/flags-tui/` with ratatui + sqlx + clap
   - Grid UI (flag × company); keyboard nav (j/k/h/l/space/d/w/q)
   - Emits migration files only; never writes DB; never talks to remote
   - Read-only `/admin/flags` web mirror (admin-only, server-rendered)
   - Ship one real gated feature as working example (financials or insights)
4. **Track 4 — CI/CD + Infisical** (LAST). GHA → GHCR → Railway Docker-image deploy; Infisical free tier for secrets.

## Pointers

- Full plan: `/Users/matthewdehaas/.claude/plans/i-need-to-do-fluttering-hellman.md`
- Prior transcript: `/Users/matthewdehaas/.claude/projects/-Users-matthewdehaas-Professional-meals-on-the-move-admin-operations-svelte/17678f4f-4c41-4ca3-b6d6-940762de6522.jsonl`
- Rust api entry: `admin-operations-rust/crates/api/src/main.rs`
- Migrations: `scripts/migrations/` (shared; Rust copies at build time via `scripts/sync-rust-migrations.sh` locally and `COPY` in `admin-operations-rust/Dockerfile`)
- SvelteKit → Rust HTTP wrapper: `src/lib/server/api-client.ts` (`internalFetch`, `callerFromLocals`)
- Playwright config: `playwright.config.ts`
- Playwright global setup: `tests/e2e/global-setup.ts`
