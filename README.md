# Meals on the Move — Admin Operations

SvelteKit 2 + Svelte 5 admin app. PostgreSQL via `pg`, Better Auth magic-link, Tailwind 4.

---

## Prerequisites

- Node.js 20+
- pnpm (`npm i -g pnpm`)
- Docker Desktop (for the dev database)
- `psql` CLI (for `db:reset-test` — ships with PostgreSQL client tools)

---

## First-time setup

```sh
pnpm install
pnpm exec playwright install chromium   # E2E browser binary

cp .env.example .env                    # fill in secrets (see .env.example)
pnpm db:reset-test                      # create meals_test + run migrations
```

The dev Docker database is managed separately — see [Dev environment](#dev-environment) below.

---

## Dev environment

All dev commands assume Docker Desktop is running.

| Command | What it does |
|---|---|
| `pnpm dev` | Start the Vite dev server (port 5173) against the Docker DB |
| `bash scripts/dev-up.sh` | `docker compose up` + migrate + seed admin, then tail logs |
| `docker compose up -d` | Start DB + web container in the background |
| `docker compose down` | Stop and remove containers (data volume persists) |
| `docker compose down -v` | Stop containers **and wipe the dev DB volume** |
| `docker compose logs -f web` | Tail the web container logs |
| `pnpm seed:admin` | Upsert the admin user defined in `.env` → `ADMIN_EMAIL` |

> `pnpm dev` reads `.env` directly and connects to `localhost:5432` (the Docker-exposed port). It does **not** require the Docker web container — just the `db` service.

To run only the database without the web container:

```sh
docker compose up -d db
pnpm dev
```

---

## Testing

Tests use a separate `meals_test` database so they never touch dev data.

### One-time test DB setup

```sh
pnpm db:reset-test   # drop + recreate meals_test, run all migrations
```

Repeat this any time you add a migration or want a clean slate.

### Running tests

| Command | What it does |
|---|---|
| `pnpm test` | Unit + integration tests (328 tests, ~1 s) |
| `pnpm test:e2e` | Playwright E2E tests (29 tests, ~40 s) — starts its own dev server |
| `pnpm test:all` | Both suites in sequence |

All three commands load `.env.test` automatically — no env vars needed.

The E2E suite starts a Vite dev server automatically via `playwright.config.ts`. If port 5173 is already in use from a separate `pnpm dev` session, Playwright will reuse it.

### Killing a stuck dev server

The Playwright-managed server is killed when the test run ends normally. If it gets orphaned:

```sh
pkill -f "vite dev"
# or find the PID and kill manually:
lsof -i :5173
kill <PID>
```

---

## Database operations

| Command | What it does |
|---|---|
| `pnpm db:reset-test` | Drop + recreate `meals_test`, apply all migrations |
| `pnpm migrate` | Apply pending migrations to `meals_test` (reads `.env.test`) |
| `docker compose down -v` | Wipe the dev DB volume entirely (destructive) |

To wipe and rebuild the **dev** database:

```sh
docker compose down -v          # removes the postgres_data volume
bash scripts/dev-up.sh          # brings it back up, migrates, seeds admin
```

---

## Scripts reference

| Script | File | Notes |
|---|---|---|
| `dev-up.sh` | `scripts/dev-up.sh` | Boots docker-compose (db + rust-api); Rust API migrates on its own startup |
| `reset-test-db.sh` | `scripts/reset-test-db.sh` | Wipe + rebuild `meals_test` (runs `sqlx migrate run` internally) |
| `seed-admin.ts` | `scripts/seed-admin.ts` | Upsert admin user from `ADMIN_EMAIL` |

---

## Environment files

| File | Purpose | Committed? |
|---|---|---|
| `.env` | Dev secrets (Docker DB, real Resend key, prod Better Auth URL) | No |
| `.env.test` | Test DB + placeholder secrets for unit/integration/E2E | Yes |
| `.env.example` | Template — copy to `.env` and fill in values | Yes |
