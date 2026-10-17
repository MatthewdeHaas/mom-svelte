//! Zero-amount placeholder filter — D1 of
//! `docs/handoff-recipes-importer.md`.
//!
//! The admin workflow pre-populates every active customer × recipe ×
//! day slot and leaves `Amount` at literal `0` for the ones NOT
//! ordered this week. STEP 4 of `plan_import_pure` treats these as
//! placeholders — skip the row, emit `ZeroAmountRowSkipped`.
//!
//! Two invariants:
//!
//! 1. **Filter correctness.** A workbook with one zero-amount row
//!    among valid rows produces inserts for the valid rows, zero
//!    errors, and exactly one `ZeroAmountRowSkipped` warning carrying
//!    the dropped row's sheet row number plus the customer / recipe
//!    for the summary line.
//!
//! 2. **Row-number stability.** When the filter drops a row mid-sheet,
//!    subsequent validation errors must still report their original
//!    sheet row number — "the error says row 42, I open the sheet and
//!    go to row 42" is a user-facing contract that mustn't drift when
//!    rows are filtered during normalisation. The row ordering is
//!    (valid @ row 2, zero @ row 3, invalid @ row 4) and the row-4
//!    error must stay on row 4.

use std::collections::HashMap;

use db::queries::{companies::CompanyRef, recipes::RecipeRef};
use excel::import::{plan_import_pure, MapResolver};
use rust_xlsxwriter::Workbook;
use uuid::Uuid;

const COMPANY_NAME: &str = "Acme Corp";
const RECIPE_NAME: &str = "Caesar Salad";

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

/// Canonical 10-column header — mirrors `HEADERS` in `import_proptest.rs`.
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

/// A row with every validator-visible field set to a known-good value.
/// Callers mutate one field (e.g. `amount = 0.0` or `meal = "Pizza"`)
/// to build the specific case the test needs.
fn valid_row(amount: f64) -> [Cell; 10] {
    [
        Cell::Str(COMPANY_NAME.to_string()),
        Cell::Str(RECIPE_NAME.to_string()),
        Cell::Str("2026-06-10".to_string()),
        Cell::Str("2026-06-08".to_string()),
        Cell::Str("Lunch".to_string()),
        Cell::Num(amount),
        Cell::Blank,
        Cell::Blank,
        Cell::Str("c".to_string()),
        Cell::Blank,
    ]
}

#[derive(Debug, Clone)]
enum Cell {
    Str(String),
    Num(f64),
    Blank,
}

fn build_workbook(rows: &[[Cell; 10]]) -> Vec<u8> {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    for (col, h) in HEADERS.iter().enumerate() {
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
                Cell::Blank => {}
            }
        }
    }
    wb.save_to_buffer().expect("save workbook")
}

#[test]
fn skips_zero_amount_rows_with_warning() {
    let resolver = seed_resolver();

    // Row 2: valid. Row 3: zero-amount placeholder. Row 4: valid.
    let rows = [valid_row(100.0), valid_row(0.0), valid_row(50.0)];
    let bytes = build_workbook(&rows);

    let plan =
        plan_import_pure(&bytes, "test-user", &resolver).expect("plan_import_pure must succeed");

    assert!(
        plan.errors.is_empty(),
        "no errors expected — zero-amount is a warning, not an error. Got: {:?}",
        plan.errors
    );
    assert_eq!(
        plan.orders.len(),
        2,
        "exactly the two non-zero rows should land as inserts"
    );

    let zero_warnings: Vec<&domain::ImportWarning> = plan
        .warnings
        .iter()
        .filter(|w| w.code() == "zero_amount_row_skipped")
        .collect();
    assert_eq!(
        zero_warnings.len(),
        1,
        "exactly one zero_amount_row_skipped warning expected, got {:?}",
        plan.warnings
    );

    // The warning must carry the sheet row number (3), not the loop
    // index. details.customer / details.recipe come from the raw cells.
    let w = zero_warnings[0];
    assert_eq!(w.row(), Some(3), "warning row must be the sheet row");
    if let domain::ImportWarning::ZeroAmountRowSkipped { details, .. } = w {
        let d = details
            .as_ref()
            .expect("details populated with customer+recipe");
        assert_eq!(d["customer"], COMPANY_NAME);
        assert_eq!(d["recipe"], RECIPE_NAME);
    } else {
        unreachable!("filter returned the right code but the wrong variant");
    }
}

#[test]
fn row_numbers_stable_across_zero_amount_skip() {
    let resolver = seed_resolver();

    // Row 2: valid. Row 3: zero-amount (filtered). Row 4: invalid meal.
    // The validator should report the row-4 error on row 4 — NOT on
    // row 3 after the dropped row shifts the index down.
    let mut row_invalid_meal = valid_row(75.0);
    row_invalid_meal[4] = Cell::Str("Pizza".to_string()); // meal column

    let rows = [valid_row(100.0), valid_row(0.0), row_invalid_meal];
    let bytes = build_workbook(&rows);

    let plan =
        plan_import_pure(&bytes, "test-user", &resolver).expect("plan_import_pure must succeed");

    assert!(
        plan.orders.is_empty(),
        "any validator error aborts the import — no inserts"
    );
    assert_eq!(
        plan.errors.len(),
        1,
        "exactly one meal error expected, got {:?}",
        plan.errors
    );
    assert_eq!(
        plan.errors[0].row, 4,
        "row-4 error must stay on row 4 even though row 3 was filtered. \
         If this reports row 3, row numbers are drifting with the normalised vec index."
    );

    // And the dropped row still produces its warning on row 3.
    let zero_warnings: Vec<&domain::ImportWarning> = plan
        .warnings
        .iter()
        .filter(|w| w.code() == "zero_amount_row_skipped")
        .collect();
    assert_eq!(zero_warnings.len(), 1);
    assert_eq!(zero_warnings[0].row(), Some(3));
}
