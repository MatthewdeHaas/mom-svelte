# admin-operations-rust

Rust backend service for Meals on the Move admin operations. Deployed as a
separate Railway service, called by the SvelteKit service over Railway's
private network via HMAC-signed internal auth headers.

## Workspace layout

| Crate | Purpose |
|---|---|
| `domain` | Pure types + `Flag` trait + role/context. No I/O. |
| `db` | `sqlx::PgPool` wrapper + migrations runner. |
| `flags` | Feature-flag registry (DB-backed, uses `domain::Flag`). |
| `excel` | `calamine` (import) + `rust_xlsxwriter` (export). |
| `pdf` | `printpdf` (packing slip generator). |
| `api` | `axum` router, HTTP handlers, auth middleware, `main.rs`. |

Each crate ships behind feature flags during rollout — see `flags` crate
and the `use_rust_backend_<domain>` convention in SvelteKit.

## Migrations

`sqlx::migrate!` embeds migrations at compile time. The source of truth
is `../scripts/migrations/` (SvelteKit tree); this crate reads them from
`./migrations/` which is a build-time copy, not a symlink. See
`crates/db/src/lib.rs` for the macro invocation.

Local development:
```sh
pnpm rust:sync-migrations    # from repo root — copies ../scripts/migrations/ here
cargo check --workspace
```

In Docker the sync happens via `COPY scripts/migrations/ /app/admin-operations-rust/migrations/`
in the Dockerfile (real copy, before `cargo build`).

## Running locally

```sh
# From repo root
docker compose up -d db                            # Postgres on 5432
pnpm rust:sync-migrations                          # populate ./migrations
INTERNAL_AUTH_SECRET=dev-secret-change-me \
  DATABASE_URL="postgresql://..." \
  cargo run -p api                                 # serves on :8080
curl -i http://localhost:8080/healthz
```

## Internal auth contract

See `../docs/internal-auth.md` for the full specification. Short version:

```
X-Internal-User:      <user_id>
X-Internal-Role:      admin | client
X-Internal-Company:   <uuid or empty string>
X-Internal-Timestamp: <unix seconds>
X-Internal-Nonce:     <random 16-byte hex>
X-Internal-Auth:      hex(HMAC-SHA256(secret, "user|role|company|ts|nonce"))
```

Rust rejects requests outside a ±60s skew window and caches seen nonces
in-memory for 60s to close the replay window.

## Status

**Phase 1: scaffold** — this commit. Workspace compiles, `/healthz` responds,
HMAC middleware verifies headers, feature-flag tables exist. No business
logic ported yet.

Next phases, in order (one sub-session each):
1. Excel import crate (port `src/lib/server/queries/imports.ts`)
2. Query layer per-domain (port `src/lib/server/queries/*.ts` behind flags)
3. Exports (packing slip PDF + xlsx exports)
4. Insights + financials
5. Cutover (flip flags, delete Node query files)
