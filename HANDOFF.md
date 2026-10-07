# Handoff — Production-readiness pass + Rust migration

## Status
Plan at `~/.claude/plans/i-need-to-do-fluttering-hellman.md`. Read it first.

**Branch**: `main`, local, **19 commits** ahead of `origin/main`. **Nothing pushed.**

**Completed commits (local on main):**
- `b31a5cb1` Track 1a — production sheet schema
- `36db1a34` Track 1b — email+password auth, magic-link dormant
- `57f04e4c` Track 2 — reset-test-db.sh env-loading fix
- `575e477a` Track 2 — byte-identical xlsx+pdf exports
- `054e57fe` Track 2 — importer structured warnings
- `d3e96624` Track 2 — real-client fixtures + sanitizer + integration tests
- `aceab7bd` Track 3 scaffold — Rust workspace + HMAC internal auth
- `2d76d76f` Track 3 Step 3 — orders + recipes
- `5fa2d87f` Track 3 Step 3 — companies
- `5fdd04d8` Track 3 Step 3 — ingredients
- `08781dc1` Track 3 Step 3 — allergens
- `2ccd87e7` Track 3 Step 3 — persons + attendance
- `587228b0` Track 3 Step 3 — invites
- `0b3cf33a` Track 3 Step 3 — financials
- `7be4729f` Track 3 Step 3 — recipe-imports (route-layer forwarder; migration 019 applied to **dev** DB, flag flipped ON)
- `88a19124` Track 3 Step 3 — menus + menu_recipe (dispatcher pattern; migration 020 applied to **dev + test**, flag flipped ON; 53/53 Rust tests, 431/431 TS tests OFF+ON)
- `32f65bf1` Track 3 Step 3 — insights (SQL `WINDOW`/`FILTER` rewrite — KPI CTE collapsed 4 parallel queries → 1; `SUM(SUM()) OVER ()` grand total; migration 021 applied + flag ON in dev + test; 27/27 Rust tests, 431/431 TS tests)

## Decisions already made (do not re-ask)
- **Stack**: SvelteKit + pg + Better Auth + ExcelJS + pdfkit. See `memory/project_stack.md`.
- **Hosting**: Live on Railway at `mom-svelte-production.up.railway.app`. No custom domain — client refuses to manage DNS.
- **Auth**: email+password UI-facing; magic-link stays dormant in code for future customer self-serve.
- **Rust**: full migration (not partial). Cargo workspace at `admin-operations-rust/` inside this repo. Separate Railway service, axum, sqlx, calamine, rust_xlsxwriter, printpdf. HMAC-signed internal auth header from SvelteKit → Rust, 60s timestamp window + 60s nonce cache.
- **Excel**: sheet 1 only; sheets 2..N silently ignored. Within-sheet normalization at import boundary. Downstream (PDF, xlsx exports) byte-identical between messy and clean inputs.
- **CI/CD**: GHA → GHCR → Railway "Deploy from Docker Image". Separate `ci.yml` + `release.yml`. No rollback — fix forward.
- **Secrets**: Infisical free tier. Local via `infisical run`, GHA via `Infisical/secrets-action`, Railway via Infisical↔Railway sync.
- **Feature flags**: Flag trait + DB registry + `/admin/flags` UI. Flag-disabled routes `throw error(404)` not 403.
- **End state (user directive, 2026-10-05)**: every TS module with business logic or DB access ports to Rust. TypeScript keeps routes, forms, cookies, Better Auth session, SSR rendering, and HTTP forwarding to Rust — nothing else. At cutover, `src/lib/server/queries/*.ts` are dispatcher stubs or deleted; `src/lib/excel/*` and `src/lib/pdf/*` are gone.

## User preferences (from memory)
- Worktrees for independent tracks (`memory/feedback_worktrees.md`).
- `/brainstorm` proactively on non-trivial tasks (`memory/feedback_brainstorm_proactive.md`).
- Ultracode + ultrathink for substantive work.

## Correction — persist this across sessions
- `src/lib/server/queries/users.ts` does **not** exist and `src/routes/admin/users/` does **not** exist. User-table reads are inlined in `src/routes/admin/companies/[id]/+page.server.ts` only (3 inline queries). Porting user admin is a route-layer refactor, not a query-module port — save it for last.
- Feature-flags is already Rust-owned (`crates/api/src/flags_handler.rs`, migrations 009–010). No TS counterpart.

## Track 3 Step 3 remaining
Rust landed (**11 of 12 domains**): orders, recipes, companies, ingredients, allergens, persons+attendance, invites, financials, recipe-imports, menus, insights.

**TS remaining** in `src/lib/server/queries/`:
- `imports.ts` (~644 LOC — Rust path already exists via Step 2's `/api/import/*`; the TS wrapper may just need converting to dispatcher shape, or may already be fully delegated — triage first). This is the **last** query-module port.

**Then route-layer refactor:**
- Users admin (3 inline queries in `src/routes/admin/companies/[id]/+page.server.ts` + Better Auth `setPassword` flow). Define `/api/users/*`; thread `caller` through actions. Not a query-module port — do this last.

## Track 3 Step 4 — Exports (next big chunk after Step 3)
Port `src/lib/excel/*` and `src/lib/pdf/*` to Rust using `rust_xlsxwriter` + `printpdf`. The byte-identical sha256 tests from Track 2 (`tests/integration/excel/downstream-identical.test.ts`) are the oracle — same input, byte-identical output. Delete the TS versions after parity.

Files to port:
- `src/lib/excel/export/production-order.ts` (~136 LOC)
- `src/lib/excel/export/shopping-list.ts`
- `src/lib/pdf/packing-slip.ts`

`scripts/verify-determinism.ts` (untracked local scratch) is the determinism oracle — fold into the Step 4 exports integration test or keep local.

## Suggested next-session order
**imports (triage) → users (route refactor) → Track 3 Step 4 exports → Track 3 Step 6 cutover (delete all TS business logic) → Track 4 (CI/CD + Infisical + Bun fold-in) → Track 5 (tests) → Track 6 (docs)**

- Migration ledger at **021** (last applied: `use_rust_backend_insights`).

## Non-obvious gotchas
- Migration 007 renamed `client_note` → `client`; `MEAL_CATEGORIES` already has `Snack R`, `Lunch R`, `Soup R`.
- `IGNORED_COLUMNS` in `imports.ts` → renamed to `WARN_REDUNDANT_COLUMNS` + promoted to structured warnings (Track 2).
- Better Auth stores password hash in `account` (`provider = 'credential'`); no column on `user`.
- Rust workspace migrations: **real copy**, not symlink. `pnpm rust:sync-migrations` (and the Dockerfile) copy `scripts/migrations/*.sql` into `admin-operations-rust/migrations/`; destination is gitignored. `sqlx::migrate!` embeds files at compile time and silently sees empty if the symlink points outside the Docker build context. **Add migration files to `scripts/migrations/` only.**
- `admin-operations-rust/.sqlx/` **is committed** — regenerate with `SQLX_OFFLINE=false DATABASE_URL=... cargo sqlx prepare --workspace -- --tests`.
- Rust toolchain is on PATH via the repo flake (direnv-loaded) — run `cargo …` directly, **not** `nix develop --command cargo …`. If a non-interactive shell drops it, fallback: `export PATH="/nix/store/20sx0qpqfhxzq5rn0mhhq9p1kah4j871-rust-default-1.99.0/bin:$PATH"`.
- Postgres is `docker compose up -d db` only (never raw `pg_ctl`). sqlx offline var: `SQLX_OFFLINE=true DATABASE_URL=postgresql://postgres:postgres@localhost:5432/meals_on_the_move`.
- Normalization boundary is `imports.ts` — downstream code must not branch on input format.
- Flag-disabled routes return 404 not 403 — don't reveal the feature exists.
- `_rust-dispatch.ts` **lazy-loads** `api-client.ts` + `logger.ts` because both import `$env/*` / `$app/*` which vitest can't resolve. Top-level static imports would break 6 test suites — keep the dynamic `import()` calls.
- Rust api test harness installs `tracing_subscriber` with `with_test_writer()` — new tests get handler-500 bodies for free.
- `tests/integration/core/financials.test.ts` has a **pre-existing flake** under parallel vitest files sharing `meals_test` (verified against stashed main — ~2/3 of runs fail). Not caused by any Rust port; worth filing but not blocking.
- **`seed_order` bind-order gotcha** (caught during insights port): position 5 is `order_source`, not `status`. Compile-time `query_as!` validates types/nullability but NOT argument semantics; audit bind order when porting seed helpers.
- **Harness Read↔Write race**: Edit/Write immediately after Read of the SAME file in the same tool-use block fails with "File has not been read yet" because the cache update lags the tool_result. Serialize across turns, or bypass with `cat > file <<EOF` heredoc from Bash.
- **Stash-pop restores stale HANDOFF.md**: if a sub-session stashes before working, popping on return will overwrite any orchestrator edits made during the sub-session. Verify HANDOFF after each sub-session returns.

## Memory to consult
- `MEMORY.md` index — read everything it links before starting a track.
- `project_prod_readiness_plan.md` — summarizes the active plan.
- `project_resend_domain.md` — magic-link only, non-blocking now; don't delete (dormant path).
- `project_clean_typecheck.md` — handled in Track 4c.
