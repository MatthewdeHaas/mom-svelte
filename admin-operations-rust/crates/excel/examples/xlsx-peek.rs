//! `xlsx-peek` — dump an .xlsx workbook's shape as JSON, using the same
//! `calamine`-based parser the importer runs. No DB, no HTTP, no server
//! required.
//!
//! Purpose: answer "would this file import cleanly?" and "what shape is
//! the admin team sending us this week?" without having to spin up
//! Postgres + the API + the SvelteKit UI. The output is JSON so it's
//! machine-readable; `jq` on top for pretty-print.
//!
//! Usage:
//!     cargo run -p excel --example xlsx-peek -- path/to/file.xlsx
//!     cargo run -p excel --example xlsx-peek -- path/to/orders.xlsx | jq .
//!     cargo run -p excel --example xlsx-peek -- --kind recipes path/to/recipes.xlsx
//!
//! `--kind orders` (default) runs the orders header analysis; `--kind
//! recipes` runs the recipe parser's validation (missing-columns check +
//! per-row Zod-equivalent validation). The report tells you exactly what
//! the preview endpoint would say — zero surprises when you upload.
//!
//! `--validate` (orders-mode only) additionally runs the full
//! `plan_import_pure` pipeline against an empty resolver and dumps every
//! failing row's raw cell state. Resolve errors ("Company X not found")
//! are filtered out — validation runs before resolution per row, and
//! this flag exists to characterise data-level failures (zero amounts,
//! bad dates). Each failing row includes the pre-normalise `Cell`
//! variant of the amount column so you can tell "literal 0" from
//! "blank" from "formula-evaluated-to-0" from "string that didn't
//! parse as a number".
//!
//! Output shape (orders):
//!   { file, bytes, sheet_count, extra_sheets_ignored, headers,
//!     header_to_field, redundant_warnings, split_storage,
//!     item_header, data_row_count, preview: [first 3 rows],
//!     failing_rows? }
//!
//! Output shape (recipes):
//!   { file, bytes, sheet_count, headers, parse_result: { rows_ok,
//!     errors } | { missing_columns } }

use std::collections::HashMap;
use std::fs;
use std::process::ExitCode;

use excel::headers::{analyse, F_AMOUNT, F_COMPANY, F_RECIPE};
use excel::parse::{parse_workbook, Cell};
use excel::recipes::{parse_recipe_xlsx, ParseRecipeError};
use serde_json::{json, Value};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    // Tiny arg parser — avoids pulling in clap for a 2-flag binary.
    let mut kind: &str = "orders";
    let mut validate = false;
    let mut path: Option<String> = None;
    let mut iter = args.iter().skip(1);
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--kind" => {
                let Some(v) = iter.next() else {
                    eprintln!("--kind needs a value (orders|recipes)");
                    return ExitCode::from(2);
                };
                kind = match v.as_str() {
                    "orders" => "orders",
                    "recipes" => "recipes",
                    other => {
                        eprintln!("unknown --kind {other:?}; expected orders|recipes");
                        return ExitCode::from(2);
                    }
                };
            }
            "--validate" => {
                validate = true;
            }
            "-h" | "--help" => {
                eprintln!(
                    "xlsx-peek — dump an .xlsx workbook's shape as JSON\n\
                     usage: xlsx-peek [--kind orders|recipes] [--validate] <file.xlsx>\n\
                     \n\
                     --validate (orders only): run plan_import_pure against an\n\
                       empty resolver and emit `failing_rows` for every row\n\
                       whose validation rejected (resolver misses are filtered\n\
                       out). Characterises data-level failures without needing\n\
                       the DB or the API to be running."
                );
                return ExitCode::SUCCESS;
            }
            other if other.starts_with("--") => {
                eprintln!("unknown flag {other:?}");
                return ExitCode::from(2);
            }
            other => {
                if path.is_some() {
                    eprintln!("expected one path, got extra: {other:?}");
                    return ExitCode::from(2);
                }
                path = Some(other.to_string());
            }
        }
    }

    let Some(path) = path else {
        eprintln!("usage: xlsx-peek [--kind orders|recipes] [--validate] <file.xlsx>");
        return ExitCode::from(2);
    };

    if validate && kind != "orders" {
        eprintln!("--validate is only supported with --kind orders");
        return ExitCode::from(2);
    }

    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("read {path:?}: {e}");
            return ExitCode::from(1);
        }
    };

    let report = match kind {
        "orders" => peek_orders(&path, &bytes, validate),
        "recipes" => peek_recipes(&path, &bytes),
        _ => unreachable!(),
    };

    match report {
        Ok(v) => {
            // Pretty-print — the operator is reading this directly, and
            // `| jq .` works on compact output too if someone pipes it.
            match serde_json::to_string_pretty(&v) {
                Ok(s) => {
                    println!("{s}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("serialize report: {e}");
                    ExitCode::from(1)
                }
            }
        }
        Err(e) => {
            eprintln!("peek failed: {e}");
            ExitCode::from(1)
        }
    }
}

fn peek_orders(path: &str, bytes: &[u8], validate: bool) -> Result<Value, String> {
    let wb = parse_workbook(bytes).map_err(|e| format!("parse_workbook: {e}"))?;
    let analysis = analyse(&wb.headers);

    // Preview: first 3 rows as plain JSON (header → stringified cell).
    let preview: Vec<HashMap<String, String>> = wb
        .rows
        .iter()
        .take(3)
        .map(|r| {
            r.iter()
                .map(|(k, v)| (k.clone(), cell_to_string(v)))
                .collect()
        })
        .collect();

    // header_to_field as plain `{header: field}`; sort by header for
    // deterministic output (HashMap iteration is non-deterministic).
    let mut htof: Vec<(String, &'static str)> = analysis
        .header_to_field
        .iter()
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    htof.sort_by(|a, b| a.0.cmp(&b.0));
    let header_to_field_obj: serde_json::Map<String, Value> = htof
        .into_iter()
        .map(|(k, v)| (k, Value::String(v.to_string())))
        .collect();

    let split_storage = analysis.split_storage.as_ref().map(|s| {
        json!({
            "frozen_header": s.frozen_header,
            "chilled_header": s.chilled_header,
        })
    });

    // --validate: run the pure pipeline against an empty resolver and
    // emit every data-level failing row with its raw cell state. See
    // `build_failing_rows` for filter rationale (resolver misses are
    // filtered out — those would depend on DB state).
    let failing_rows = if validate {
        Some(build_failing_rows(bytes, &wb, &analysis)?)
    } else {
        None
    };

    let mut out = json!({
        "file": path,
        "bytes": bytes.len(),
        "kind": "orders",
        "sheet_count_total": wb.extra_sheet_count + 1,
        "extra_sheets_ignored": wb.extra_sheet_count,
        "headers": wb.headers,
        "data_row_count": wb.rows.len(),
        "header_to_field": header_to_field_obj,
        "redundant_warnings": analysis.redundant_warnings,
        "split_storage": split_storage,
        "item_header": analysis.item_header,
        "preview_first_3": preview,
    });
    if let Some(rows) = failing_rows {
        out["failing_rows"] = Value::Array(rows);
    }
    Ok(out)
}

/// Run the pure import pipeline against an empty resolver and project
/// every data-level failing row into a diagnostic shape:
///
///   { row, field, message, raw_value, customer, recipe,
///     amount_cell_raw, amount_cell_type }
///
/// `field`-less errors and `field == "company" | "recipe"` errors are
/// filtered out — the former are sheet-level ("workbook has no rows"),
/// the latter are resolver misses which depend on DB state and are not
/// what this flag exists to characterise. Everything else — amount,
/// dates, meal category, portion size, day, storage — comes through.
///
/// `amount_cell_raw` / `amount_cell_type` are populated by looking the
/// row's amount header up in the raw `wb.rows` (indexed by
/// `row_number - 2`, matching the convention in `plan_import_pure`).
/// This bypasses the validator's own `raw_value` field, which only
/// stringifies `Cell::Number` and reports `None` for missing/blank —
/// losing the distinction between "literal 0", "blank cell", and
/// "string that didn't parse as a number" which is exactly what D1
/// needs to answer.
fn build_failing_rows(
    bytes: &[u8],
    wb: &excel::parse::ParsedWorkbook,
    analysis: &excel::headers::HeaderAnalysis,
) -> Result<Vec<Value>, String> {
    use excel::import::{plan_import_pure, MapResolver};
    use std::collections::HashMap as StdHashMap;

    let resolver = MapResolver {
        companies: StdHashMap::new(),
        recipes: StdHashMap::new(),
    };
    let plan = plan_import_pure(bytes, "xlsx-peek", &resolver)
        .map_err(|e| format!("plan_import_pure: {e}"))?;

    // Reverse-lookup: canonical field → raw header string. First match
    // wins — the orders analyser already dedupes aliases to a single
    // header per field, so for amount/company/recipe this is unique.
    let header_for_field = |field: &str| -> Option<&str> {
        analysis
            .header_to_field
            .iter()
            .find_map(|(h, f)| (*f == field).then_some(h.as_str()))
    };
    let amount_header = header_for_field(F_AMOUNT);
    let company_header = header_for_field(F_COMPANY);
    let recipe_header = header_for_field(F_RECIPE);

    let mut out: Vec<Value> = Vec::new();
    for err in &plan.errors {
        // Keep data-level errors only; drop resolver misses.
        let field = err.field.as_deref();
        if matches!(field, Some("company") | Some("recipe") | None) {
            continue;
        }

        // row_number = index-in-data_rows + 2. The validator and the
        // caller agree on this convention; data_rows is a truncated
        // view of wb.rows (boilerplate stripped), so wb.rows[row-2]
        // is the same row the validator saw (truncation only drops
        // the tail, never shifts earlier indices).
        let idx = (err.row - 2) as usize;
        let raw_row = wb.rows.get(idx);

        let (amount_cell_raw, amount_cell_type) = match (amount_header, raw_row) {
            (Some(h), Some(row)) => match row.get(h) {
                Some(c) => (cell_to_string(c), cell_type_tag(c).to_string()),
                None => (String::new(), "absent".to_string()),
            },
            _ => (String::new(), "unknown".to_string()),
        };

        let customer = company_header
            .and_then(|h| raw_row.and_then(|r| r.get(h)))
            .map(cell_to_string)
            .unwrap_or_default();
        let recipe = recipe_header
            .and_then(|h| raw_row.and_then(|r| r.get(h)))
            .map(cell_to_string)
            .unwrap_or_default();

        out.push(json!({
            "row": err.row,
            "field": err.field,
            "message": err.message,
            "raw_value": err.raw_value,
            "customer": customer,
            "recipe": recipe,
            "amount_cell_raw": amount_cell_raw,
            "amount_cell_type": amount_cell_type,
        }));
    }
    Ok(out)
}

/// Classify a Cell variant into a short tag — the whole point of
/// --validate is distinguishing these for the operator. `Number` is
/// split into `number_zero` vs `number` because D1 is specifically
/// asking "literal 0 vs blank vs string".
fn cell_type_tag(c: &Cell) -> &'static str {
    match c {
        Cell::Null => "null",
        Cell::String(s) if s.trim().is_empty() => "string_blank",
        Cell::String(_) => "string",
        Cell::Date(_) => "date",
        Cell::Bool(_) => "bool",
        Cell::Number(n) if *n == 0.0 => "number_zero",
        Cell::Number(_) => "number",
    }
}

fn peek_recipes(path: &str, bytes: &[u8]) -> Result<Value, String> {
    // Peek at the workbook independently so we can report headers even
    // when the recipe parser rejects with MissingColumns.
    let wb = parse_workbook(bytes).map_err(|e| format!("parse_workbook: {e}"))?;

    let preview: Vec<HashMap<String, String>> = wb
        .rows
        .iter()
        .take(3)
        .map(|r| {
            r.iter()
                .map(|(k, v)| (k.clone(), cell_to_string(v)))
                .collect()
        })
        .collect();

    let parse_result = match parse_recipe_xlsx(bytes) {
        Ok(result) => json!({
            "ok": true,
            "rows_valid": result.rows.len(),
            "errors": result.errors,
        }),
        Err(ParseRecipeError::MissingColumns(s)) => json!({
            "ok": false,
            "reason": "missing_columns",
            "message": s,
        }),
        Err(ParseRecipeError::NoSheet) => json!({
            "ok": false,
            "reason": "no_sheet",
        }),
        Err(ParseRecipeError::Workbook(e)) => json!({
            "ok": false,
            "reason": "workbook_parse",
            "message": e.to_string(),
        }),
    };

    Ok(json!({
        "file": path,
        "bytes": bytes.len(),
        "kind": "recipes",
        "sheet_count_total": wb.extra_sheet_count + 1,
        "headers": wb.headers,
        "data_row_count": wb.rows.len(),
        "preview_first_3": preview,
        "parse_result": parse_result,
    }))
}

/// Flatten a Cell to a short display string. Null → empty. Dates come
/// in pre-formatted as YYYY-MM-DD. Numbers lose trailing `.0` for
/// readability.
fn cell_to_string(c: &Cell) -> String {
    match c {
        Cell::Null => String::new(),
        Cell::String(s) => s.clone(),
        Cell::Date(s) => s.clone(),
        Cell::Bool(b) => b.to_string(),
        Cell::Number(n) => {
            if n.fract() == 0.0 && n.is_finite() {
                format!("{}", *n as i64)
            } else {
                format!("{n}")
            }
        }
    }
}
