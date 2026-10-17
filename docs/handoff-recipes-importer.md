# Handoff: recipes importer Option A + admin help page

Written 2026-10-17 from branch `recipes/optional-allergens-and-help`
after a long compacted session. Scope is locked by the user; the
earlier session over-explored. This doc is the source of truth for the
next session's work.

## Standing user constraints (preserved verbatim)

- "please don't race reads and writes please"
- "There shouldn't be 'flaky' tests. Unless it's a genuine network
  blip, not a poorly designed test that has a race condition."
- "Run gates yourself" (granted 2026-10-09).
- Cargo: `cd admin-operations-rust && cargo ...`; skip the bash -c
  flake wrapper; don't re-run tests to pretty up output.
- Use Docker for Postgres: `docker compose up -d db`, never raw
  postgres/pg_ctl.
- Use pnpm (not npm).
- Tool-use ordering: Read + Edit/Write of the SAME file must be
  serialized across turns.
- RAILWAY_TOKEN must never be echoed or logged.
- `data/` stays out of git. The three real-client xlsx files there are
  local-only sanity inputs; the generated normalised files also stay
  under `data/`.

## State at handoff

- Branch `recipes/optional-allergens-and-help` off `origin/main`.
- Clean working tree except: `D HANDOFF_CUTOVER.md`,
  `?? admin-operations-rust/.rust-api.pid`,
  `?? admin-operations-rust/crates/excel/examples/`,
  `?? docs/handoff-track8.md`.
- `xlsx-peek` built locally at
  `admin-operations-rust/crates/excel/examples/xlsx-peek.rs` — NOT YET
  COMMITTED. Peeked all three real-client files this session.
- Env fix (`$env/dynamic/private`) landed on
  `scripts/rust-off-docker` as commit `2fb15569`. Independent of this
  work — do not touch.
- PR #16 (`scripts/rust-off-docker`) is open; the env fix is its final
  commit. User is testing it in a fresh shell.

## Facts I verified this session (fresh reads)

- **Prod sheet** (`data/WS 26 Master Prod Sheet Week 3 - June 8(1).xlsx`):
  shape is clean and matches the oracle fixture. 350 rows, 7 sheets
  (6 ignored), split-storage detected, Item + Qty-formula warnings
  are informational. All header mapping succeeds.
- **Recipes sheet** (`data/Recipes MoM.xlsx`): 3 cols, 1,698 rows.
  Hard-aborts with `missing_columns: unit, allergens`. Amount cells
  carry the unit baked in: `"100g"`, `"1.00 ml"`, `"0.10 g"`.
- **SLEC form** (`data/SLEC Order Form-05 January week One.xlsx`):
  explicit scope-out per user ("this will not touch the app. It is
  where the excel sheet data comes from"). Admin uses it to AUTHOR the
  WS 26 prod sheet. **Do not try to import the SLEC shape.**
- The user confirmed the person/allergen/ingredient tables already
  exist. Migration files `015_use_rust_backend_allergens.sql`,
  `016_use_rust_backend_persons.sql`,
  `014_use_rust_backend_ingredients.sql` are present. Read them
  before any schema claim.
- Recipes importer is REAL — not a stub. UI at
  `src/routes/admin/recipes/new/+page.{server.ts,svelte}`. Parser at
  `admin-operations-rust/crates/excel/src/recipes.rs`. Template
  downloader at `src/routes/admin/recipes/new/template/+server.ts`.
  Routes: `POST /api/recipe-imports/preview` + `.../commit`.
- The 87 "amount must be greater than zero" errors on the prod sheet
  are data-level (per-row), not shape-level. Validator at
  `admin-operations-rust/crates/excel/src/validate.rs:211-247` fires
  on `amount <= 0` AND on `amount` being missing/non-numeric. **Not
  yet investigated** — see Deliverable 1 below.

## User-locked decisions

- **Recipes importer — Option A.** Separate `Unit` column (required
  when Ingredient is set, same rule as today). `Allergens` becomes
  optional *when the column is absent* (not per-row — a present column
  with a blank cell still means "no allergens"). Degradation goes:
  present-and-populated > present-and-blank (= no allergens) >
  column-absent (= skip allergen tracking for this upload).
- **Help page.** Single scrollable `/admin/help` page. Walkthrough:
  companies → persons & allergens → recipes & ingredients → orders →
  import. Simple but thorough.
- **Allergen pre-fill (Q1(b)).** Produce `data/Recipes MoM - normalized.xlsx`
  via a Rust example binary. Split `Amount` into Amount+Unit via
  tight regex (`^(\d+(?:\.\d+)?)\s*([a-zA-Z]+)$`). Pre-fill
  `Allergens` from a tiny ~20-30-entry UNAMBIGUOUS exact-match
  whitelist (`Milk` → Dairy, `Egg` → Egg, `Wheat Flour` → Gluten,
  `Peanuts` → Peanuts, `Butter` → Dairy, `Cream` → Dairy, `Cheese` →
  Dairy, `Yogurt` → Dairy, `Soy Sauce` → Soy + Gluten (soy sauce is
  almost always wheat-containing), `Tofu` → Soy, `Shrimp` →
  Shellfish, `Lobster` → Shellfish, `Crab` → Shellfish, `Almonds` →
  Tree Nuts, `Cashews` → Tree Nuts, `Walnuts` → Tree Nuts,
  `Hazelnuts` → Tree Nuts, `Salmon` → Fish, `Tuna` → Fish, `Cod` →
  Fish, `Oats` → Gluten (not technically gluten-containing but
  cross-contamination realities make this the safer default — flag
  this as a judgment call in the tool's stdout audit log so the user
  can override). Everything else stays blank. The whitelist is the
  file; add any additions by editing the whitelist constant, not by
  training a classifier.
- **Scope-out.** SLEC importer. Dashboard changes. Allergen inference
  beyond the whitelist. R-suffix meal variant parsing. Per-person
  attendance tables.

## Deliverables (one PR, this branch)

### D1 — Investigate the 87 zero-amount errors (do first)

Extend `xlsx-peek.rs` with a `--validate` modifier for `--kind orders`
that runs `plan_import_pure` against an empty `MapResolver` and emits
`failing_rows` for every data-level validator rejection. Resolver
misses are filtered out (they depend on DB state and aren't what this
flag characterises). Each row carries `{row, field, message,
raw_value, customer, recipe, amount_cell_raw, amount_cell_type}` with
`amount_cell_type` distinguishing `number_zero` / `string_blank` /
`string` / `null` / `date` / `bool` / `number`.

**Done 2026-10-17.** Shipped as commit 2 in the PR layout. Finding:

- **All 87 are literal `number_zero` cells.** Not blanks, not strings,
  not formulas evaluating to 0, not dates. 100% uniform.
- **Clustered by customer:** SLEC 80, Richmond Hill CHATS 7.
- **Clustered by recipe:** Lentil Soup 18, Applesauce 10, Hummus 9,
  then a long tail of pairs. Batch pattern, not scatter.
- **Row span 13–349**, spread across the sheet with contiguous
  stretches early on.
- Every row has customer, recipe, and dates populated — these are
  structurally valid order rows with Amount intentionally set to `0`.

**Decision (user, 2026-10-17): placeholder filter, WIDE predicate.**
Any row with `Amount == 0` is skipped as a placeholder regardless of
other fields. Matches admin mental model ("if I put 0, I don't want
it"). Follow-up (now in this PR):

- Add `ZeroAmountRowSkipped { row, message, details }` variant to
  `ImportWarning` in `crates/domain/src/import.rs`. Code string
  `zero_amount_row_skipped` (snake_case per existing taxonomy).
  `details` carries `{customer, recipe}` for the import summary.
- Filter at **normalise**, not validate — the warning needs to be
  emitted BEFORE validation runs, and the row must be dropped from
  the normalised vec so `validate_row` never sees it. Simplest shape:
  in `plan_import_pure` STEP 4, after `normalise_row` returns, check
  the amount field on the normalised row; if `Value::Number(0)`, emit
  the warning and skip pushing to the `normalised` vec. Index/row
  numbering stays stable because STEP 5 iterates `normalised` with
  its own enumerate (not the raw-row index).

  Wait — the current STEP 5 passes `row_number = (i as i64) + 2`
  where `i` is the index into `normalised`. If we skip zero-amount
  rows during normalise, row numbers drift (row 14 could become
  row 13 after dropping row 13). That breaks the "the error says
  row 42, I open the sheet and go to row 42" contract.

  Fix: store `(row_number, normalised_row)` tuples instead of a bare
  vec, and compute `row_number` once at normalise time. All
  downstream code uses the stored row_number, never the loop index.
- Keep `validate_row`'s strict `amount > 0` check — defense in depth,
  and the test for "zero amount errors when it slips past the filter"
  stays valid.
- Test: on the prod sheet (via a new integration test that uses the
  xlsx fixture if one exists, or just an inline xlsx bytes constant),
  assert `warnings.iter().filter(|w| w.code() == "zero_amount_row_skipped").count() == 87` and `errors.is_empty()`.

### D2 — Parser change: Allergens column optional when absent

File: `admin-operations-rust/crates/excel/src/recipes.rs`.

- Drop `"allergens"` from `REQUIRED` (line 59).
- Change the `col_header[...]` panics to `.get(...)` with `None`
  fallback for allergens only. Pattern: pre-resolve
  `h_allergens: Option<&String>` instead of `&String`.
- In the row loop, read allergens as
  `h_allergens.and_then(|h| cell_text(raw.get(h)))`.
- When the column IS present, blank cells still mean "no allergens"
  for that row (current behaviour — do not break).
- Add two tests to the `#[cfg(test)]` module:
  - `allergens_column_absent_produces_none_rows`: workbook with
    only Recipe/Ingredient/Amount/Unit headers; a valid row parses
    with `allergens: None`.
  - `allergens_column_present_but_blank_produces_none`: existing
    behaviour, locked in as a test if there isn't one already (grep;
    `parses_happy_row` tests the populated case).

Update `/admin/recipes/new` help copy at
`src/routes/admin/recipes/new/+page.svelte:672-676` (the spec table).
Add a note under the Allergens row: "You can also leave the Allergens
column out entirely." Keep the `badge-opt` badge.

### D3 — Friendlier error when Amount contains a unit

File: `admin-operations-rust/crates/excel/src/recipes.rs::cell_number`
(line 238-257).

When a string cell starts with a number followed by letters
(`"100g"`, `"1.00 ml"`), return a specific error-signal that
`parse_from_workbook` can convert into:

> *"Amount contains a unit (e.g. `100g`); split the number into
> Amount and the unit (e.g. `g`, `ml`) into Unit."*

The current `cell_number` returns `Option<f64>` with no error
signal. Cleanest fix: change the signature to return a small enum
`CellAmount { Ok(f64), WithUnit(String, String), Empty, NotANumber }`
and plumb the two new cases through the row loop. One test for each.

### D4 — Recipes normaliser: `recipes-normalise` example binary

New file: `admin-operations-rust/crates/excel/examples/recipes-normalise.rs`.

Reads a 3-column recipes xlsx (shape: `Recipe, Amount, Ingredient`),
emits a 5-column xlsx alongside with suffix ` - normalized`:

- `Recipe` — pass through verbatim.
- `Ingredient` — pass through verbatim.
- `Amount` — numeric. Split from source string via
  `^(\d+(?:\.\d+)?)\s*([a-zA-Z]+)$`. On mismatch: emit the raw cell
  to stderr with the row number, leave Amount blank in output (the
  parser will then error on that row with "ingredient rows must have
  amount and unit" — the admin will see which rows need their
  attention).
- `Unit` — second regex capture group, lowercased, trimmed.
- `Allergens` — pre-fill from the whitelist in D5. Empty otherwise.

Uses `calamine` for read and `rust_xlsxwriter` for write — both
already in the `excel` crate's deps. Zero new deps.

Writes the output xlsx; does NOT commit it. Writes beside the input,
same directory. On stderr it emits a two-section audit log:

```
=== ambiguous Amount cells (left blank in output) ===
row 234: "a pinch"
row 567: "to taste"
...

=== allergen pre-fills applied ===
row 42: Ingredient "Milk" → Allergens "Dairy"
row 100: Ingredient "Soy Sauce" → Allergens "Soy, Gluten"
...
```

### D5 — Allergen whitelist

Live as a module in the normaliser (NOT in production parser —
whitelist is a tool-side convenience; production is user-authored).
Exact string match on `Ingredient` cell, case-insensitive, no fuzzy.
List from the user's spec above. Document in-file that this is
intentionally narrow because allergens are legal/safety data.

### D6 — `/admin/help` page

Route: `src/routes/admin/help/+page.svelte` (`+page.server.ts` with
auth guard, same shape as other admin pages). Linked from the sidebar
in `src/routes/admin/+layout.svelte` or wherever the admin nav lives
(grep for existing sidebar entries like `Recipes`, `Orders`,
`Companies`).

Content — one scrollable page, headings as sections, concrete
examples throughout. Draft outline (expand with real screenshots or
code snippets where helpful):

1. **Overview** — one paragraph on what the app models and how the
   admin uses it.
2. **Companies** — the top-level customer unit. Create / edit /
   deactivate. Note the active/inactive distinction.
3. **Persons** — individuals inside a company. Hold allergy
   information per person. Link out to the company detail page.
4. **Allergens** — a catalog. Attach to Persons (what they can't eat)
   and to Ingredients (what the ingredient contains). The kitchen
   sees matches.
5. **Recipes** — a named dish with an ingredient list. Allergen
   coverage derives from ingredients.
6. **Ingredients** — the raw material of recipes. Allergen tagging
   lives here.
7. **Orders** — one bulk production line: company × recipe × serving
   date × portion × amount × storage.
8. **Importing** — the weekly workflow:
   - WS 26-style sheet → `/admin/orders` import.
   - Recipes catalog → `/admin/recipes/new`, "Import from
     spreadsheet". Column spec (link down to the template
     download).
   - Warnings vs errors — warnings proceed, errors abort.
9. **Dietary carve-outs** — explain that allergen matching happens
   from both ends (Person has allergies; Ingredient has allergens;
   intersection flags a problem). Note that the SLEC-style
   per-person-per-day sheet is NOT the import format — the admin
   maintains that internally and produces the bulk sheet from it.
10. **Troubleshooting** — common import errors ("Amount must be
    greater than zero" → check for placeholder rows; "Company X not
    found" → create the company first; "Excel template missing
    columns" → re-download the template; the new "Amount contains
    a unit" error from D3).

Style: lean on existing CSS tokens (`--motm-text`, `--motm-pink`,
`--motm-border` — grep for them in sibling pages). No new design
tokens. Readable at 80ch wide. Section anchors so links can jump.

### D7 — Commit `xlsx-peek`

Already written at `admin-operations-rust/crates/excel/examples/xlsx-peek.rs`.
Just needs to be `git add`-ed with D1's `--validate` extension on top.

### D8 — `.gitignore data/`

Add `data/` to the repo root's `.gitignore`.

### D9 — Memory update

New memory: `project_recipes_importer_option_a.md` — Allergens
optional-when-absent landed in PR #<N>; recipes template contract now
accepts 4-col or 5-col sheets. Reference the PR number. Add index
line to `MEMORY.md`.

## PR commit layout

Branch: `recipes/optional-allergens-and-help` (already created,
tracking `origin/main`).

1. `feat(excel): add xlsx-peek example binary` — D7 base
2. `feat(excel): --validate mode for xlsx-peek (dumps failing rows)` — D1
3. `fix(recipes): Allergens column optional when absent` — D2
4. `feat(recipes): friendlier error when Amount contains a unit` — D3
5. `feat(excel): recipes-normalise example binary` — D4 + D5
6. `chore(gitignore): add data/` — D8
7. `feat(admin): /admin/help page` — D6
8. `docs(recipes): update /admin/recipes/new import help copy` — part of D2

PR title: `feat(recipes): optional allergens column + /admin/help page + xlsx tooling`

PR body: Note that the 87-row report from D1 lives in the first
commit's description and may motivate a follow-up (NOT in this PR
unless the finding is clearly "add zero_amount_row_skipped warning").

## Verification plan

| Stage | Command | Expected |
|---|---|---|
| Build | `cd admin-operations-rust && cargo build --release -p excel --example xlsx-peek --example recipes-normalise` | both binaries built |
| D1 | `./admin-operations-rust/target/release/examples/xlsx-peek --kind orders --validate "data/WS 26 Master Prod Sheet Week 3 - June 8(1).xlsx" \| jq '.failing_rows \| length'` | 87 |
| D2 parser tests | `cd admin-operations-rust && cargo test -p excel --lib recipes::` | all green, 2 new tests pass |
| D3 unit test | `cd admin-operations-rust && cargo test -p excel --lib recipes::tests::amount_with_unit` | passes |
| D4 end-to-end | `./admin-operations-rust/target/release/examples/recipes-normalise "data/Recipes MoM.xlsx"` | writes `data/Recipes MoM - normalized.xlsx`; stderr lists ambiguities + prefills |
| D4 round-trip | `./admin-operations-rust/target/release/examples/xlsx-peek --kind recipes "data/Recipes MoM - normalized.xlsx" \| jq '.parse_result.ok'` | `true` |
| D4 upload (manual) | drag the normalised file into `/admin/recipes/new` with mode=add | preview succeeds; fuzzy warnings on duplicate names expected; commit lands |
| Workspace gates | `pnpm check && pnpm run test && (cd admin-operations-rust && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check && cargo test --workspace)` | all green |
| D6 manual | open http://localhost:5173/admin/help | page renders, sections anchor-link, no console errors |

## Known traps from this session

- cwd drifts after `cd admin-operations-rust`. Reset with
  `cd /Users/matthewdehaas/Professional/meals-on-the-move/admin-operations-svelte`
  between unrelated bash blocks or use absolute paths.
- `xlsx-peek` output path is
  `admin-operations-rust/target/release/examples/xlsx-peek` (plural
  `examples`), not `admin-operations-rust/target/release/xlsx-peek`.
- `python3` on this machine does not have `openpyxl`. Do not try to
  pip-install — stay in Rust.
- The user has explicitly scoped out: SLEC importer, dashboard
  changes, R-suffix parser, per-person tables. Do not reopen these
  scopes without being asked.
- The user prefers one tool call at a time with proper
  read-before-write cadence. The previous session over-batched
  (dozens of tool calls per turn) and the user called that out. Match
  pace to the task.

## Thin prompt for the fresh session

Paste this into a new session with the user:

> I'm continuing work from `docs/handoff-recipes-importer.md` on
> branch `recipes/optional-allergens-and-help`. The handoff doc has
> the locked scope, user constraints, and PR layout. Starting with
> D1 — the 87-zero-amount-error investigation — then I'll report
> back before touching parser code. Please confirm the branch and
> I'll proceed.
