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
   - **Scaffold complete** (this commit). Workspace: `domain`, `db`, `flags`, `excel`, `pdf`, `api`. HMAC middleware (6-header signed, 60s timestamp window + 60s nonce cache in DashMap, HMAC-before-nonce-insert) with 6 passing unit tests. `/healthz` + `/api/flags` + `/api/flags/check`. SvelteKit signer at `src/lib/server/api-client.ts`. Migration 009 (feature_flag + feature_flag_company, 5 seeded flags). Dockerfile + docker-compose `rust-api` service. Flake-provided Rust toolchain.
   - **Next sub-sessions** (one per ~200k chunk): excel import crate → query layer per-domain behind flags → exports → insights → cutover.
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
