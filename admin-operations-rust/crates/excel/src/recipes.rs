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
        let amount_cell = cell_number(raw.get(h_amount));
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
        let mut issues: Vec<(&'static str, String)> = Vec::new();

        if recipe.is_none() {
            issues.push(("recipe", "Recipe name is required".to_string()));
        }

        // Amount validation branches on CellAmount's four variants:
        //   Ok(n)             — must be positive; otherwise "greater than zero"
        //   WithUnit(_, unit) — specific "contains a unit" error (D3)
        //   NotANumber        — generic "not a number" error
        //   Empty             — not an error here; the "amount required
        //                       when ingredient is set" check below fires
        //                       instead so the message stays identical
        //                       to the TS behaviour.
        let amount: Option<f64> = match &amount_cell {
            CellAmount::Ok(n) => {
                if !n.is_finite() || *n <= 0.0 {
                    issues.push(("amount", "Amount must be greater than zero".to_string()));
                    None
                } else {
                    Some(*n)
                }
            }
            CellAmount::WithUnit(_n, unit_text) => {
                // D3: friendlier error for `"100g"` / `"1.00 ml"` /
                // `"0.10 g"`-shaped cells. Points directly at the fix
                // instead of letting the row trip the generic
                // "amount and unit" message.
                issues.push((
                    "amount",
                    format!(
                        "Amount contains a unit (e.g. `100g`); split the number \
                         into Amount and the unit (e.g. `{}`) into Unit.",
                        unit_text
                    ),
                ));
                None
            }
            CellAmount::NotANumber => {
                issues.push(("amount", "Amount must be a number".to_string()));
                None
            }
            CellAmount::Empty => None,
        };

        if ingredient.is_some() {
            let has_amount = amount.is_some();
            let has_unit = unit.is_some();
            if !has_amount || !has_unit {
                // Only emit the generic "amount and unit" message when
                // the amount cell was `Empty` (truly missing) — if it
                // was `WithUnit` or `NotANumber` the specific error
                // above already fires and we'd double-report. Blank
                // amount PLUS blank unit on an ingredient row still
                // produces this message exactly as before.
                let amount_specific = matches!(
                    amount_cell,
                    CellAmount::WithUnit(_, _) | CellAmount::NotANumber
                );
                if !amount_specific {
                    // TS attaches this to the root path (`""`) via
                    // custom issue; its `.path.join('.')` evaluates
                    // to `""`, which we represent as `"unknown"` to
                    // match the TS fallback `?? 'unknown'`.
                    issues.push(("", "Ingredient rows must have amount and unit".to_string()));
                }
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
                .map(|(_, m)| m.as_str())
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

/// Richer return shape for the amount column — the TS parser only
/// distinguishes "number" from "not a number", but the real-client
/// recipes sheet has cells like `"100g"` and `"1.00 ml"` where the
/// unit is baked into Amount. Those used to surface as
/// `"Amount must be greater than zero"` (because `cell_number`
/// returned `None` → the row-level "ingredient rows must have amount
/// and unit" error fired), which is misleading. `WithUnit` lets the
/// row loop emit a specific message pointing the user at the fix.
///
/// See D3 of `docs/handoff-recipes-importer.md`.
#[derive(Debug, PartialEq)]
enum CellAmount {
    /// Clean numeric value.
    Ok(f64),
    /// Cell was a string of the form `<number><letters>` — the number
    /// and the trailing unit text, both trimmed. The caller surfaces
    /// a specific "contains a unit; split into Amount + Unit" error.
    WithUnit(f64, String),
    /// Cell was absent, null, or an empty/whitespace string. Matches
    /// the TS behaviour where `Number(undefined)` yielded `NaN` and
    /// surfaced as "amount missing" downstream.
    Empty,
    /// Cell was present but neither a number, a `<num><letters>`
    /// composite, nor a parseable numeric string. Matches TS's
    /// `isNaN(Number(v))` branch.
    NotANumber,
}

/// Parse a numeric cell. Numbers pass through; strings try
/// `parse::<f64>` first, then the `<number><letters>` composite
/// shape for the unit-baked-in case; everything else is `NotANumber`
/// or `Empty`. Semantics are a superset of TS `cellNumber(cell)`:
/// every input that TS parsed to a finite number parses to
/// `CellAmount::Ok` here, every empty cell is `Empty`, and the two
/// new variants (`WithUnit`, `NotANumber`) split what TS lumped into
/// its implicit "amount missing" error.
fn cell_number(cell: Option<&Cell>) -> CellAmount {
    let Some(cell) = cell else {
        return CellAmount::Empty;
    };
    match cell {
        Cell::Null => CellAmount::Empty,
        Cell::Number(n) => {
            if n.is_finite() {
                CellAmount::Ok(*n)
            } else {
                CellAmount::NotANumber
            }
        }
        Cell::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                return CellAmount::Empty;
            }
            // Clean numeric string first — the common case for cells
            // ExcelJS/calamine typed as string (`"5"`, `"1.5"`).
            if let Some(n) = t.parse::<f64>().ok().filter(|n| n.is_finite()) {
                return CellAmount::Ok(n);
            }
            // Composite `<number><letters>` — e.g. `"100g"`, `"1.00 ml"`,
            // `"0.10 g"`. Pure ASCII, no sign, optional whitespace between
            // number and unit. Matches the regex spelled out in D3 of
            // `docs/handoff-recipes-importer.md`:
            //     ^(\d+(?:\.\d+)?)\s*([a-zA-Z]+)$
            // Hand-rolled rather than pulling in `regex` because this is
            // the only pattern this parser needs.
            if let Some((num, unit)) = split_amount_with_unit(t) {
                if let Some(n) = num.parse::<f64>().ok().filter(|n| n.is_finite()) {
                    return CellAmount::WithUnit(n, unit);
                }
            }
            CellAmount::NotANumber
        }
        _ => CellAmount::NotANumber,
    }
}

/// Try to split a trimmed string into `(<number>, <unit>)` where the
/// number matches `\d+(\.\d+)?` and the unit is one-or-more ASCII
/// letters. Whitespace between them is tolerated and dropped. Returns
/// `None` on any shape mismatch.
///
/// Hand-rolled scanner — pulling in `regex` for one pattern isn't worth
/// the dependency weight. The grammar is strict enough that a loop over
/// `char_indices` is clearer than a regex here.
fn split_amount_with_unit(s: &str) -> Option<(String, String)> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return None;
    }

    // Scan integer part — one or more ASCII digits.
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 {
        return None;
    }

    // Optional fractional part — `.` followed by one or more digits.
    if i < bytes.len() && bytes[i] == b'.' {
        let frac_start = i + 1;
        let mut j = frac_start;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j == frac_start {
            // Trailing dot with no digits — not our grammar.
            return None;
        }
        i = j;
    }
    let num_end = i;

    // Optional whitespace between number and unit.
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    let unit_start = i;

    // Unit — one or more ASCII letters, then nothing else.
    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
        i += 1;
    }
    if i == unit_start {
        return None;
    }
    if i != bytes.len() {
        return None;
    }

    Some((s[..num_end].to_string(), s[unit_start..].to_string()))
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
    fn amount_with_unit_errors_with_friendly_message() {
        // D3: a string cell shaped `<number><letters>` ("100g",
        // "1.00 ml", "0.10 g") used to trip the generic "Ingredient
        // rows must have amount and unit" message because
        // `cell_number` returned `None`. CellAmount::WithUnit now
        // surfaces a specific error pointing at the fix.
        let wb = mk_wb(
            &["Recipe", "Ingredient", "Amount", "Unit", "Allergens"],
            &[vec![
                ("Recipe", Cell::String("R".into())),
                ("Ingredient", Cell::String("I".into())),
                ("Amount", Cell::String("100g".into())),
                ("Unit", Cell::String("g".into())),
            ]],
        );
        let result = parse_from_workbook(&wb).unwrap();
        assert_eq!(result.rows.len(), 0);
        assert_eq!(result.errors.len(), 1);
        let e = &result.errors[0];
        assert_eq!(e.row, 2);
        assert_eq!(e.field.as_deref(), Some("amount"));
        assert!(
            e.message.contains("contains a unit"),
            "message must point at the unit-split fix, got: {}",
            e.message
        );
        assert!(
            e.message.contains("`g`"),
            "message must echo the detected unit, got: {}",
            e.message
        );
    }

    #[test]
    fn amount_with_unit_handles_whitespace_and_decimals() {
        // Shapes that appear in the real Recipes MoM sheet: `"1.00 ml"`
        // and `"0.10 g"`. Both must trigger the WithUnit path.
        for raw in ["1.00 ml", "0.10 g", "100g"] {
            let wb = mk_wb(
                &["Recipe", "Ingredient", "Amount", "Unit", "Allergens"],
                &[vec![
                    ("Recipe", Cell::String("R".into())),
                    ("Ingredient", Cell::String("I".into())),
                    ("Amount", Cell::String(raw.into())),
                    ("Unit", Cell::String("g".into())),
                ]],
            );
            let result = parse_from_workbook(&wb).unwrap();
            assert_eq!(
                result.errors.len(),
                1,
                "input {raw:?} must produce exactly one error"
            );
            assert!(
                result.errors[0].message.contains("contains a unit"),
                "input {raw:?} must trigger the WithUnit error, got: {}",
                result.errors[0].message
            );
        }
    }

    #[test]
    fn amount_bare_string_errors_as_not_a_number() {
        // A non-numeric, non-composite string (`"a pinch"`) hits the
        // NotANumber branch — the specific "Amount must be a number"
        // error, not the generic amount+unit one.
        let wb = mk_wb(
            &["Recipe", "Ingredient", "Amount", "Unit", "Allergens"],
            &[vec![
                ("Recipe", Cell::String("R".into())),
                ("Ingredient", Cell::String("I".into())),
                ("Amount", Cell::String("a pinch".into())),
                ("Unit", Cell::String("g".into())),
            ]],
        );
        let result = parse_from_workbook(&wb).unwrap();
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].field.as_deref(), Some("amount"));
        assert!(
            result.errors[0].message.contains("must be a number"),
            "got: {}",
            result.errors[0].message
        );
    }

    #[test]
    fn split_amount_with_unit_rejects_bad_shapes() {
        // Direct test on the scanner — leading signs, exponents,
        // trailing junk, pure numbers, pure letters all reject.
        assert_eq!(super::split_amount_with_unit(""), None);
        assert_eq!(super::split_amount_with_unit("100"), None);
        assert_eq!(super::split_amount_with_unit("g"), None);
        assert_eq!(super::split_amount_with_unit("-100g"), None);
        assert_eq!(super::split_amount_with_unit("1e3g"), None);
        assert_eq!(super::split_amount_with_unit("100g!"), None);
        assert_eq!(super::split_amount_with_unit("100."), None);
        assert_eq!(
            super::split_amount_with_unit("100g"),
            Some(("100".into(), "g".into()))
        );
        assert_eq!(
            super::split_amount_with_unit("1.00 ml"),
            Some(("1.00".into(), "ml".into()))
        );
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
