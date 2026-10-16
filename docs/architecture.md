# Architecture

System overview for new contributors. The code is the source of truth —
sections point at files, not snippets.

## Topology

Two Railway services on the same private network, sharing one Postgres:

- **SvelteKit** (Node). Serves the browser UI, SSRs every page, owns the
  auth session. `Dockerfile`.
- **Rust API** (`admin-operations-rust/`). Owns every business query and
  every write. Not publicly reachable; only SvelteKit talks to it.
  `admin-operations-rust/Dockerfile`.

Standing directive: **business logic in Rust, HTTP transport in TypeScript**.

## HTTP boundary

Every `+page.server.ts` resolves a caller from `locals.user` and calls
`internalFetch`. No SQL, no domain logic in TypeScript.

- Caller + signer: `src/lib/server/api-client.ts:50` (`callerFromLocals`),
  `src/lib/server/api-client.ts:72` (`internalFetch`).
- Rust binary entry: `admin-operations-rust/crates/api/src/main.rs:15`.
- Router (every endpoint listed here): `admin-operations-rust/crates/api/src/lib.rs:50`.

Representative round-trip for `GET /api/orders/paginated`:

```
Browser → src/routes/admin/orders/+page.server.ts:12  (load)
       → getOrdersPaginated(...)  (dispatch wrapper)
       → api-client.ts:72 internalFetch         [HMAC-sign 6 headers]
       → api/src/auth.rs:180 verify_internal_auth
       → api/src/orders.rs:179 list_paginated
       → db::queries::orders::list_paginated    (sqlx)
       → Postgres
```

## Internal auth

HMAC-SHA256 over `user|role|company|timestamp|nonce`, 60-second skew
window, in-memory nonce cache. **Full protocol:** `docs/internal-auth.md`.
Verifier at `admin-operations-rust/crates/api/src/auth.rs:180`
(middleware) and `:203` (core verifier).

## Crate map

From `admin-operations-rust/Cargo.toml:3`:

- **`domain`** — enums, DTOs, warning taxonomy. No I/O.
- **`db`** — pool + `sqlx::migrate!` + query modules. Entry:
  `admin-operations-rust/crates/db/src/lib.rs:1`.
- **`flags`** — flag registry (ZST per flag). Entry:
  `admin-operations-rust/crates/flags/src/lib.rs:1`.
- **`excel`** — xlsx importer and exporters. Entry:
  `admin-operations-rust/crates/excel/src/import.rs:1`.
- **`pdf`** — packing-slip PDFs. Entry:
  `admin-operations-rust/crates/pdf/src/lib.rs`.
- **`api`** — axum router, HMAC middleware, handlers. Entry:
  `admin-operations-rust/crates/api/src/lib.rs:1`.

## Migration ledger (single-migrator architecture)

Rust is the sole migrator. One source directory
(`scripts/migrations/`), one ledger (`_sqlx_migrations`, SHA-384
checksums), one codepath (`db::migrate` at
`admin-operations-rust/crates/db/src/lib.rs:45`).

The Rust side reads from a **copy**, not a symlink: locally via
`scripts/sync-rust-migrations.sh`, in prod via `COPY scripts/migrations`
in `admin-operations-rust/Dockerfile`. `sqlx::migrate!` embeds bytes at
compile time; a symlink out of the build context silently produces an
empty migrator.

In production the Rust API runs `sqlx::migrate!` on binary startup
(`admin-operations-rust/crates/api/src/main.rs` → `serve`'s call to
`db::migrate`). In dev + CI the same binary is invoked as
`./admin-operations-rust/target/release/api migrate` (or `sqlx migrate
run --source migrations` inside the Rust workspace, which uses the
same `_sqlx_migrations` ledger). Idempotent — second-mover skips.

**History:** a Node migrator (`scripts/migrate.ts`, SHA-256 ledger)
coexisted with the Rust migrator through 2026-10-16. The two ledgers
drifted on prod and crashlooped the Rust API on boot. Collapsed to
Rust-only; Node migrator deleted.

## Byte-identical exports

xlsx and PDF exports must produce byte-identical bytes for the same
semantic input. Pinned by two post-processors:

- xlsx: `admin-operations-rust/crates/excel/src/determinism.rs:41`
  (`pin_xlsx_bytes`) — ZIP entry mtimes to DOS epoch, compression
  level 6, scrub `"Unknown"` from `docProps/core.xml`.
- PDF: `admin-operations-rust/crates/pdf/src/determinism.rs:38`
  (`pin_pdf_bytes`) — pin trailer `/ID` and XMP `InstanceID`.

Regression guards: three byte-identity tests in
`admin-operations-rust/crates/api/tests/exports.rs` seed data, hit the
endpoint twice, compare sha256 digests.

## Feature flags

DB-backed, resolved per-request. Flag definitions are ZSTs in
`admin-operations-rust/crates/flags/src/lib.rs`; admin surface at
`admin-operations-rust/crates/api/src/flags_handler.rs:31` (`list`) and
`:68` (`check`). `/admin/flags` is read-only in the browser.

`GET /api/flags/snapshot` (`flags_handler.rs:151`) is the hydration
endpoint for the local flags-TUI (`crates/flags-tui/`, see
`docs/feature-flags.md`). It returns the full catalog + raw
`feature_flag` + `feature_flag_company` rows + the company list in one
JSON blob. Protected by the standard HMAC middleware **plus** a
handler-level check that the caller's `user_id` matches
`AppState.flags_snapshot_user_id` — loaded once in `main.rs` from the
`FLAGS_SNAPSHOT_USER_ID` env var. Unset env var → 503 (fail-closed);
mismatched caller → 403. Carried on state (not re-read from env per
request) so parallel `#[sqlx::test]` runs don't race on
`std::env::set_var`. Audit-logged at `info` on every pull. Read-only
by construction — the only path to mutate flags is a migration file
applied through the normal Railway redeploy chain.

## Local dev

```sh
docker compose up -d db                                           # Postgres
pnpm dev                                                          # SvelteKit on 5173
bash -c '. .direnv/flake-profile-*.rc && cd admin-operations-rust && cargo run -p api'
```

Set `RUST_API_URL=http://localhost:8080` and `INTERNAL_AUTH_SECRET`
(≥32 bytes) in `.env`. The Rust binary refuses to start without the
latter (`admin-operations-rust/crates/api/src/main.rs:61`).

## Testing

Three layers:

- **Vitest integration** — `pnpm test`. Transaction-rollback pattern
  under `tests/integration/`.
- **Rust integration** — `cargo test --workspace`. Each test gets a
  fresh DB via `#[sqlx::test(migrations = "../../migrations")]`.
  Fixtures under `admin-operations-rust/crates/api/tests/`.
- **Playwright e2e** — `pnpm test:e2e`. Boots both services on 5174/8081
  against a shared DB. The Rust binary is pre-built in `globalSetup`
  (`tests/e2e/global-setup.ts:108`); `playwright.config.ts:60` starts
  both webServers in parallel. The migration-ledger reconciliation
  (above) also lives in `globalSetup`.

**Not covered here:** per-track history, every endpoint, every
migration. Grep the file paths above.
