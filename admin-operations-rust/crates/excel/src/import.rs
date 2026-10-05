//! Full import pipeline. Mirrors `importOrdersFromRows` in
//! `src/lib/server/queries/imports.ts:472-644`.
//!
//! `plan_import` runs the 7-step pipeline (parse → analyse → stop →
//! mismatch → normalise → validate → resolve) and returns an
//! `ImportPlan`. It does NOT write to the DB — the API layer decides
//! whether to insert (commit) or discard (preview). This split keeps
//! both endpoints exercising the same code path and makes the
//! integration test trivial: parse once, assert once.
//!
//! Error strategy mirrors TS: collect ALL row-level errors across ALL
//! rows before returning. A single bad row aborts the whole import —
//! `plan_import` returns a plan with `errors` populated and `orders`
//! left empty.

use std::collections::HashMap;

use db::queries::{companies, orders::InsertOrder, recipes};
use domain::{ImportRowError, ImportWarning};
use serde_json::json;
use sqlx::PgPool;

use crate::headers::{analyse, HeaderAnalysis, F_RECIPE};
use crate::normalise::normalise_row;
use crate::parse::{parse_workbook, Cell, ParsedWorkbook};
use crate::validate::{validate_row, ParsedRow};

/// What `plan_import` produces. `orders` is only populated when
/// `errors.is_empty()` — otherwise the caller returns the plan as-is
/// (no inserts).
#[derive(Debug)]
pub struct ImportPlan {
    pub orders: Vec<InsertOrder>,
    pub warnings: Vec<ImportWarning>,
    pub errors: Vec<ImportRowError>,
    pub row_count: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("workbook parse failed: {0}")]
    Parse(#[from] crate::parse::ParseError),
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
}

/// Parse + validate + resolve. No DB writes.
///
/// `created_by` is the user id attached to every inserted order; it's
/// not consulted until the caller actually commits.
pub async fn plan_import(
    bytes: &[u8],
    created_by: &str,
    pool: &PgPool,
) -> Result<ImportPlan, ImportError> {
    let workbook = parse_workbook(bytes)?;
    let mut warnings: Vec<ImportWarning> = Vec::new();
    let mut errors: Vec<ImportRowError> = Vec::new();

    // Sheet-level `extra_sheets_ignored` — matches TS verbatim including
    // the "sheet was" / "sheets were" pluralisation.
    if workbook.extra_sheet_count > 0 {
        let count = workbook.extra_sheet_count;
        warnings.push(ImportWarning::ExtraSheetsIgnored {
            row: None,
            message: format!(
                "{} additional {} ignored; only the first sheet is imported.",
                count,
                if count == 1 {
                    "sheet was"
                } else {
                    "sheets were"
                }
            ),
            details: Some(json!({ "count": count })),
        });
    }

    if workbook.rows.is_empty() {
        return Ok(ImportPlan {
            orders: Vec::new(),
            warnings,
            errors,
            row_count: 0,
        });
    }

    // STEP 1 — header analysis (column map, split-storage, redundant).
    let analysis = analyse(&workbook.headers);
    warnings.extend(analysis.redundant_warnings.iter().cloned());

    // STEP 2 — boilerplate stop.
    let data_rows = truncate_at_boilerplate(&workbook, &analysis, &mut warnings);
    if data_rows.is_empty() {
        return Ok(ImportPlan {
            orders: Vec::new(),
            warnings,
            errors,
            row_count: 0,
        });
    }

    // STEP 3 — item/recipe mismatch (sheet-level, before normalisation
    // strips the Item column).
    emit_item_recipe_mismatches(&analysis, &data_rows, &mut warnings);

    // STEP 4 — per-row normalisation.
    let normalised: Vec<_> = data_rows
        .iter()
        .enumerate()
        .map(|(i, raw)| {
            let (row, w) = normalise_row(raw, &analysis, (i as i64) + 2);
            warnings.extend(w);
            row
        })
        .collect();

    // STEP 5 — per-row validation (no DB calls yet).
    let mut parsed: Vec<Option<ParsedRow>> = Vec::with_capacity(normalised.len());
    for (i, row) in normalised.iter().enumerate() {
        let row_number = (i as i64) + 2;
        match validate_row(row, row_number) {
            Ok(p) => parsed.push(Some(p)),
            Err(errs) => {
                errors.extend(errs);
                parsed.push(None);
            }
        }
    }

    if !errors.is_empty() {
        return Ok(ImportPlan {
            orders: Vec::new(),
            warnings,
            errors,
            row_count: data_rows.len(),
        });
    }

    // STEP 6 — resolve company + recipe. TS uses `Promise.all`; we go
    // serial for now — the real-client fixture has 350 rows, ~700
    // lookups, which is a few hundred ms serial. If it ever matters,
    // swap to `futures::future::join_all` with a semaphore.
    let mut insert_orders: Vec<InsertOrder> = Vec::with_capacity(parsed.len());
    for (i, p) in parsed.into_iter().enumerate() {
        let row_number = (i as i64) + 2;
        let p = p.expect("validated rows are Some when errors is empty");

        let company = companies::get_by_name(pool, &p.company).await?;
        let recipe = recipes::get_by_name_active(pool, &p.recipe).await?;

        let mut row_errors: Vec<ImportRowError> = Vec::new();
        match company {
            None => row_errors.push(ImportRowError {
                row: row_number,
                field: Some("company".into()),
                message: format!("Company \"{}\" not found in the system", p.company),
                raw_value: None,
                severity: None,
                suggestion: None,
            }),
            Some(ref c) if !c.active => row_errors.push(ImportRowError {
                row: row_number,
                field: Some("company".into()),
                message: format!(
                    "Company \"{}\" is inactive and cannot receive orders",
                    p.company
                ),
                raw_value: None,
                severity: None,
                suggestion: None,
            }),
            _ => {}
        }
        if recipe.is_none() {
            row_errors.push(ImportRowError {
                row: row_number,
                field: Some("recipe".into()),
                message: format!("Recipe \"{}\" not found or is inactive", p.recipe),
                raw_value: None,
                severity: None,
                suggestion: None,
            });
        }

        if !row_errors.is_empty() {
            errors.extend(row_errors);
            continue;
        }

        let company = company.unwrap();
        let recipe = recipe.unwrap();

        insert_orders.push(InsertOrder {
            company_id: company.id,
            recipe_id: recipe.id,
            created_by: created_by.to_string(),
            order_source: "excel_import",
            status: "pending",
            meal_category: p.meal_category,
            amount: p.amount,
            portion_size: p.portion_size,
            day: p.day,
            serving_date: p.serving_date,
            shipment_date: p.shipment_date,
            frozen: matches!(p.storage, Some('f')),
            chilled: matches!(p.storage, Some('c')),
            recurring: false,
            client: p.client,
        });
    }

    if !errors.is_empty() {
        return Ok(ImportPlan {
            orders: Vec::new(),
            warnings,
            errors,
            row_count: data_rows.len(),
        });
    }

    let row_count = insert_orders.len();
    Ok(ImportPlan {
        orders: insert_orders,
        warnings,
        errors,
        row_count,
    })
}

/// Scan rows for the first boilerplate marker; return the slice before
/// it and emit `boilerplate_row_stopped` on hit. Mirrors the TS
/// `for` loop that uses `rawRows.slice(0, i)`.
fn truncate_at_boilerplate<'a>(
    workbook: &'a ParsedWorkbook,
    analysis: &HeaderAnalysis,
    warnings: &mut Vec<ImportWarning>,
) -> Vec<&'a HashMap<String, Cell>> {
    let total = workbook.rows.len();
    for (i, row) in workbook.rows.iter().enumerate() {
        if is_boilerplate(row, analysis.item_header.as_deref()) {
            let row_number = (i as i64) + 2;
            let dropped = total - i;
            warnings.push(ImportWarning::BoilerplateRowStopped {
                row: Some(row_number),
                message: format!(
                    "Stopped reading at boilerplate row {}; {} trailing {} ignored.",
                    row_number,
                    dropped,
                    if dropped == 1 { "row" } else { "rows" }
                ),
                details: Some(json!({
                    "stopped_at": row_number,
                    "dropped_count": dropped,
                })),
            });
            return workbook.rows[..i].iter().collect();
        }
    }
    workbook.rows.iter().collect()
}

/// Check Customer / Recipe / Item cells against boilerplate tokens.
/// Mirrors `isBoilerplateRow` in `imports.ts:441-460` — the TS version
/// explicitly reads `Customer`/`customer`/`Recipe`/`recipe` keys.
fn is_boilerplate(raw: &HashMap<String, Cell>, item_header: Option<&str>) -> bool {
    let mut candidates: Vec<&str> = Vec::new();
    for key in ["Customer", "customer", "Recipe", "recipe"] {
        if let Some(Cell::String(s)) = raw.get(key) {
            candidates.push(s.as_str());
        }
    }
    if let Some(h) = item_header {
        if let Some(Cell::String(s)) = raw.get(h) {
            candidates.push(s.as_str());
        }
    }

    for text in candidates {
        if crate::boilerplate::is_boilerplate(Some(text), None, None) {
            return true;
        }
    }
    false
}

/// Scan data rows for item vs recipe mismatches when an Item column
/// is present. Mirrors step 3 of `importOrdersFromRows`.
fn emit_item_recipe_mismatches(
    analysis: &HeaderAnalysis,
    rows: &[&HashMap<String, Cell>],
    warnings: &mut Vec<ImportWarning>,
) {
    let Some(item_header) = analysis.item_header.as_deref() else {
        return;
    };

    // Find the ORIGINAL header that maps to the `recipe` field. The TS
    // path reads `row['Recipe'] ?? row['recipe']`; we honour the full
    // header_to_field map so renamed-but-canonical columns still work.
    let recipe_header: Option<&str> = analysis
        .header_to_field
        .iter()
        .find(|(_, field)| **field == F_RECIPE)
        .map(|(h, _)| h.as_str());

    for (i, row) in rows.iter().enumerate() {
        let item_val = row.get(item_header).and_then(|c| match c {
            Cell::String(s) => Some(s.as_str()),
            _ => None,
        });
        let recipe_val = recipe_header
            .and_then(|h| row.get(h))
            .and_then(|c| match c {
                Cell::String(s) => Some(s.as_str()),
                _ => None,
            });

        let (Some(item), Some(recipe)) = (item_val, recipe_val) else {
            continue;
        };
        let item_t = item.trim();
        let recipe_t = recipe.trim();
        if item_t.is_empty() || recipe_t.is_empty() {
            continue;
        }
        if item_t.eq_ignore_ascii_case(recipe_t) {
            continue;
        }

        warnings.push(ImportWarning::ItemRecipeMismatch {
            row: Some((i as i64) + 2),
            message: format!(
                "Item \"{}\" differs from Recipe \"{}\"; Recipe wins.",
                item_t, recipe_t
            ),
            details: Some(json!({
                "item": item_t,
                "recipe": recipe_t,
            })),
        });
    }
}
