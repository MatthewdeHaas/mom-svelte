# Handoff — Production-readiness pass + Rust migration

## Status
Plan approved and finalized at `~/.claude/plans/i-need-to-do-fluttering-hellman.md`. Read it first. Nothing has been executed yet. 15 files modified + 1 untracked migration (`scripts/migrations/007_production_sheet_schema.sql`) sit uncommitted — this is the Track 1a baseline, do not stash or revert.

## Decisions already made (do not re-ask)
- **Stack**: SvelteKit + pg + Better Auth + ExcelJS + pdfkit. See `memory/project_stack.md`.
- **Hosting**: Live on Railway at `mom-svelte-production.up.railway.app`. No custom domain — client refuses to manage DNS.
- **Auth**: email+password UI-facing; magic-link stays dormant in code for future customer self-serve.
- **Rust**: full migration (not partial). Cargo workspace at `admin-operations-rust/` inside this repo (not a new repo). Separate Railway service, axum, sqlx, calamine, rust_xlsxwriter, printpdf. HMAC-signed internal auth header from SvelteKit → Rust, 60s timestamp window.
- **Excel**: sheet 1 only; sheets 2..N silently ignored (info warning). Within-sheet normalization at import boundary. Downstream (PDF, xlsx exports) byte-identical between messy and clean inputs. Real file: `~/Downloads/WS 26 Master Prod Sheet Week {2,3}.xlsx`.
- **CI/CD**: GHA → GHCR → Railway "Deploy from Docker Image". Separate `ci.yml` (test gate) and `release.yml` (build + push on main). No rollback — fix forward.
- **Secrets**: Infisical free tier. Local via `infisical run`, GHA via `Infisical/secrets-action`, Railway via Infisical↔Railway sync. `.env.example` strips to non-secret config.
- **Feature flags**: Flag trait + DB registry + `/admin/flags` UI. Gate financials, insights, allergens. Flag-disabled routes `throw error(404)` not 403.

## User preferences (from memory)
- Worktrees for independent tracks (`memory/feedback_worktrees.md`) — provide tmux pane prompts.
- `/brainstorm` proactively on non-trivial tasks (`memory/feedback_brainstorm_proactive.md`).
- Ultracode + ultrathink for substantive work.

## Execution order
Six tracks. Spawn a fresh session per track — each eats ~200k.

1. **Track 1 (1-2 days)** — Verify migration 007 applies cleanly, run full test suite, commit WIP as one logical commit. Then email+password auth (magic-link dormant). Migration 008.
2. **Track 2 (2-3 days)** — Excel importer: sheet 1 only, in-app normalization, warnings for Item/Qty-formula/Frozen+Chilled/case-fold/boilerplate rows.
3. **Track 3 (2-3 weeks)** — Cargo workspace at `admin-operations-rust/`. Scaffold → excel import → query layer per-domain behind flags → exports → insights → cutover. Migration 009.
   - **Scaffold complete.** Workspace: `domain`, `db`, `flags`, `excel`, `pdf`, `api`. HMAC middleware (6-header signed, 60s timestamp window + 60s nonce cache in DashMap, HMAC-before-nonce-insert) with 6 passing unit tests. `/healthz` + `/api/flags` + `/api/flags/check`. SvelteKit signer at `src/lib/server/api-client.ts`. Migration 009 (feature_flag + feature_flag_company, 5 seeded flags). Dockerfile + docker-compose `rust-api` service. Flake-provided Rust toolchain.
   - **Step 2 complete** — Excel importer in Rust behind `use_rust_backend_imports`. Real-client fixtures + sanitizer + integration tests + byte-identical downstream exports.
   - **Step 3 domain-1 complete (orders).** sqlx offline cache under `admin-operations-rust/.sqlx/` committed; `pnpm rust:prepare` regenerates it. `admin-operations-rust/crates/db/src/queries/orders.rs` now covers the full 11-function surface (`list`, `list_paginated`, `get_by_id`, `list_by_shipment_date`, `counts_by_status`, `create`, `update`, `confirm`, `fulfill`, `cancel`, `set_status_batch`) with `query_as!` compile-time checks (dynamic WHERE paths use `QueryBuilder`). `/api/orders/*` HTTP handlers in `admin-operations-rust/crates/api/src/orders.rs` with admin-vs-client scoping injected server-side (clients get `company_id` forced regardless of input). 50 integration tests in `tests/orders.rs` cover list/paginated/get_by_id/counts/create/update/transitions/batch + client-scoping (which the TS suite can't express — no HTTP boundary). Migration 011 adds `use_rust_backend_orders`. TS side: new `src/lib/server/queries/_rust-dispatch.ts` with 30s per-caller flag cache + 60s warn-throttle + `serializeFilters`/`filtersToQueryString`/`normalizeBody` helpers. `src/lib/server/queries/orders.ts` — every exported fn gains an optional `caller?: InternalCaller`; routes stay untouched (omitting `caller` is a no-op → SQL path). `_rust-dispatch.ts` lazy-loads `api-client.ts` + `logger.ts` because both import `$env/*` / `$app/*` which vitest can't resolve — top-level static imports would break 6 test suites. 403 existing TS tests + 50 Rust tests + `pnpm check` all green. Pattern is proven; the rest of Step 3 is mechanical repetition.
   - **Step 3 domains 2-5 complete (recipes, companies, ingredients, allergens).** Same pattern as orders — see commits `2d76d76f`, `5fa2d87f`, `5fd0d8df`, `08781dc1`.
   - **Step 3 domain-6 complete (persons + attendance).** `admin-operations-rust/crates/db/src/queries/persons.rs` covers the full 13-function surface (`list_by_company`, `get_by_id`, `get_allergens`, `get_with_allergens`, `list_with_allergens_by_company`, `create`, `update`, `deactivate`, `add_allergen`, `remove_allergen`, `set_allergens`, `get_attendance_by_date`, `upsert_attendance`, `upsert_attendance_batch`) with `query_as!` compile-time checks; the two multi-row writes (`set_allergens`, `upsert_attendance_batch`) use `QueryBuilder::push_values`. `/api/persons/*` + `/api/attendance/*` handlers in `admin-operations-rust/crates/api/src/persons.rs` enforce admin-vs-client scoping via two helpers: `can_read_company_persons` (for list / create / attendance list) and `ensure_client_can_touch_person` (pre-fetches with `include_inactive=true` for `/:id` reads, PATCH, deactivate, allergen mutations, upsert_attendance; batch upsert dedups person_ids before per-row scope check so N identical rows cost one query). Cross-company probes 404 (lie-about-existence); missing `company_id` on a client token is 403 (malformed caller). Error mapping: 23503 / 23514 → 400 — no 23505 branch (persons has no UNIQUE on name; attendance's `(person_id, serving_date)` UNIQUE is resolved by `ON CONFLICT DO UPDATE` inside the query layer). 68 integration tests in `tests/persons.rs` cover list/get-by-id (9) + create/update/deactivate (12) + with-allergens + allergens-get + add/remove/set-allergens (16) + attendance writes (5) + attendance reads (6) + client-scoping (19 Rust-only, mirroring every scope-gated handler including batch cross-company → no partial writes, batch own-only → OK). Migration 016 adds `use_rust_backend_persons`. `src/lib/server/queries/persons.ts` dispatcher adds optional `caller?: InternalCaller` to every exported fn; `serving_date` ISO strings coming off the wire are rehydrated to `Date` via `hydrateAttendance` so TS test `tests/integration/core/persons.test.ts:502`'s `instanceof Date` assertion still holds. 403 TS tests + 68 Rust tests + `pnpm check` all green.
   - **Next sub-sessions** (one per ~200k chunk): users → financials → insights → exports → cutover.
4. **Track 4 (3-5 days, interleaved)** — GHA → GHCR → Railway image deploy; Infisical; 49 svelte-check warnings; `/healthz`; request IDs.
5. **Track 5 (1-2 weeks, after cutover)** — `sqlx::test` + proptest; Playwright E2E; visual regression; real fixtures sanitized from `~/Downloads/WS 26`.
6. **Track 6 (parallel with 5)** — `docs/{ops,auth,excel-import,feature-flags}.md`.

## Non-obvious gotchas
- Migration 007 renames `client_note` → `client` — 15 files already updated to match; don't fight this.
- `MEAL_CATEGORIES` already has `Snack R`; Track 2 adds `Lunch R`, `Soup R`.
- `IGNORED_COLUMNS` in `imports.ts` already contains `Item` and `Quantity to Produce/Ship` — Track 2 renames to `WARN_REDUNDANT_COLUMNS` and promotes to structured warnings.
- Better Auth stores password hash in `account` table (`provider = 'credential'`); no separate password column on `user`.
- Rust workspace migrations: **real copy**, not symlink. `scripts/sync-rust-migrations.sh` (and the Dockerfile) copy `scripts/migrations/*.sql` into `admin-operations-rust/migrations/`; the destination is `.gitignore`d. `sqlx::migrate!` embeds files at compile time and silently sees an empty dir if the symlink points outside the Docker build context.
- Rust toolchain comes from the repo flake (`nix develop`) — `rustup`/`~/.cargo/bin` are gone. Run cargo through `nix develop --command cargo …` or stay inside the dev shell. `pnpm rust:check` and `pnpm rust:test` wrap the common calls (both auto-sync migrations first).
- Normalization boundary is `imports.ts` — downstream code (`src/lib/excel/export/*`, `src/lib/pdf/*`) must not branch on input format.
- Flag-disabled routes return 404 not 403 — don't reveal the feature exists.

## Memory to consult
- `MEMORY.md` index — read everything it links before starting a track.
- `project_prod_readiness_plan.md` — summarizes the active plan.
- `project_resend_domain.md` — magic-link only, non-blocking now; don't delete (dormant path).
- `project_clean_typecheck.md` — handled in Track 4c.

## Next session — Track 3 Step 3 remaining domains

### Correction to earlier handoff drafts
The previous "domain-7 = users" and "feature-flags = query port" entries are **wrong** and should be ignored:
- There is **no** `src/lib/server/queries/users.ts` and **no** `tests/integration/core/users.test.ts`. User-table reads are inlined in two places only: `src/routes/admin/companies/[id]/+page.server.ts` (3 inline queries: list by company, lookup by email, lookup by id+company) and the `JOIN "user"` inside `getCompanyUsers()` in `src/lib/server/queries/companies.ts` (already behind the Rust `companies` path per commit `5fa2d87f`). Porting user admin is a **route-layer refactor** (define `/api/users/*`, convert the 3 inline queries, thread `caller` through the `+page.server.ts` actions) — not the same shape as the other six query-module ports. Save it for last.
- Feature-flags is **already Rust-owned** (`crates/api/src/flags_handler.rs`, migrations 009–010). No TS counterpart exists.

### Actual remaining real targets (sorted by surface size)

| Module | LOC | Consumers | TS tests | Notes |
|---|---|---|---|---|
| `invites.ts` | 49 | 2 routes | **none** — add alongside | 5 fns. Admin-only surface (allergens is the template). `getInviteByToken` is used by the unauthenticated `/invite/[token]` route; that route stays on the SQL path (no caller) in this commit, same precedent as every other route. |
| `financials.ts` | 183 | `/admin/financials` | none | Aggregations only; feature-flag-gated route. Admin-only. |
| `recipe-imports.ts` | 269 | 1 route | none | Writer-heavy, transactional; pairs with the already-ported `imports` path at `crates/api/src/import.rs`. |
| `menus.ts` | 290 | 5 routes | `menus.test.ts` | `setAllMenuSlots` exercises batch-dedup from persons template. Admin + client consumers. |
| `insights.ts` | 384 | `/admin/insights` | `insights.test.ts` | KPI/CUBE-heavy; plan says "rewrite as SQL `WINDOW`/`CUBE`." |
| `imports.ts` | 644 | import route | — | Already has a Rust path via Step 2 (`/api/import/*`); the TS dispatcher wrapper may just need completing — triage case-by-case. |
| **users (route refactor)** | — | `/admin/companies/[id]` | — | **Different pattern.** Define `/api/users/*` for the 3 inline queries + Better Auth `setPassword`-related reset flow. Not a query-module port. Do this **last**. |

### Suggested next order
**invites → financials → recipe-imports → menus → insights → imports (triage) → users (route refactor)**

### Paste verbatim into a fresh session

> Read HANDOFF.md in the repo root. Tracks 1a/1b, 2, and Track 3 Scaffold + Step 2 + Step 3 domains 1-6 (orders, recipes, companies, ingredients, allergens, persons+attendance) are complete and local on main. The earlier handoff drafts named "users" and "feature-flags" as next targets — both are wrong (users has no query module; flags is already Rust-owned). The real next target is **invites** (49 LOC, admin-only, no existing TS test — add one at the same time). Follow the allergens template at `admin-operations-rust/crates/db/src/queries/allergens.rs` + `crates/api/src/allergens.rs` + `crates/api/tests/allergens.rs` + `scripts/migrations/015_use_rust_backend_allergens.sql` + `src/lib/server/queries/allergens.ts` — admin-only, 23505→409 mapping, `require_admin` scope helper. Full function surface: `get_by_token`, `list_by_company`, `create`, `mark_used`, `revoke` (`mark_used` and `revoke` emit the same SQL — both set `used_at = now()`; keep them as separate handlers for API parity with the TS surface). Wire `/api/invites/*` into `crates/api/src/lib.rs` under the `/api` subtree. Add `scripts/migrations/017_use_rust_backend_invites.sql` then `pnpm rust:sync-migrations`. Add `tests/integration/core/invites.test.ts` mirroring the 5 fns (SQL-path only, same TestContext pattern as `allergens.test.ts`). Convert `src/lib/server/queries/invites.ts` into a dispatcher using the existing `_rust-dispatch.ts` (optional `caller?: InternalCaller` on each fn; routes stay untouched — the unauthenticated `/invite/[token]` route uses the SQL path with no caller, consistent with every other route in this commit series). Verify: `pnpm rust:prepare` → `pnpm rust:check` → `SQLX_OFFLINE=true DATABASE_URL="postgresql://postgres:postgres@127.0.0.1:5432/meals_on_the_move" cargo test --manifest-path admin-operations-rust/Cargo.toml --test invites` → `pnpm check` → `pnpm test tests/integration/core/invites.test.ts`. Commit one domain at a time. Ultracode. Ultrathink before acting.
