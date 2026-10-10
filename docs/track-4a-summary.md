# Track 4a — CI/CD + Infisical: Summary & Next Steps

> **Withdrawn 2026-10-09 — Infisical removed from the pipeline.**
>
> After setting up the Infisical account and starting the port-over,
> we concluded the vault's complexity wasn't justified at this scale
> (one deployer, one env, ~9 secrets, deploys measured in weeks).
> `deploy.yaml` was rewritten to just run `railway up` — no fetch, no
> allowlist, no bypass flag. Secrets now live in Railway's per-service
> dashboard directly.
>
> The sections below are preserved as historical record of the Track
> 4a design pass. The **CI / healthchecks / request-ID / migrations**
> material is still accurate. The **Infisical bootstrap guide** is
> dead — ignore it. Current ops reality is in `docs/ops.md § Secrets`.
> Revisit a vault when the team passes one deployer or services pass ~5.

Track 4a landed in four commits on `main` (all local, nothing pushed):

| Commit | Scope |
|---|---|
| `4e697ff2` | Rename `ci.yml` → `ci.yaml`, add Rust gates, drop pre-Rust deploy job |
| `7a4c96ff` | `/healthz` + `/healthz/deep`, request-id propagation, migration lint, idempotent migrations |
| `13f00482` | Add `railway` + `infisical` CLIs to the Nix dev shell |
| `6dd801db` | Migrate secrets to Infisical, two-service Railway deploy workflow |

The Nix flake entry (`13f00482`) landed in the middle because the user
needed `railway` on PATH mid-session; it logically belongs to Commit 3's
scope.

## What's actually running now

**CI (`.github/workflows/ci.yaml`)** — on every push to `main` and every
PR:

- `ci` job: migration lint → `pnpm run migrate` → `pnpm check` → `pnpm test`
- `rust-ci` job: sqlx offline cache check → `cargo fmt --check` → `cargo clippy -D warnings` → `cargo test --workspace` (534 tests)
- `e2e` job: Playwright against SvelteKit + Rust running concurrently

Both services' Postgres is a service container; CI env has placeholder
Better Auth secrets that are **never** promoted to prod.

**Healthchecks:**

- `GET /healthz` on both SvelteKit and Rust — DB-only probe, 500 ms
  timeout, 200/503. Railway's probe target.
- `GET /healthz/deep` on SvelteKit only — DB + Rust-via-`internalFetch`.
  Reserved for uptime monitors; Railway deliberately does NOT use it
  because coupling liveness to Rust would cascade-restart web when Rust
  cycles.

**Request IDs:**

- `handleRequestId` is the first hook in `src/hooks.server.ts`'s
  sequence. It accepts an inbound `X-Request-Id` matching
  `^[A-Za-z0-9._~-]{1,128}$`, else mints a UUIDv4. The ID lands on
  `event.locals.requestId`, is echoed back on the response, is attached
  to every Pino log line as `requestId`, and is forwarded to Rust as a
  plain header (not HMAC-signed — Rust's `MakeRequestUuid` middleware
  prefers the incoming value).

**Migrations:**

- Prod: Rust owns migrations via `sqlx::migrate!` on startup. SvelteKit's
  `CMD` no longer runs the TS migrator; it only seeds the admin user
  when `SEED_ADMIN=1`.
- Dev/CI: both migrators still run (TS ledger `migrations` sha256, sqlx
  ledger `_sqlx_migrations` SHA-384). All migrations are idempotent
  (`CREATE TABLE IF NOT EXISTS`, `DO $$ ... to_regclass() ...`) so order
  between them doesn't matter.
- `scripts/lint-migrations.ts` enforces the idempotent patterns in CI.
  Added to `pnpm run lint:migrations` and gated in `ci.yaml`.

**Deploy workflow (`.github/workflows/deploy.yaml`) — current state after 2026-10-09 rewrite:**

- Triggered by `workflow_run` after CI goes green on `main`, or manual
  dispatch.
- Matrix over `[mom-svelte, Rust-API]` runs `railway up --service <svc>
  --detach` in parallel. Nothing else — no secret fetch, no allowlist,
  no bypass flag.
- Blast-radius bound is now Railway-native: a secret set on
  `mom-svelte`'s dashboard is invisible to `Rust-API` and vice versa.

**Local dev:**

- Flake ships `railway` CLI. direnv autoload picks it up.
- `scripts/backfill-dev-sqlx.ts` is a dev-only tool to seed the sqlx
  ledger from an existing TS-migrated DB so `/healthz/deep` works
  without a full nuke-and-replay.

## Setting up Infisical — step by step

> **Section withdrawn 2026-10-09.** Infisical was removed from the
> pipeline. The guide below is kept as historical record of the design
> pass; **do not follow it**. Current ops reality: edit secrets
> directly in Railway's per-service dashboard. See `docs/ops.md §
> Secrets`.

<details>
<summary>Historical Infisical bootstrap (do not follow)</summary>

**1. Create the account + project** (~5 min on the web)

- Sign up at <https://app.infisical.com> (free tier is more than enough
  for this project).
- New project → name it **`admin-operations`**.
- Infisical gives you two envs by default: `dev` and `prod`.

**2. Populate `prod` secrets** from `railway variables --service
mom-svelte` and `--service Rust-API`.

**3. Create a machine identity** (`ci-prod-reader`, Universal Auth,
read-only on `admin-operations/prod`).

**4. Add GitHub repo secrets**: `INFISICAL_CLIENT_ID_PROD`,
`INFISICAL_CLIENT_SECRET_PROD`, `RAILWAY_TOKEN`.

**5. (Optional)** `ci-dev-reader` for local `infisical run --env=dev`.

</details>

## How close you are to a real deploy

**Current state (2026-10-09):** one `git push` away. The sequence is:

1. Push `main`.
2. CI (`ci.yaml`) runs: typecheck, vitest, Rust fmt/clippy/test, E2E.
3. On green, GitHub fires `deploy.yaml` via `workflow_run`.
4. Deploy's matrix runs `railway up --service <svc> --detach` on
   `mom-svelte` and `Rust-API` in parallel.
5. Railway builds the two images using the values already in each
   service's Variables dashboard, swaps traffic once `/healthz`
   returns 200.

**Three things to verify before the first push:**

- Railway service names in `.github/workflows/deploy.yaml` match the
  dashboard **exactly**: `mom-svelte` and `Rust-API`. Case and hyphen
  matter.
- `RAILWAY_TOKEN` GH secret exists and is project-scoped to
  `striking-encouragement`, not account-wide. Account-wide tokens fail
  with a confusing permissions error on `railway up`.
- The `Rust-API` Railway service points at the Rust `railway.toml`. If
  you created the service by picking the repo in Railway's UI without
  specifying a config path, add `Config File Path =
  admin-operations-rust/railway.toml` in that service's Settings →
  Deploy. Otherwise Railway will try to build with the top-level
  `railway.toml` and deploy the SvelteKit image into the Rust service.

**What's untested until you do the above:** the `railway up` matrix in
GHA. The CLI call is trivial, but it's never been run against a real
project token from a GHA runner for this repo.

## Is this Track 4b?

No. Track 4b (GHCR Docker image builds) is a separate deferred
optimization — see `docs/handoff-track4b.md` once it's written. 4a is
closed by the first green `workflow_run → deploy.yaml` cycle after the
three pre-flight checks pass.

## Still local

Nothing has been pushed. Per the standing directive, commits stay on
your laptop until you explicitly ask. When you're ready:

```sh
git push origin main
```

That one push triggers CI → on green, deploys. Confirm the three
pre-flight items above first.
