# Track 4a — CI/CD + Infisical: Summary & Next Steps

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

**Deploy workflow (`.github/workflows/deploy.yaml`):**

- Triggered by `workflow_run` after CI goes green on `main`, or manual
  dispatch.
- Matrix over `[mom-svelte, Rust-API]` runs the two services in
  parallel. Per-service allowlist: web gets `BETTER_AUTH_SECRET`,
  `BETTER_AUTH_URL`, `INTERNAL_AUTH_SECRET`, `RUST_API_URL`,
  `RESEND_API_KEY`, `EMAIL_FROM`, `ADMIN_EMAIL`. Rust gets
  `DATABASE_URL`, `INTERNAL_AUTH_SECRET`, `RUST_LOG`. The allowlist is
  the only thing bounding blast radius if a secret is accidentally
  shared.
- Escape hatch: repo var `INFISICAL_BYPASS=1` skips both the fetch and
  the `railway variables --set` steps so Railway can still deploy on
  last-known-good values during an Infisical outage.

**Local dev:**

- Flake now ships `railway` and `infisical` CLIs. `nix develop` or
  direnv autoload picks them up.
- `scripts/backfill-dev-sqlx.ts` is a dev-only tool to seed the sqlx
  ledger from an existing TS-migrated DB so `/healthz/deep` works
  without a full nuke-and-replay.

## Setting up Infisical — step by step

**1. Create the account + project** (~5 min on the web)

- Sign up at <https://app.infisical.com> (free tier is more than enough
  for this project).
- New project → name it **`admin-operations`**. The slug must match
  what `deploy.yaml:67` passes to `project-slug`; if you name it
  differently, update the workflow to match.
- Infisical gives you two envs by default: `dev` and `prod`. That's
  what the workflow expects. Leave them.

**2. Populate `prod` secrets**

Fastest way to grab current prod values is from your laptop:

```sh
railway login            # if you haven't already
railway link             # pick striking-encouragement / production
railway variables --service mom-svelte
railway variables --service Rust-API
```

Then in Infisical → `admin-operations` → `prod`, add:

| Key | Where from | Who gets it in deploy |
|---|---|---|
| `BETTER_AUTH_SECRET` | current `mom-svelte` value | web |
| `BETTER_AUTH_URL` | current `mom-svelte` value (public URL) | web |
| `INTERNAL_AUTH_SECRET` | current value (shared both sides) | both |
| `RUST_API_URL` | `http://rust-api.railway.internal:8080` or Railway's internal DNS | web |
| `RESEND_API_KEY` | current `mom-svelte` value | web |
| `EMAIL_FROM` | `Meals on the Move <noreply@mealsonthemove.ca>` | web |
| `ADMIN_EMAIL` | your admin email | web |
| `DATABASE_URL` | current `Rust-API` value (Railway Postgres DSN) | Rust |
| `RUST_LOG` | `info,sqlx::query=warn` | Rust |

Double-check `RUST_API_URL` — on Railway's private network it's
`<service>.railway.internal`. If the service is named `Rust-API` in the
dashboard, the DNS name becomes `rust-api.railway.internal` (Railway
lowercases).

**3. Create the machine identity**

Infisical UI → **Access Control** → **Machine Identities** → **Create**.

- Name: `ci-prod-reader`
- Auth method: **Universal Auth** (that's what
  `Infisical/secrets-action@v1.0.9` uses).
- Scope: read-only access to `admin-operations` → `prod`. No write, no
  `dev`. (This is Infisical's "Role" + "Project" permissions on the
  identity.)
- Save. The next screen shows **Client ID** and **Client Secret** —
  **copy both now**, the secret is shown exactly once.

**4. Add GitHub repo secrets**

Repo → **Settings** → **Secrets and variables** → **Actions** → **New
repository secret**:

- `INFISICAL_CLIENT_ID_PROD` — paste the client ID
- `INFISICAL_CLIENT_SECRET_PROD` — paste the client secret
- `RAILWAY_TOKEN` — from Railway: Account Settings → Tokens → **Create
  Token**, project-scoped to `striking-encouragement`. **Not** account-
  wide; account-wide tokens fail with a confusing permissions error on
  `railway up`.

**5. (Optional but recommended) `dev` identity for local use**

Same shape:

- `ci-dev-reader` machine identity, scoped read-only to
  `admin-operations` → `dev`.
- Populate `dev` secrets in Infisical (your local Better Auth secret,
  the dev Postgres DSN, etc.).
- Add to `~/.zshrc` or your shell profile:
  ```sh
  export INFISICAL_CLIENT_ID=<dev-reader-id>
  export INFISICAL_CLIENT_SECRET=<dev-reader-secret>
  ```
- Then run the app with `infisical run --env=dev -- pnpm dev` and
  `infisical run --env=dev -- cargo run -p api`. This gets `.env.local`
  out of the repo entirely.

The `infisical` CLI is already in the flake, so `nix develop` or direnv
autoload has it on PATH.

## How close you are to a real deploy

Once GH secrets are set and Infisical is populated, you are **one `git
push` away**. The sequence is:

1. Push `main`.
2. CI (`ci.yaml`) runs: typecheck, vitest, Rust fmt/clippy/test, E2E.
3. On green, GitHub fires `deploy.yaml` via `workflow_run`.
4. Deploy's matrix runs `mom-svelte` and `Rust-API` in parallel. For
   each: Infisical fetch → allowlist filter → `railway variables --set`
   → `railway up --detach`.
5. Railway builds the two images, swaps traffic once `/healthz` returns
   200.

**Three things to verify before the first push or the first run will
fail:**

- Railway service names in `.github/workflows/deploy.yaml` match the
  dashboard **exactly**: `mom-svelte` and `Rust-API`. Case and hyphen
  matter.
- `RAILWAY_TOKEN` is project-scoped to `striking-encouragement`, not
  account-wide.
- The `Rust-API` Railway service points at the Rust `railway.toml`. If
  you created the service by picking the repo in Railway's UI without
  specifying a config path, add `Config File Path =
  admin-operations-rust/railway.toml` in that service's Settings →
  Deploy, otherwise Railway will try to build with the top-level
  `railway.toml` (which builds the SvelteKit image and would deploy the
  wrong binary to the wrong service).

**What's untested until you do the above:**

- The `Infisical/secrets-action@v1.0.9` call — never run against a real
  tenant in this repo.
- The `railway variables --set` + `railway up` matrix in GHA.

If anything breaks on the first real deploy, those two steps are where
to look. The escape hatch (`vars.INFISICAL_BYPASS=1`) exists specifically
so an Infisical hiccup doesn't block an emergency deploy — set the var,
re-run, Railway serves on its stored values.

## Is this Track 4b?

No. There is no Track 4b in the plan. This is the final shape of 4a —
Infisical account creation is deliberately a human step (OAuth, UI
clicks, scoped tokens) that doesn't belong in automation. The code side
is finished; the ops side is a one-time setup documented above and in
`docs/ops.md`.

## Still local

Nothing has been pushed. Per the standing directive, commits stay on
your laptop until you explicitly ask. When you're ready:

```sh
git push origin main
```

That one push triggers CI → on green, deploys. Confirm the three
pre-flight items above first.
