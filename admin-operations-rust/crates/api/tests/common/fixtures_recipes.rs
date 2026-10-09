//! In-memory .xlsx fixture builder for the recipe-imports tests.
//!
//! We build each fixture with `rust_xlsxwriter` instead of checking a
//! binary into the repo for two reasons:
//!
//! 1. Tests stay self-describing — one function body declares both the
//!    shape AND the data, so a reviewer can see the inputs without
//!    opening Excel.
//! 2. Fixtures evolve with the parser. If `REQUIRED` ever grows a column,
//!    only this file breaks — not a stale binary blob in git.
//!
//! The round-trip (bytes → multipart → calamine → parser) is byte-faithful
//! because `rust_xlsxwriter` and `calamine` are the same shape of library
//! on either side; both the orders import tests and `excel`'s own unit
//! tests already lean on this.

#![allow(dead_code)] // not all helpers are used by every test

use rust_xlsxwriter::Workbook;

/// Minimal, happy-path workbook: one recipe, one ingredient, one allergen.
/// Headers match the TS parser's `normaliseHeader` tolerance (any casing).
pub fn happy_single_row() -> Vec<u8> {
    build_from_rows(&[&["Chicken Caesar", "Chicken Breast", "200", "g", "Dairy"]])
}

/// Two recipes, each with two ingredients, mixed allergens. Exercises
/// recipe grouping + ingredient dedup + allergen split.
pub fn two_recipes_two_ingredients_each() -> Vec<u8> {
    build_from_rows(&[
        &["Chicken Caesar", "Chicken Breast", "200", "g", "Dairy"],
        &["Chicken Caesar", "Croutons", "30", "g", "Gluten, Dairy"],
        &["Garden Salad", "Lettuce", "100", "g", ""],
        &["Garden Salad", "Tomato", "50", "g", ""],
    ])
}

/// Recipe-only row (no ingredient). The parser accepts this; the
/// orchestrator upserts the recipe and leaves its ingredient list empty.
pub fn recipe_only_row() -> Vec<u8> {
    build_from_rows(&[&["Water", "", "", "", ""]])
}

/// A row with the same ingredient twice under the same recipe — first
/// occurrence wins (TS parity). Second row's amount/unit should NOT
/// appear in the final recipe_ingredient.
pub fn duplicate_ingredient_per_recipe() -> Vec<u8> {
    build_from_rows(&[
        &["Soup", "Water", "500", "ml", ""],
        &["Soup", "Water", "999", "l", ""], // dup — ignored
        &["Soup", "Salt", "5", "g", ""],
    ])
}

/// Mix a valid row and a row with a validation error (ingredient but no
/// unit). The parser emits one `ImportRowError`; both endpoints should
/// 422 and NOT commit anything.
pub fn one_valid_one_invalid_row() -> Vec<u8> {
    build_from_rows(&[
        &["Soup", "Water", "500", "ml", ""],
        &["Soup", "Salt", "5", "", ""], // bad: unit missing
    ])
}

/// A row with a negative amount — parser emits a field-specific error
/// ("Amount must be greater than zero").
pub fn negative_amount_row() -> Vec<u8> {
    build_from_rows(&[&["Soup", "Water", "-10", "ml", ""]])
}

/// Workbook with the five required columns renamed/missing. Specifically:
/// drop `unit` to trigger `ParseRecipeError::MissingColumns`. Hard 400.
pub fn missing_required_column() -> Vec<u8> {
    // No `Unit` column — the required-column check should fire before
    // any row is walked.
    let mut wb = Workbook::new();
    let sheet = wb.add_worksheet();
    for (col, header) in ["Recipe", "Ingredient", "Amount", "Allergens"]
        .iter()
        .enumerate()
    {
        sheet.write(0, col as u16, *header).unwrap();
    }
    sheet.write(1, 0, "Soup").unwrap();
    sheet.write(1, 1, "Water").unwrap();
    sheet.write(1, 2, "500").unwrap();
    sheet.write(1, 3, "").unwrap();
    wb.save_to_buffer().unwrap()
}

/// A workbook whose only data rows are blank → parser returns 0 rows →
/// handler maps to 400 "The spreadsheet contains no data rows."
pub fn only_blank_rows() -> Vec<u8> {
    build_from_rows(&[&["", "", "", "", ""], &["", "", "", "", ""]])
}

/// A fixture suitable for the fuzzy-match test: one input ingredient
/// that is one edit away from a pre-seeded existing ingredient.
///
/// Caller is expected to pre-seed an ingredient named "Chicken Breast"
/// so "Chicken Breasts" (plural) triggers a fuzzy warning.
pub fn fuzzy_match_fixture() -> Vec<u8> {
    build_from_rows(&[&[
        "Chicken Caesar",
        "Chicken Breasts", // 1 char off from the seeded "Chicken Breast"
        "200",
        "g",
        "Dairy",
    ]])
}

// ---------------------------------------------------------------
// Internals
// ---------------------------------------------------------------

/// Build an .xlsx with headers `Recipe/Ingredient/Amount/Unit/Allergens`
/// and the given data rows. Writes `Amount` as a number when it parses,
/// as a string otherwise — matches how a user-authored sheet looks.
fn build_from_rows(rows: &[&[&str]]) -> Vec<u8> {
    let mut wb = Workbook::new();
    let sheet = wb.add_worksheet();

    for (col, header) in ["Recipe", "Ingredient", "Amount", "Unit", "Allergens"]
        .iter()
        .enumerate()
    {
        sheet.write(0, col as u16, *header).unwrap();
    }

    for (ri, row) in rows.iter().enumerate() {
        let r = (ri as u32) + 1; // row 0 is header
        for (ci, cell) in row.iter().enumerate() {
            let c = ci as u16;
            // Amount column — write as number if it parses, as text
            // otherwise so the parser's string-number fallback branch
            // gets exercised too.
            if ci == 2 {
                if let Ok(n) = cell.parse::<f64>() {
                    sheet.write(r, c, n).unwrap();
                    continue;
                }
            }
            sheet.write(r, c, *cell).unwrap();
        }
    }

    wb.save_to_buffer().unwrap()
}
