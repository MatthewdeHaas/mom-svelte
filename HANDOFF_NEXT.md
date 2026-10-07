# Thin handoff — Track 3 Step 4 (exports to Rust)

**Read first:** `HANDOFF.md` (repo root) and `~/.claude/plans/i-need-to-do-fluttering-hellman.md` (Track 3 Step 4).

## Context

Track 3 Step 3 is **fully complete** (12/12 query-module domains). 19 commits ahead of `origin/main`, nothing pushed. Migration ledger at 021. Last three domain commits:

- `32f65bf1` insights (SQL `WINDOW`/`FILTER` rewrite — KPI CTE collapsed 4 parallel queries → 1; `SUM(SUM()) OVER ()` grand total)
- `88a19124` menus + menu_recipe
- `7be4729f` recipe-imports

Imports was a triage no-op — the Rust path already landed in Step 2 (scaffold `aceab7bd`); `src/routes/admin/orders/import/+page.server.ts:50-125` already forwards to `/api/import/commit` when `use_rust_backend_imports = TRUE` (seeded in migration 010).

**Next real work in Track 3:** port the three export modules to Rust. This is **Step 4**, not another Step 3 domain.

## Scope — port these to Rust

| File | LOC | Signature | Current deps |
|---|---|---|---|
| `src/lib/excel/export/production-order.ts` | 136 | `generateProductionOrderSheet(pool, from, to): Promise<Buffer>` | ExcelJS, `getOrders`, `pinXlsxDates` |
| `src/lib/excel/export/shopping-list.ts` | 136 | `generateShoppingList(pool, from, to): Promise<Buffer>` | ExcelJS, `getOrders`, `getRecipesWithIngredientsByIds` |
| `src/lib/pdf/packing-slip.ts` | 529 | `generatePackingSlips(pool, date): Promise<{...}[]>` | pdfkit, svg-to-pdfkit, static SVG logo, Letter page geometry (612×792 pt), Helvetica/Helvetica-Bold |

**Total: ~801 LOC.** The PDF is the hardest — SVG logo embed, Letter page math, Helvetica metrics.

## Routes that will flip at cutover (do not change in this commit)

- `src/routes/admin/exports/+page.server.ts` (lines 4-6, 40, 50, 60)
- `src/routes/admin/exports/packing-slips/+server.ts` (line 3, 23)
- `src/routes/admin/exports/shopping-list/+server.ts` (line 3, 24)
- `src/routes/admin/exports/production-order/+server.ts` (line 3, 24)

## Target Rust surface

- Crate stubs exist but are empty: `admin-operations-rust/crates/excel/src/lib.rs` (0 LOC), `admin-operations-rust/crates/pdf/src/lib.rs` (1 LOC). Import-side files already live in `crates/excel/src/{headers,normalise,validate,meals,boilerplate,parse,import,recipes,fuzzy}.rs` — don't collide with those.
- Add `crates/excel/src/export/` (new submodule): `production_order.rs`, `shopping_list.rs`. Use `rust_xlsxwriter` (already in `Cargo.toml` per the plan).
- Flesh out `crates/pdf/src/lib.rs` with `packing_slip.rs`. Use `printpdf` (also in plan). SVG logo at `static/mom-logo.svg` — printpdf has `Svg` support; verify it handles the file's path geometry.
- Add `crates/api/src/exports.rs` with handlers:
  - `GET /api/exports/production-order?from=&to=` → xlsx bytes
  - `GET /api/exports/shopping-list?from=&to=` → xlsx bytes
  - `GET /api/exports/packing-slips?date=` → JSON array of `{filename, pdf_base64, meta...}` to match the TS `generatePackingSlips` return shape (lets the TS route zip them before responding). Admin-only — reject non-admin at handler top like `menus.rs` mutations.
- Wire into `crates/api/src/lib.rs` under `/api`.
- Migration **022** `use_rust_backend_exports` (plural — one flag covers all three, same as import side). Add to `scripts/migrations/`; `pnpm rust:sync-migrations` copies into `admin-operations-rust/migrations/` (gitignored).

## The one real architectural decision

**Byte-identical sha256 is the oracle.** `tests/integration/excel/downstream-identical.test.ts` already asserts that two runs over the same semantic data produce byte-identical xlsx + pdf. The TS implementations achieve this via `pinXlsxDates` (`src/lib/excel/determinism.ts`) + fixed sort orders + `scripts/verify-determinism.ts` (local scratch).

`rust_xlsxwriter` and `printpdf` won't produce byte-identical bytes to ExcelJS/pdfkit — the ZIP container ordering, XML attribute ordering, PDF xref table layout, and `/CreationDate` are all implementation-specific. Two options:

1. **Pin Rust output to be byte-identical to the TS output** — doable but expensive: match ExcelJS's exact zip entry order (`[Content_Types].xml` first, specific `xl/` ordering), match pdfkit's object stream layout and xref offsets. Likely requires forking or heavy knob-twisting on both crates.
2. **Switch the oracle to `sha256(semanticExtract(bytes))`** — extract rows from the xlsx (`openpyxl`-style) and text + layout tokens from the PDF (`pdftotext -layout` + optional pdfinfo), hash the extracted content. Two independent generators of the same semantic output then produce the same hash.

**Recommendation: option 2.** Rewriting production tooling to match a 3rd-party library's byte layout is a tar pit; the determinism guarantee that matters is "same semantic input → same rendered output," which the semantic extractor captures. Document the oracle change in the commit so it's not silently relaxed.

If you disagree after reading `determinism.ts` + the test, flag it in the commit message and go with option 1 — don't half-do it.

## Verification before committing

- `cargo build --workspace` clean.
- `cargo test -p api --test exports` — new file; cover each of the 3 handlers + admin-only auth rejection.
- Full TS integration suite flag OFF (default): 431/431 unchanged.
- Apply migration 022 to dev + test DBs, flip flag ON globally, run `tests/integration/excel/downstream-identical.test.ts` **twice** — once with flag OFF (TS→TS determinism), once with flag ON (Rust→Rust determinism). Both must pass. Semantic-oracle rewrite goes in the same commit if that's the path chosen.
- Byte-identical cross-impl parity (TS output = Rust output) is NOT a requirement — the oracle is "same impl, same output, twice." Route switch flips the whole system to Rust at cutover.

## Commit shape

`feat(rust): port exports (production-order + shopping-list + packing-slip) to Rust` — capture: oracle choice (semantic vs. byte-identical), SVG logo embed approach, migration 022 applied + flag flipped ON in dev + test, LOC deleted from TS at cutover is deferred to Step 6. Leave `HANDOFF.md` + `scripts/verify-determinism.ts` + `HANDOFF_NEXT.md` untouched — orchestrator updates them after.

## Gotchas (from recent landings)

- Rust toolchain on PATH via flake — run `cargo …` directly. Fallback: `export PATH="/nix/store/20sx0qpqfhxzq5rn0mhhq9p1kah4j871-rust-default-1.99.0/bin:$PATH"`.
- Postgres: `docker compose up -d db` only. sqlx offline: `SQLX_OFFLINE=true DATABASE_URL=postgresql://postgres:postgres@localhost:5432/meals_on_the_move`.
- `admin-operations-rust/migrations/` is gitignored — add migration to `scripts/migrations/` only, run `pnpm rust:sync-migrations`.
- `admin-operations-rust/.sqlx/` is committed. Regenerate: `SQLX_OFFLINE=false DATABASE_URL=... cargo sqlx prepare --workspace -- --tests`.
- **Do NOT stash HANDOFF.md** before working — stash-pop will revert the orchestrator's edits made in parallel. Leave it untouched.
- Harness Read↔Write race on same tool-use block: Edit after Read of the SAME file fails with "File has not been read yet." Serialize across turns, or use `cat > file <<EOF` heredoc from Bash to bypass.
- `tests/integration/core/financials.test.ts` has a pre-existing flake under parallel vitest (~2/3 fail rate on main). Not caused by any port; re-run to confirm green, don't chase.
- `pnpm check` is already at 0 warnings (`fae13c5c` resolved the 49 Track 4c references). Keep it that way.
- `src/lib/excel/determinism.ts` + the production-order sort key tuple (recipe, company, serving, meal, portion, amount, client, storage) are the determinism contract — mirror the exact sort order in Rust or the semantic hashes drift between impls (if you ever want cross-impl parity for diagnostic purposes).
- `generatePackingSlips` returns an array (one PDF per order), not a single buffer — the API shape must match. Current TS zips them in the route handler, so keeping the handler returning JSON `[{filename, pdf_base64}, …]` keeps the route logic unchanged.

## Report back

- Commit SHA (or "staged, awaiting review").
- Oracle decision: byte-identical pinning vs. semantic extraction. One sentence why.
- Rust test count on `exports` suite.
- Any `rust_xlsxwriter` / `printpdf` surprises (SVG support, font embedding, zip layout).
- Confirmation `downstream-identical.test.ts` passes with flag ON + flag OFF.

Ultracode. Ultrathink before touching the PDF — the Letter geometry + SVG logo + Helvetica metrics is the one place where "it compiled and ran" ≠ "it renders correctly."
