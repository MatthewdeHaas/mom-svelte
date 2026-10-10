# Operations

What an operator needs to run the service and troubleshoot it. Deeper
architecture context in `docs/architecture.md`.

## Services

Two Railway services from this repo, one shared Postgres:

- **SvelteKit** (Node). `Dockerfile`. Public URL.
- **Rust API** (`admin-operations-rust/Dockerfile`). Private only — not
  publicly reachable; SvelteKit talks to it over the Railway private
  network.
- **Postgres** — Railway's managed service in prod, Docker locally.

## Env-var inventory

Baseline from `.env.example`. **Secrets must come from Railway's secrets
UI, never the repo**.

| Var | Services | Secret? | Notes |
|---|---|---|---|
| `DATABASE_URL` | both | — | Postgres DSN. |
| `POSTGRES_USER/PASSWORD/DB` | SvelteKit | yes | Alternate shape; see `.env.example:1`. |
| `BETTER_AUTH_SECRET` | SvelteKit | **yes** | Session signing. |
| `BETTER_AUTH_URL` | SvelteKit | — | Public base URL. |
| `ADMIN_EMAILS` | SvelteKit | — | Comma-separated. First is seeded. |
| `ADMIN_INITIAL_PASSWORD` | SvelteKit | **yes** | Rotated after first login. |
| `RESEND_API_KEY` | SvelteKit | **yes** | Resend. |
| `EMAIL_FROM` | SvelteKit | — | Verified sender string. |
| `LOG_LEVEL` | SvelteKit | — | Pino level. |
| `RUST_API_URL` | SvelteKit | — | `http://rust:8080` on private net. |
| `INTERNAL_AUTH_SECRET` | both | **yes** | ≥32 bytes. Must match on both. |
| `PORT` | Rust | — | Default 8080 (`main.rs:71`). |
| `DATABASE_MAX_CONNECTIONS` | Rust | — | Default 10 (`db/src/lib.rs:28`). |
| `RUST_LOG` | Rust | — | tracing EnvFilter. Default `info,sqlx::query=warn`. |

## Deploys

GitHub Actions → Railway. CI runs on every push to `main`
(`.github/workflows/ci.yaml`); on a green run, `deploy.yaml` fires via
`workflow_run` and ships both services in parallel.

The workflow matrix is `[mom-svelte, Rust-API]`. For each service it:

1. Pulls prod secrets from Infisical (project `admin-operations`, env
   `prod`) using the `Infisical/secrets-action`.
2. Filters down to that service's allowlist (web gets Better Auth +
   `RUST_API_URL`; Rust gets `DATABASE_URL` — never the other way
   around). Missing keys log a warning but don't fail the deploy, so a
   staged rollout can add a key later.
3. `railway variables --set K=V ...` pushes the allowlisted values.
4. `railway up --service <svc> --detach` triggers the build.

Both Dockerfiles build from this repo on Railway's side. Build contexts
are the repo root so each image can `COPY scripts/migrations` (see the
"Migrations in prod" section — symlinks out of context would silently
produce an empty sqlx migrator).

**Manual re-deploy:** `gh workflow run deploy.yaml` or the Actions UI →
Deploy → Run workflow. Same path CI uses; no manual `railway up` needed.

**Escape hatch:** if Infisical is unavailable, set repo variable
`INFISICAL_BYPASS=1` and re-run. The workflow skips both the fetch and
the `railway variables --set` steps — Railway keeps serving with its
last-known-good values. Clear the bypass once Infisical is back.

## Health checks

Both services expose `GET /healthz` (DB-only, 500 ms timeout, 200/503).
SvelteKit also exposes `GET /healthz/deep` which additionally probes
Rust via `internalFetch` and returns 200 only if both sides are green.

Railway's probe target is `/healthz`, **not** `/healthz/deep` — coupling
liveness to the Rust service would cause cascading restarts whenever
Rust cycles. Use `/healthz/deep` from an uptime monitor or by hand.

- Rust: `admin-operations-rust/crates/api/src/health.rs`.
- SvelteKit shallow: `src/routes/healthz/+server.ts`.
- SvelteKit deep: `src/routes/healthz/deep/+server.ts`.

## Migrations in prod

The Rust binary owns migrations in prod: `sqlx::migrate!` runs on
startup against the shared Postgres
(`admin-operations-rust/crates/api/src/main.rs:30`). Ledger:
`_sqlx_migrations` (SHA-384).

The SvelteKit container no longer runs the TS migrator — its `CMD` only
seeds the admin user (when `SEED_ADMIN=1`) and starts the Node server
(`Dockerfile:36`). The TS migrator and its `migrations` (sha256) ledger
remain live for dev/CI, where tests still rely on it.

All migrations are idempotent (`CREATE TABLE IF NOT EXISTS`,
`DO $$ ... to_regclass() ...`), so a fresh dev DB can run either
migrator first with no divergence.

Source of truth is `scripts/migrations/`. The Rust image gets a build-
time **copy** (not symlink) via `admin-operations-rust/Dockerfile` —
`sqlx::migrate!` embeds bytes at compile time, so a symlink out of the
build context would silently produce an empty migrator.

## Logs

Both services write JSON to stdout; Railway's log viewer scrapes it.

- Rust: `tracing` + `tracing-subscriber` (`main.rs:81`). Filter via
  `RUST_LOG`.
- SvelteKit: Pino (`src/lib/server/logger.ts`). Auth paths redacted at
  `src/hooks.server.ts:26`.

Request IDs propagate end-to-end via `X-Request-Id` (`crates/api/src/lib.rs:297`).
Grep by request id to trace a request across the HTTP hop.

## Common failures

- **`RUST_API_URL is not set`** — SvelteKit env missing the var. SSR
  500s on every admin/client route. Thrown at
  `src/lib/server/api-client.ts:79`.
- **`INTERNAL_AUTH_SECRET must be at least 32 bytes`** — Rust refuses
  to start (`main.rs:62`). Set it on the Rust service.
- **HMAC `401 unauthorized` on every call** — the two services disagree
  on `INTERNAL_AUTH_SECRET`. Most common cause: one side redeployed
  with the new value, the other hasn't yet; or `.env` leaked a stale
  value in dev. Rust logs the specific reason at `warn` level
  (`crates/api/src/auth.rs:195`).
- **Import 400 `invalid type: string "false", expected a boolean`** —
  regression warning. axum query extractor needs the shared
  `bool_from_str` deserializer; fixed in commit `09e4e671`. If this
  recurs on a new endpoint, that endpoint's query struct is missing
  the custom deserializer.
- **xlsx/PDF diffs across runs** — the determinism post-processors
  regressed. See `architecture.md` → "Byte-identical exports" and the
  three guards in `crates/api/tests/exports.rs`.

## Rotating `INTERNAL_AUTH_SECRET`

Deploy both services with the new secret in the same Railway rollout.
Expect a few seconds of 401 blips mid-rollout — the browser surfaces
these as a toast and users retry. Accepted blast radius; same tradeoff
documented in `docs/internal-auth.md`.

## Database

- **Prod:** Railway managed Postgres. Backups via Railway's native
  snapshots; retention per plan. Restore flow: Railway UI → snapshot
  → restore to new DB → repoint `DATABASE_URL` on both services.
- **Local:** `docker compose up -d db`.

Never run raw `postgres` / `pg_ctl` locally — always use the compose
service so credentials, port, and volume stay consistent with the
rest of the stack.

## Infisical

Prod secrets live in Infisical, not GitHub or Railway's dashboard.

**Project layout:**

- Project: `admin-operations`
- Environments: `dev`, `prod`
- Machine identities (universal auth):
  - `ci-dev-reader` — read `dev`. Used by local `infisical run`.
  - `ci-prod-reader` — read `prod`. Used by `deploy.yaml`.

**GitHub repo secrets required for `deploy.yaml`:**

| Secret | Purpose |
|---|---|
| `INFISICAL_CLIENT_ID_PROD` | `ci-prod-reader` identity ID |
| `INFISICAL_CLIENT_SECRET_PROD` | `ci-prod-reader` secret |
| `RAILWAY_TOKEN` | project-scoped Railway token |

**Local use:** `infisical run --env=dev -- pnpm dev`. The `infisical` CLI
is in the flake, so `nix develop` or direnv autoload picks it up.

**Rotating a prod secret:** update it in Infisical → re-run
`deploy.yaml` (manual dispatch is fine) → Railway redeploys both
services with the new value. Both services share
`INTERNAL_AUTH_SECRET`, so expect a few seconds of 401 blips mid-
rollout; same tradeoff as the `INTERNAL_AUTH_SECRET` rotation section
above.

**Adding a new prod secret:**

1. Add the key in Infisical → prod.
2. Add the key to the matching service's `allowlist` in
   `.github/workflows/deploy.yaml`. **Do not** copy a web secret into
   Rust's allowlist or vice versa — the allowlist is the only thing
   keeping blast radius bounded.
3. Push the workflow change; the next deploy will pick it up.

## Local troubleshooting

- `docker compose logs db` — Postgres logs.
- `pnpm dev` — SvelteKit on 5173.
- `cd admin-operations-rust && cargo run -p api` — Rust on 8080.

**Port note:** `pnpm test:e2e` spins up its own copies on 5174/8081
(`playwright.config.ts:7,29`). Don't run `pnpm dev` + e2e at the same
time if you're already on 5173/8080 — they won't collide, but it's
easy to hit the wrong one. e2e's SvelteKit port 5174 and Rust port
8081 are intentional to keep the dev stack clear.
