//! Property tests for the Excel importer. Three invariants:
//!
//! 1. **Parser never panics.** Random `(headers, cells)` fed through
//!    `rust_xlsxwriter` → bytes → `plan_import_pure` must either
//!    succeed or return a structured `ImportError::Parse`. Catches
//!    regex / UTF-8 / arithmetic panics in the normaliser.
//!
//! 2. **Valid workbook → full inserts, zero errors.** A random
//!    workbook built from strategies that mirror the Zod schema
//!    produces exactly `row_count` orders with no errors.
//!
//! 3. **One bad field in one row → zero inserts, structured errors.**
//!    Injecting a single validator violation (e.g. `amount = -1.0`,
//!    `meal = "Pizza"`, `storage = "z"`) aborts the whole plan with
//!    at least one `ImportRowError` on a 1-based row number ≥ 2.
//!
//! Target is `plan_import_pure` — a synchronous entry point that
//! takes a `&dyn Resolver` instead of a `PgPool`. This keeps the
//! property body plain blocking Rust (no runtime bridge, no
//! `#[sqlx::test]`) so proptest's shrinker works at full speed.
//! The async wrapper `plan_import` is covered by the real-client
//! oracle in `crates/api/tests/import_real_client.rs`.
//!
//! Case counts are modest (32 per property) — the signal/case ratio
//! for this importer is high and the three-property wall clock is
//! dominated by xlsx (de)serialisation, not case count.

use std::collections::HashMap;

use db::queries::{companies::CompanyRef, recipes::RecipeRef};
use excel::import::{plan_import_pure, ImportError, MapResolver};
use proptest::prelude::*;
use rust_xlsxwriter::Workbook;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Fixtures the three tests share — one company, one recipe. Smaller
// sample space = faster convergence (the real-client oracle already
// covers the "many companies / many recipes" axis).
// ---------------------------------------------------------------------------

const COMPANY_NAME: &str = "Acme Corp";
const RECIPE_NAME: &str = "Caesar Salad";

/// Build a `MapResolver` seeded with exactly one active company and
/// one active recipe. Keys are lowercased to match the production
/// contract (`db::queries::companies::get_many_by_name` returns
/// `LOWER(name)` keys).
fn seed_resolver() -> MapResolver {
    let mut companies: HashMap<String, CompanyRef> = HashMap::new();
    companies.insert(
        COMPANY_NAME.to_lowercase(),
        CompanyRef {
            id: Uuid::new_v4(),
            active: true,
        },
    );
    let mut recipes: HashMap<String, RecipeRef> = HashMap::new();
    recipes.insert(RECIPE_NAME.to_lowercase(), RecipeRef { id: Uuid::new_v4() });
    MapResolver { companies, recipes }
}

// ---------------------------------------------------------------------------
// Workbook builders. All three properties write xlsx bytes with
// `rust_xlsxwriter` so the parser sees a real ZIP-wrapped payload
// (not a hand-rolled byte blob). Metadata is not pinned here — the
// importer doesn't care about determinism of its input.
// ---------------------------------------------------------------------------

/// One row of canonical-schema input. All fields present in valid
/// form; the "one bad field" property mutates exactly one of these
/// in place before writing.
#[derive(Debug, Clone)]
struct ValidRow {
    company: String,
    recipe: String,
    serving_date: String,  // YYYY-MM-DD
    shipment_date: String, // YYYY-MM-DD, <= serving_date
    meal_category: String,
    amount: f64,
    portion_size: Option<f64>,
    day: Option<i32>,
    storage: Option<char>,
    client: Option<String>,
}

/// The canonical 10-column header row. Must stay in sync with the
/// COLUMN_MAP in `excel::headers`.
const HEADERS: &[&str] = &[
    "Customer",
    "Recipe",
    "Serving Date",
    "Shipment Date",
    "Meal",
    "Order Amount",
    "Portion Size",
    "Day",
    "Storage",
    "Client",
];

/// Serialise one row's cells in the order dictated by `HEADERS`.
///
/// Returns `None` entries for fields that should be left blank. The
/// writer dispatches by `Option` + type to pick `write_string` /
/// `write_number` / blank.
fn row_to_cells(r: &ValidRow) -> [Cell; 10] {
    [
        Cell::Str(r.company.clone()),
        Cell::Str(r.recipe.clone()),
        Cell::Str(r.serving_date.clone()),
        Cell::Str(r.shipment_date.clone()),
        Cell::Str(r.meal_category.clone()),
        Cell::Num(r.amount),
        match r.portion_size {
            Some(p) => Cell::Num(p),
            None => Cell::Blank,
        },
        match r.day {
            Some(d) => Cell::Num(d as f64),
            None => Cell::Blank,
        },
        match r.storage {
            Some(c) => Cell::Str(c.to_string()),
            None => Cell::Blank,
        },
        match r.client {
            Some(ref s) => Cell::Str(s.clone()),
            None => Cell::Blank,
        },
    ]
}

/// Cell dispatch helper — `rust_xlsxwriter`'s `write` is generic over
/// the input type, so we fan out through this enum rather than
/// threading `Box<dyn IntoExcelData>` through the generators.
#[derive(Debug, Clone)]
enum Cell {
    Str(String),
    Num(f64),
    Blank,
}

/// Build an xlsx workbook with `HEADERS` as row 1 and `rows` as the
/// data rows. Returns the raw bytes.
fn build_workbook(headers: &[&str], rows: &[[Cell; 10]]) -> Vec<u8> {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    for (col, h) in headers.iter().enumerate() {
        ws.write_string(0, col as u16, *h).expect("write header");
    }
    for (ri, row) in rows.iter().enumerate() {
        let r = (ri + 1) as u32;
        for (ci, cell) in row.iter().enumerate() {
            let c = ci as u16;
            match cell {
                Cell::Str(s) => {
                    ws.write_string(r, c, s).expect("write string");
                }
                Cell::Num(n) => {
                    ws.write_number(r, c, *n).expect("write number");
                }
                Cell::Blank => { /* leave empty — importer sees Null */ }
            }
        }
    }
    wb.save_to_buffer().expect("save workbook")
}

/// Variant for the "parser never panics" property — generates a
/// workbook with ARBITRARY headers (not the canonical set) and
/// arbitrary cell shapes. Many of these won't parse to anything
/// useful, but none should panic.
fn build_arbitrary_workbook(headers: Vec<String>, rows: Vec<Vec<ArbCell>>) -> Vec<u8> {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    for (col, h) in headers.iter().enumerate() {
        ws.write_string(0, col as u16, h).expect("write header");
    }
    for (ri, row) in rows.iter().enumerate() {
        let r = (ri + 1) as u32;
        for (ci, cell) in row.iter().enumerate() {
            let c = ci as u16;
            match cell {
                ArbCell::Str(s) => {
                    ws.write_string(r, c, s).expect("write string");
                }
                ArbCell::Num(n) => {
                    ws.write_number(r, c, *n).expect("write number");
                }
                ArbCell::Bool(b) => {
                    ws.write_boolean(r, c, *b).expect("write bool");
                }
                ArbCell::Blank => {}
            }
        }
    }
    wb.save_to_buffer().expect("save workbook")
}

#[derive(Debug, Clone)]
enum ArbCell {
    Str(String),
    Num(f64),
    Bool(bool),
    Blank,
}

// ---------------------------------------------------------------------------
// Strategies. Kept small — the real-client oracle covers the big
// combinatorial space; these just need to vary each field enough
// that any regression in the pipeline surfaces.
// ---------------------------------------------------------------------------

/// Canonical meal categories — mirror of `excel::meals::MEAL_CATEGORIES`.
const CANONICAL_MEALS: &[&str] = &[
    "Breakfast",
    "Lunch",
    "Lunch R",
    "Snack",
    "Snack R",
    "Soup",
    "Soup R",
    "Dinner",
];

/// ISO date in a narrow band (2024-2027) so shipment <= serving is
/// easy to satisfy without wraparound logic.
fn iso_date_strategy() -> impl Strategy<Value = String> {
    (2024i32..=2027, 1u32..=12, 1u32..=28)
        .prop_map(|(y, m, d)| format!("{:04}-{:02}-{:02}", y, m, d))
}

/// A pair of (serving, shipment) with `shipment <= serving`. Done by
/// generating two dates and swapping if needed — simpler than
/// correlating generators.
fn date_pair_strategy() -> impl Strategy<Value = (String, String)> {
    (iso_date_strategy(), iso_date_strategy()).prop_map(
        |(a, b)| {
            if a >= b {
                (a, b)
            } else {
                (b, a)
            }
        },
    )
}

/// A fully valid row. Draws from the single seeded company + recipe.
fn valid_row_strategy() -> impl Strategy<Value = ValidRow> {
    (
        date_pair_strategy(),
        prop::sample::select(CANONICAL_MEALS.to_vec()),
        1.0f64..=10_000.0,
        prop::option::of(1.0f64..=5_000.0),
        prop::option::of(1i32..=7),
        prop::option::of(prop::sample::select(vec!['f', 'c'])),
        prop::option::of("[A-Za-z ]{1,40}"),
    )
        .prop_map(
            |((serving, shipment), meal, amount, portion, day, storage, client)| ValidRow {
                company: COMPANY_NAME.to_string(),
                recipe: RECIPE_NAME.to_string(),
                serving_date: serving,
                shipment_date: shipment,
                meal_category: meal.to_string(),
                amount,
                portion_size: portion,
                day,
                storage,
                client,
            },
        )
}

/// Which single field to corrupt + what to replace it with. Each
/// variant produces a value guaranteed to fail validation.
#[derive(Debug, Clone)]
enum BadField {
    NegativeAmount,
    // NOTE: `ZeroAmount` was removed from the corruption set — literal
    // `amount == 0` is now a WIDE placeholder filter at normalise time
    // (STEP 4 in `plan_import_pure`) that emits a
    // `ZeroAmountRowSkipped` warning and drops the row. The row never
    // reaches `validate_row`, so a workbook with every row set to
    // amount=0 produces `orders.is_empty() && errors.is_empty()` —
    // which would break this property's "at least one error" invariant.
    // See D1 of `docs/handoff-recipes-importer.md` and the dedicated
    // filter tests in `tests/zero_amount_filter.rs`.
    NonNumericAmount,
    BadMeal,
    BadServingDate,
    BadShipmentDate,
    ShipmentAfterServing,
    DayOutOfRange,
    FractionalDay,
    EmptyCompany,
    EmptyRecipe,
}

fn bad_field_strategy() -> impl Strategy<Value = BadField> {
    prop_oneof![
        Just(BadField::NegativeAmount),
        Just(BadField::NonNumericAmount),
        Just(BadField::BadMeal),
        Just(BadField::BadServingDate),
        Just(BadField::BadShipmentDate),
        Just(BadField::ShipmentAfterServing),
        Just(BadField::DayOutOfRange),
        Just(BadField::FractionalDay),
        Just(BadField::EmptyCompany),
        Just(BadField::EmptyRecipe),
    ]
}

/// Apply the corruption. Returns the mutated row and a cell-override
/// map for fields that need a non-standard shape (e.g. a string in
/// the amount cell).
fn corrupt_row(row: &mut ValidRow, bad: &BadField) -> Option<(usize, Cell)> {
    match bad {
        BadField::NegativeAmount => {
            row.amount = -1.0;
            None
        }
        BadField::NonNumericAmount => {
            // Can't fit a string in the f64 field; return a cell
            // override at the amount column (index 5).
            Some((5, Cell::Str("not a number".to_string())))
        }
        BadField::BadMeal => {
            row.meal_category = "Pizza".to_string();
            None
        }
        BadField::BadServingDate => {
            row.serving_date = "not-a-date".to_string();
            None
        }
        BadField::BadShipmentDate => {
            row.shipment_date = "2026/06/08".to_string();
            None
        }
        BadField::ShipmentAfterServing => {
            // Force shipment one year after serving (shipment > serving fails).
            row.serving_date = "2026-01-01".to_string();
            row.shipment_date = "2027-01-01".to_string();
            None
        }
        BadField::DayOutOfRange => {
            row.day = Some(99);
            None
        }
        BadField::FractionalDay => {
            // Override the Day cell (index 7) with a fractional number.
            Some((7, Cell::Num(2.5)))
        }
        BadField::EmptyCompany => {
            row.company = String::new();
            None
        }
        BadField::EmptyRecipe => {
            row.recipe = String::new();
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Property 1 — parser never panics on random valid xlsx bytes.
// ---------------------------------------------------------------------------

#[test]
fn parser_never_panics_on_random_shapes() {
    let resolver = seed_resolver();

    // Arbitrary header set — some may collide with canonical names,
    // most won't. 1-6 headers, 0-4 rows.
    let header_strat = prop::collection::vec("[A-Za-z0-9 /_]{1,20}", 1..=6);

    proptest!(ProptestConfig::with_cases(32), |(headers in header_strat, row_count in 0usize..=4)| {
        // Build rows after we know the header width. Deterministic
        // Blank fill — the parser-panic property doesn't need dense
        // random content; headers + empty rows already exercise the
        // parse pipeline. The denser random-cell sweep below hits
        // the normaliser + validator paths.
        let width = headers.len();
        let mut rows: Vec<Vec<ArbCell>> = Vec::with_capacity(row_count);
        for _ in 0..row_count {
            let row = (0..width).map(|_| ArbCell::Blank).collect();
            rows.push(row);
        }

        let bytes = build_arbitrary_workbook(headers, rows);
        let result = plan_import_pure(&bytes, "test-user", &resolver);
        // The point: no panic. Any Ok or structured Err is acceptable.
        prop_assert!(matches!(result, Ok(_) | Err(ImportError::Parse(_)) | Err(ImportError::Db(_))));
    });

    // Second sweep — canonical headers + random cell soup. Exercises
    // the normaliser and validator with every row-level cell shape.
    let canonical_headers: Vec<String> = HEADERS.iter().map(|h| h.to_string()).collect();
    let cell_strat = prop_oneof![
        "[A-Za-z0-9 \\-/]{0,30}".prop_map(ArbCell::Str),
        (-1_000.0f64..=1_000.0).prop_map(ArbCell::Num),
        any::<bool>().prop_map(ArbCell::Bool),
        Just(ArbCell::Blank),
    ];
    let rows_strat = prop::collection::vec(prop::collection::vec(cell_strat, 10..=10), 0..=4);
    proptest!(ProptestConfig::with_cases(32), |(rows in rows_strat)| {
        let bytes = build_arbitrary_workbook(canonical_headers.clone(), rows);
        let result = plan_import_pure(&bytes, "test-user", &resolver);
        prop_assert!(matches!(result, Ok(_) | Err(ImportError::Parse(_)) | Err(ImportError::Db(_))));
    });
}

// ---------------------------------------------------------------------------
// Property 2 — valid workbook → full inserts, zero errors.
// ---------------------------------------------------------------------------

#[test]
fn valid_workbook_imports_fully() {
    let resolver = seed_resolver();

    let rows_strat = prop::collection::vec(valid_row_strategy(), 1..=8);

    proptest!(ProptestConfig::with_cases(32), |(rows in rows_strat)| {
        let row_count = rows.len();
        let cells: Vec<[Cell; 10]> = rows.iter().map(row_to_cells).collect();
        let bytes = build_workbook(HEADERS, &cells);

        let plan = plan_import_pure(&bytes, "test-user", &resolver)
            .expect("plan_import_pure must succeed on valid input");

        // The handoff spec: valid workbook → full inserts, zero errors.
        // That's all we assert. Cross-row ordering, amount round-trip,
        // and `plan.row_count` details are the real-client oracle's
        // job — asserting them here just overfits the property to the
        // current normaliser.
        prop_assert_eq!(plan.errors.len(), 0, "no errors expected for valid workbook");
        prop_assert_eq!(plan.orders.len(), row_count, "one InsertOrder per input row");
    });
}

// ---------------------------------------------------------------------------
// Property 3 — one bad field in one row → zero inserts, structured errors.
// ---------------------------------------------------------------------------

#[test]
fn one_bad_field_zero_inserts() {
    let resolver = seed_resolver();

    // 1-6 valid rows + an index + a corruption. The corruption lands
    // on exactly one row; everything else stays valid.
    let strat = (
        prop::collection::vec(valid_row_strategy(), 1..=6),
        bad_field_strategy(),
    )
        .prop_flat_map(|(rows, bad)| {
            let len = rows.len();
            (Just(rows), 0usize..len, Just(bad))
        });

    proptest!(ProptestConfig::with_cases(32), |((rows, bad_idx, bad) in strat)| {
        let total_rows = rows.len();
        let mut rows = rows;
        let override_cell = corrupt_row(&mut rows[bad_idx], &bad);

        let mut cells: Vec<[Cell; 10]> = rows.iter().map(row_to_cells).collect();
        if let Some((col, cell)) = override_cell {
            cells[bad_idx][col] = cell;
        }

        let bytes = build_workbook(HEADERS, &cells);
        let plan = plan_import_pure(&bytes, "test-user", &resolver)
            .expect("plan_import_pure must succeed on structured-error input");

        prop_assert_eq!(plan.orders.len(), 0, "zero inserts when any row is bad");
        prop_assert!(!plan.errors.is_empty(), "at least one structured error");
        prop_assert!(
            plan.errors.len() <= total_rows,
            "errors cannot exceed data-row count"
        );

        // Every error must carry a 1-based sheet row number >= 2
        // (header is row 1).
        for err in &plan.errors {
            prop_assert!(err.row >= 2, "row number must be a 1-based data row");
            prop_assert!(
                err.row <= (total_rows as i64) + 1,
                "row number must not exceed last data row"
            );
        }
    });
}
