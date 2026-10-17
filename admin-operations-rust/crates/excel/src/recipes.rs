//! Recipe-xlsx parser. Mirrors `src/lib/excel/parse-recipes.ts` 1:1 —
//! same header normalisation, same required-column list, same Zod-
//! equivalent per-row validation, same blank-row skip rule.
//!
//! The parser is deliberately thin: it builds a header map off row 1,
//! walks data rows, coerces cells, and emits either a validated
//! `RecipeExcelRowParsed` or an `ImportRowError`. Nothing else — no DB
//! lookups, no fuzzy matching, no transaction. The orchestrator
//! (`db::queries::recipe_imports::import_recipes_from_rows`) owns
//! those; the HTTP layer composes them.
//!
//! Reuses `parse::parse_workbook` for the calamine → `ParsedWorkbook`
//! adapter so the two importers share identical cell-coercion rules —
//! the only wrinkle is that the orders importer keeps the ORIGINAL
//! header casing on each row map while this one normalises headers to
//! `trim/lower/underscore` for the column lookup. We get the ordered
//! `headers` list off the parsed workbook and build our own
//! normalised-key column index.
//!
//! Error shape: fields `{row, field, message, raw_value, severity,
//! suggestion}` match `domain::import::ImportRowError` verbatim. For
//! parser-emitted errors we set `field: Some(path)` and
//! `severity: Some("error")`; everything else is `None`. Row numbers
//! are 1-based (header row = 1, first data row = 2) to match TS.
//!
//! Missing columns are a HARD error, not a per-row one — the TS parser
//! throws before iterating rows. We surface that as
//! `ParseRecipeError::MissingColumns(Vec<String>)`; the handler maps it
//! to 400 with the comma-joined names so the user sees the same
//! message as today.

use std::collections::HashMap;

use domain::import::ImportRowError;
use domain::recipes::{ParseRecipesResult, RecipeExcelRowParsed};

use crate::parse::{parse_workbook, Cell, ParsedWorkbook};

/// Hard failures from the recipe parser. Row-level validation lands in
/// `ParseRecipesResult::errors` and does NOT produce this type.
#[derive(Debug, thiserror::Error)]
pub enum ParseRecipeError {
    /// calamine couldn't open the workbook or read sheet 1.
    #[error("workbook parse failed: {0}")]
    Workbook(#[from] crate::parse::ParseError),

    /// Row 1 is empty or sheet 1 is missing — matches the TS throw
    /// `"Excel file contains no worksheets"` plus the empty-header case.
    #[error("Excel file contains no worksheets")]
    NoSheet,

    /// One or more required columns are absent from the header row.
    /// Message matches TS: `"Excel template missing columns: a, b"`.
    #[error("Excel template missing columns: {0}")]
    MissingColumns(String),
}

/// Required header names, in TS order. Any missing one aborts the parse.
///
/// `allergens` is intentionally NOT in this list — it's optional at the
/// column level. When the column is absent, every row's `allergens`
/// field comes out as `None` and ingredient-allergen tracking is simply
/// not touched by this upload. When the column IS present, blank cells
/// still mean "no allergens for this row" (same as today). The three-
/// level degradation is: present-and-populated beats present-and-blank
/// (= no allergens) beats column-absent (= skip allergen tracking for
/// this upload). See D2 of `docs/handoff-recipes-importer.md` for the
/// user decision.
const REQUIRED: &[&str] = &["recipe", "ingredient", "amount", "unit"];

/// Normalise a header the same way `normaliseHeader` does in TS:
/// trim, lowercase, collapse runs of whitespace to a single underscore.
///
/// `\s+` in the JS regex matches ASCII + Unicode whitespace; we stay
/// ASCII-only here because every header on real fixtures is plain ASCII
/// and the recipe template is admin-controlled.
fn normalise_header(raw: &str) -> String {
    let trimmed = raw.trim().to_ascii_lowercase();
    // Collapse any run of ASCII whitespace into a single `_`.
    let mut out = String::with_capacity(trimmed.len());
    let mut in_ws = false;
    for ch in trimmed.chars() {
        if ch.is_ascii_whitespace() {
            if !in_ws {
                out.push('_');
                in_ws = true;
            }
        } else {
            out.push(ch);
            in_ws = false;
        }
    }
    out
}

/// Public entry point. Byte-identical semantics to `parseRecipeExcel`.
///
/// Returns a `ParseRecipesResult` on success — `rows` and `errors` may
/// both be non-empty; the caller aborts the import when `errors` is
/// non-empty (TS parity). `ParseRecipeError` is reserved for shape-of-
/// workbook failures (bad xlsx, missing columns) that stop parsing
/// before we can produce a result at all.
pub fn parse_recipe_xlsx(bytes: &[u8]) -> Result<ParseRecipesResult, ParseRecipeError> {
    let wb = parse_workbook(bytes)?;
    parse_from_workbook(&wb)
}

/// Testable core — splits workbook parse from row walk so unit tests
/// can feed in a fabricated `ParsedWorkbook` without a real .xlsx.
fn parse_from_workbook(wb: &ParsedWorkbook) -> Result<ParseRecipesResult, ParseRecipeError> {
    if wb.headers.is_empty() {
        return Err(ParseRecipeError::NoSheet);
    }

    // Build the normalised-key → ORIGINAL-header map so we can look up
    // cells on the parsed row (which is keyed by original header).
    let mut col_header: HashMap<String, String> = HashMap::with_capacity(wb.headers.len());
    for h in &wb.headers {
        if h.is_empty() {
            continue;
        }
        col_header.insert(normalise_header(h), h.clone());
    }

    // Missing-columns check — TS throws before touching any data row.
    let missing: Vec<&str> = REQUIRED
        .iter()
        .filter(|k| !col_header.contains_key(**k))
        .copied()
        .collect();
    if !missing.is_empty() {
        return Err(ParseRecipeError::MissingColumns(missing.join(", ")));
    }

    // Pre-resolve the original headers once — one hashmap lookup per
    // row per column beats re-resolving each iteration. `allergens` is
    // optional at the column level: when the sheet omits it, the row
    // loop below reads `None` for every row's allergens cell, and the
    // downstream orchestrator skips allergen tracking entirely for this
    // upload. See the `REQUIRED` comment above for the full three-level
    // degradation contract.
    let h_recipe = &col_header["recipe"];
    let h_ingredient = &col_header["ingredient"];
    let h_amount = &col_header["amount"];
    let h_unit = &col_header["unit"];
    let h_allergens: Option<&String> = col_header.get("allergens");

    let mut rows: Vec<RecipeExcelRowParsed> = Vec::with_capacity(wb.rows.len());
    let mut errors: Vec<ImportRowError> = Vec::new();

    for (i, raw) in wb.rows.iter().enumerate() {
        // Header row is row 1; first data row is row 2.
        let row_number = (i as i64) + 2;

        let recipe = cell_text(raw.get(h_recipe));
        let ingredient = cell_text(raw.get(h_ingredient));
        let amount = cell_number(raw.get(h_amount));
        let unit = cell_text(raw.get(h_unit));
        // Allergens column is optional at the sheet level. When absent,
        // every row reports `None` here; when present-but-blank, the row
        // reports `None` too — same wire shape, same downstream handling.
        let allergens = h_allergens.and_then(|h| cell_text(raw.get(h)));

        // Skip blank rows (no recipe AND no ingredient). Matches TS.
        if recipe.is_none() && ingredient.is_none() {
            continue;
        }

        // Validation rules, mirroring the Zod schema + superRefine:
        //   1. `recipe` required, non-empty
        //   2. `amount` (if present) must be a positive number
        //   3. if `ingredient` is set → `amount` AND `unit` required
        //
        // TS collects issues into `error.issues`, picks the first path
        // as `field`, and joins all messages with `"; "`. Replicate.
        let mut issues: Vec<(&'static str, &'static str)> = Vec::new();

        if recipe.is_none() {
            issues.push(("recipe", "Recipe name is required"));
        }
        if let Some(a) = amount {
            if !a.is_finite() || a <= 0.0 {
                issues.push(("amount", "Amount must be greater than zero"));
            }
        }
        if ingredient.is_some() {
            let has_amount = amount.is_some();
            let has_unit = unit.is_some();
            if !has_amount || !has_unit {
                // TS attaches this to the root path (`""`) via custom
                // issue; its `.path.join('.')` evaluates to `""`, which
                // we represent as `"unknown"` to match the TS fallback
                // `?? 'unknown'`. The TS code checks for a non-empty
                // path first — the custom issue above has an empty path
                // array, so the join is `""` and the `??` picks
                // "unknown". We match that behaviour precisely.
                issues.push(("", "Ingredient rows must have amount and unit"));
            }
        }

        if !issues.is_empty() {
            // field = first issue's path, or "unknown" if blank.
            let field = {
                let first = issues[0].0;
                if first.is_empty() {
                    "unknown".to_string()
                } else {
                    first.to_string()
                }
            };
            let message = issues
                .iter()
                .map(|(_, m)| *m)
                .collect::<Vec<_>>()
                .join("; ");
            errors.push(ImportRowError {
                row: row_number,
                field: Some(field),
                message,
                raw_value: None,
                severity: Some("error".to_string()),
                suggestion: None,
            });
            continue;
        }

        // All checks passed — unwrap is safe because `recipe` was
        // required and `issues` would have fired otherwise.
        rows.push(RecipeExcelRowParsed {
            recipe: recipe.expect("recipe is Some when no issues"),
            ingredient,
            amount,
            unit,
            allergens,
        });
    }

    Ok(ParseRecipesResult { rows, errors })
}

/// Trim-to-non-empty on a string-shaped cell. Numbers, bools, nulls
/// return `None`. Matches TS `cellText(cell)`.
fn cell_text(cell: Option<&Cell>) -> Option<String> {
    let s = cell?.as_str_like()?;
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

/// Parse a numeric cell. Numbers pass through; strings try `parse::<f64>`;
/// everything else is `None`. Matches TS `cellNumber(cell)` (which
/// `Number(v)` + NaN check — strings numeric or not, both work).
fn cell_number(cell: Option<&Cell>) -> Option<f64> {
    match cell? {
        Cell::Number(n) => {
            if n.is_finite() {
                Some(*n)
            } else {
                None
            }
        }
        Cell::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                t.parse::<f64>().ok().filter(|n| n.is_finite())
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_wb(headers: &[&str], rows: &[Vec<(&str, Cell)>]) -> ParsedWorkbook {
        let headers_vec: Vec<String> = headers.iter().map(|h| h.to_string()).collect();
        let rows_vec: Vec<HashMap<String, Cell>> = rows
            .iter()
            .map(|r| r.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
            .collect();
        ParsedWorkbook {
            headers: headers_vec,
            rows: rows_vec,
            extra_sheet_count: 0,
        }
    }

    #[test]
    fn normalises_header_matches_ts_rules() {
        assert_eq!(normalise_header(" Recipe "), "recipe");
        assert_eq!(normalise_header("Ingredient Name"), "ingredient_name");
        assert_eq!(normalise_header("ALLERGENS"), "allergens");
        assert_eq!(normalise_header("  Spaces   Here"), "spaces_here");
    }

    #[test]
    fn missing_columns_error_lists_all() {
        let wb = mk_wb(&["Recipe", "Ingredient"], &[]);
        let err = parse_from_workbook(&wb).unwrap_err();
        match err {
            ParseRecipeError::MissingColumns(s) => {
                assert!(s.contains("amount"));
                assert!(s.contains("unit"));
                // `allergens` is intentionally NOT required at the
                // column level — see D2 of
                // `docs/handoff-recipes-importer.md`. Its absence must
                // not surface here.
                assert!(
                    !s.contains("allergens"),
                    "allergens must not appear in missing-columns error: {s:?}"
                );
            }
            _ => panic!("expected MissingColumns, got {err:?}"),
        }
    }

    #[test]
    fn allergens_column_absent_produces_none_rows() {
        // Four-column sheet — no Allergens header at all. Valid rows
        // parse with `allergens: None`; missing-columns error does NOT
        // fire for allergens.
        let wb = mk_wb(
            &["Recipe", "Ingredient", "Amount", "Unit"],
            &[vec![
                ("Recipe", Cell::String("Caesar".into())),
                ("Ingredient", Cell::String("Chicken".into())),
                ("Amount", Cell::Number(200.0)),
                ("Unit", Cell::String("g".into())),
            ]],
        );
        let result = parse_from_workbook(&wb).expect("parse must succeed without Allergens column");
        assert_eq!(result.errors.len(), 0);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].allergens, None);
    }

    #[test]
    fn allergens_column_present_but_blank_produces_none() {
        // Five-column sheet with a blank Allergens cell — same wire
        // shape as the absent-column case (`None`), different source of
        // truth. Locked in as a regression test so the two paths don't
        // drift.
        let wb = mk_wb(
            &["Recipe", "Ingredient", "Amount", "Unit", "Allergens"],
            &[vec![
                ("Recipe", Cell::String("Caesar".into())),
                ("Ingredient", Cell::String("Chicken".into())),
                ("Amount", Cell::Number(200.0)),
                ("Unit", Cell::String("g".into())),
                ("Allergens", Cell::Null),
            ]],
        );
        let result = parse_from_workbook(&wb).unwrap();
        assert_eq!(result.errors.len(), 0);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].allergens, None);
    }

    #[test]
    fn parses_happy_row() {
        let wb = mk_wb(
            &["Recipe", "Ingredient", "Amount", "Unit", "Allergens"],
            &[vec![
                ("Recipe", Cell::String("Caesar".into())),
                ("Ingredient", Cell::String("Chicken".into())),
                ("Amount", Cell::Number(200.0)),
                ("Unit", Cell::String("g".into())),
                ("Allergens", Cell::String("Dairy, Gluten".into())),
            ]],
        );
        let result = parse_from_workbook(&wb).unwrap();
        assert_eq!(result.errors.len(), 0);
        assert_eq!(result.rows.len(), 1);
        let r = &result.rows[0];
        assert_eq!(r.recipe, "Caesar");
        assert_eq!(r.ingredient.as_deref(), Some("Chicken"));
        assert_eq!(r.amount, Some(200.0));
        assert_eq!(r.unit.as_deref(), Some("g"));
        assert_eq!(r.allergens.as_deref(), Some("Dairy, Gluten"));
    }

    #[test]
    fn skips_blank_rows() {
        // A row with every cell empty is dropped silently — TS parity.
        let wb = mk_wb(
            &["Recipe", "Ingredient", "Amount", "Unit", "Allergens"],
            &[
                vec![
                    ("Recipe", Cell::Null),
                    ("Ingredient", Cell::Null),
                    ("Amount", Cell::Null),
                    ("Unit", Cell::Null),
                    ("Allergens", Cell::Null),
                ],
                vec![("Recipe", Cell::String("Only name".into()))],
            ],
        );
        let result = parse_from_workbook(&wb).unwrap();
        assert_eq!(result.errors.len(), 0);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].recipe, "Only name");
    }

    #[test]
    fn negative_amount_errors_on_row() {
        let wb = mk_wb(
            &["Recipe", "Ingredient", "Amount", "Unit", "Allergens"],
            &[vec![
                ("Recipe", Cell::String("R".into())),
                ("Ingredient", Cell::String("I".into())),
                ("Amount", Cell::Number(-1.0)),
                ("Unit", Cell::String("g".into())),
            ]],
        );
        let result = parse_from_workbook(&wb).unwrap();
        assert_eq!(result.rows.len(), 0);
        assert_eq!(result.errors.len(), 1);
        let e = &result.errors[0];
        // Row 1 is header → data row is row 2.
        assert_eq!(e.row, 2);
        assert_eq!(e.field.as_deref(), Some("amount"));
        assert_eq!(e.severity.as_deref(), Some("error"));
        assert!(e.message.contains("greater than zero"));
    }

    #[test]
    fn ingredient_without_unit_errors_with_unknown_field() {
        let wb = mk_wb(
            &["Recipe", "Ingredient", "Amount", "Unit", "Allergens"],
            &[vec![
                ("Recipe", Cell::String("R".into())),
                ("Ingredient", Cell::String("I".into())),
                ("Amount", Cell::Number(100.0)),
                // Unit missing
            ]],
        );
        let result = parse_from_workbook(&wb).unwrap();
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].field.as_deref(), Some("unknown"));
        assert!(result.errors[0]
            .message
            .contains("Ingredient rows must have amount and unit"));
    }

    #[test]
    fn recipe_only_row_passes() {
        // No ingredient → amount/unit not required.
        let wb = mk_wb(
            &["Recipe", "Ingredient", "Amount", "Unit", "Allergens"],
            &[vec![("Recipe", Cell::String("R".into()))]],
        );
        let result = parse_from_workbook(&wb).unwrap();
        assert_eq!(result.errors.len(), 0);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].ingredient, None);
    }

    #[test]
    fn missing_recipe_name_errors() {
        let wb = mk_wb(
            &["Recipe", "Ingredient", "Amount", "Unit", "Allergens"],
            &[vec![
                // No Recipe but has Ingredient → row isn't blank, so it hits validation.
                ("Ingredient", Cell::String("I".into())),
                ("Amount", Cell::Number(1.0)),
                ("Unit", Cell::String("g".into())),
            ]],
        );
        let result = parse_from_workbook(&wb).unwrap();
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].field.as_deref(), Some("recipe"));
    }

    #[test]
    fn case_insensitive_column_lookup() {
        let wb = mk_wb(
            &["RECIPE", "iNgReDiEnT", "amount", "UNIT", "Allergens"],
            &[vec![
                ("RECIPE", Cell::String("R".into())),
                ("iNgReDiEnT", Cell::String("I".into())),
                ("amount", Cell::Number(5.0)),
                ("UNIT", Cell::String("g".into())),
                ("Allergens", Cell::Null),
            ]],
        );
        let result = parse_from_workbook(&wb).unwrap();
        assert_eq!(result.errors.len(), 0);
        assert_eq!(result.rows.len(), 1);
    }

    #[test]
    fn string_number_parses_as_amount() {
        // ExcelJS reads some numeric cells as strings; TS `Number("5")` works.
        let wb = mk_wb(
            &["Recipe", "Ingredient", "Amount", "Unit", "Allergens"],
            &[vec![
                ("Recipe", Cell::String("R".into())),
                ("Ingredient", Cell::String("I".into())),
                ("Amount", Cell::String("5".into())),
                ("Unit", Cell::String("g".into())),
            ]],
        );
        let result = parse_from_workbook(&wb).unwrap();
        assert_eq!(result.errors.len(), 0);
        assert_eq!(result.rows[0].amount, Some(5.0));
    }
}
