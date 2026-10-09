# Excel import

Operator-facing. Covers what workbooks the system accepts, what each
warning means, and how to fix a rejected upload.

## Accepted formats

Two shapes, same output:

- **Clean 10-column template.** Headers exactly: `Day, Customer, Recipe,
  Amount, Portion Size, Meal Category, Serving Date, Shipment Date,
  Frozen, Chilled`.
- **Client's real sheet.** Messy: 7 sheets with 6 non-data tabs, an
  extra `Item` column that duplicates `Recipe`, a `Quantity to
  Produce/Ship` spreadsheet formula column, trailing boilerplate rows
  (`PACKING SLIP`, `OFFICE USE ONLY`, `Cambros`, `Packed By`, `#`).

The importer normalises both into the same domain shape at the
boundary — see the 7-step pipeline below.

## The 7-step pipeline

Entry: `plan_import` and `plan_import_pure` at
`admin-operations-rust/crates/excel/src/import.rs:91` and `:159`.

1. **parse** — read the workbook with `calamine`; cells typed as
   string, number, date, bool.
   (`admin-operations-rust/crates/excel/src/parse.rs`)
2. **analyse** — classify header columns, flag the Item + QtyFormula
   warnings at sheet level.
   (`admin-operations-rust/crates/excel/src/headers.rs`)
3. **stop** — truncate at the first boilerplate marker row (see
   list above). Emits `boilerplate_row_stopped`.
   (`import.rs:342` `truncate_at_boilerplate`)
4. **mismatch** — if an `Item` column exists, compare it against
   `Recipe` per row; emit `item_recipe_mismatch` for each divergence.
   (`import.rs:397`)
5. **normalise** — per-row: case-fold meal category, collapse
   `Frozen`/`Chilled` into one `storage` field, trim strings.
   (`admin-operations-rust/crates/excel/src/normalise.rs`)
6. **validate** — per-row: types, required fields, enum values. No DB.
   (`admin-operations-rust/crates/excel/src/validate.rs`)
7. **resolve** — look up `company` + `recipe` by name
   (case-insensitive). Emit a row error if missing or inactive.
   (`import.rs:252`)

`plan_import` prefetches every distinct company and recipe name into a
`MapResolver` so the pure body never touches the pool.

## Warning taxonomy

Non-fatal normalisation notices. Definitions:
`admin-operations-rust/crates/domain/src/import.rs:29`. Codes serialise
as `snake_case` strings and are asserted by test (`:186`).

| Code | Trigger | Operator action |
|---|---|---|
| `extra_sheets_ignored` | Workbook has >1 sheet | None — informational. Only sheet 1 is read. |
| `item_column_ignored` | Sheet has an `Item` column (duplicate of `Recipe`) | None — the column is dropped. |
| `qty_formula_ignored` | Sheet has `Quantity to Produce/Ship` | None — recomputed as `amount × portion_size`. |
| `item_recipe_mismatch` | A row's `Item` ≠ `Recipe` | Verify intent on that row. Recipe wins. |
| `storage_both_set` | A row has `Frozen` **and** `Chilled` marked | Fix the source. Importer defaults to **frozen**. |
| `storage_missing_defaulted_chilled` | A row has neither marked | Fix the source. Importer defaults to **chilled**. |
| `meal_case_normalized` | A row's meal category was case-folded (e.g. `LUnch` → `Lunch`) | None — automatic. |
| `boilerplate_row_stopped` | Reached a packing-slip / office-use marker | Verify no real data lives after the marker. |

## Error taxonomy

Row-level failures. Any one error aborts the whole import — zero rows
are inserted. The kitchen can't partially trust an import; the UI
forces the operator to fix everything and re-upload.

Shape: `admin-operations-rust/crates/domain/src/import.rs:137`.

- `row` — 1-based sheet index; header is row 1, first data row is row 2.
- `field` — set for validation errors (`company`, `recipe`, `amount`,
  …); `None` for row-level problems (not-found/inactive).
- `message` — operator-facing text.
- `suggestion` — populated by fuzzy match when a lookup misses (see
  below). The UI can offer "did you mean X?".

## Preview vs commit

Two endpoints, one pipeline:

- `POST /api/import/preview` — runs the pipeline, discards the result.
  (`admin-operations-rust/crates/api/src/import.rs:31`)
- `POST /api/import/commit` — runs the pipeline; inserts in a single
  transaction when `errors.is_empty()`, else returns 200 with
  `inserted: 0`.
  (`admin-operations-rust/crates/api/src/import.rs:48`)

Multipart contract: field name `file`, `.xlsx` extension required
(extension — not content-type — is trusted; some clients send
`application/octet-stream`), 5 MiB cap. See `import.rs:28` and
`:87-124`.

## Normalisation details that trip operators

- **Meal category case folding.** `LUnch` → `Lunch` + warning. Known
  values live in the normalise module.
- **Storage collapse.** `Frozen` + `Chilled` columns collapse into one
  `storage` field. Both set → warning + default **frozen**. Neither
  set → warning + default **chilled**. Logic: `normalise.rs`.
- **Item vs Recipe.** When both columns are present, the Item column is
  ignored at sheet level (`item_column_ignored`) and compared per row
  (`item_recipe_mismatch`) so divergence surfaces as a warning, not
  silent data loss. Recipe always wins.
- **Qty formula column.** `Quantity to Produce/Ship` is a spreadsheet
  formula column; the importer ignores it and recomputes downstream.
- **Boilerplate stop.** Tokens: `PACKING SLIP`, `OFFICE USE ONLY`,
  `Cambros`, `Packed By`, `#`. First hit truncates the row list;
  trailing rows below the marker are dropped with a warning.
  Match logic: `admin-operations-rust/crates/excel/src/boilerplate.rs`.

## Company + recipe resolution

Exact match first (case-insensitive via the `LOWER(name)` index on the
`company` and `recipe` tables); fuzzy fallback for the suggestion
field. Fuzzy matcher at
`admin-operations-rust/crates/excel/src/fuzzy.rs` — single-row-buffer
Levenshtein, `0.65` similarity threshold, minimum input length 4.
Mirrors the TS matcher so suggestions stay stable through the port.

Inactive companies produce a distinct error message ("inactive and
cannot receive orders") so the operator can tell "unknown" from
"deactivated".

## Determinism and testing

- **Property tests.** `admin-operations-rust/crates/excel/tests/import_proptest.rs`
  sweeps the importer over randomly-shaped workbooks: parser never
  panics, valid rows round-trip, one bad field aborts cleanly.
- **Real-client oracle.**
  `admin-operations-rust/crates/api/tests/import_real_client.rs` runs
  both the messy and sanitized fixture and asserts the exact warning
  taxonomy with specific counts. Fixtures live at
  `tests/integration/excel/fixtures/real-client-week-3{,-clean}.xlsx`
  and are included at compile time (`import_real_client.rs:27`).

**When a real customer sends a new weird workbook shape:** sanitize it,
drop it under `tests/integration/excel/fixtures/`, extend the oracle,
make it fail, then fix the pipeline until it passes. The oracle is the
regression gate.
