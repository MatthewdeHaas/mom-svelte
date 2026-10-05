//! Per-row normalisation. Mirrors `normaliseRow` in
//! `src/lib/server/queries/imports.ts:350-423`.
//!
//! The output is a `HashMap<&'static str, Value>` keyed by the canonical
//! field names declared in `headers.rs`. Downstream validation reads
//! those keys directly — no intermediate struct — so adding a field
//! means updating the two touchpoints (`validate.rs` + `headers.rs`)
//! and nothing else.

use std::collections::HashMap;

use domain::ImportWarning;
use serde_json::{json, Value};

use crate::headers::{HeaderAnalysis, SplitStorage, F_MEAL_CATEGORY, F_STORAGE};
use crate::meals::{canonicalize, CanonResult};
use crate::parse::Cell;

/// Per-row output. Values carry their source types so the validator
/// can enforce string/number/null rules without re-parsing.
pub type NormalisedRow = HashMap<&'static str, Value>;

/// Convert one parsed row (keyed by ORIGINAL header strings) into a
/// `NormalisedRow` keyed by canonical field names, plus any row-level
/// warnings the normalisation produced.
pub fn normalise_row(
    raw: &HashMap<String, Cell>,
    analysis: &HeaderAnalysis,
    row_number: i64,
) -> (NormalisedRow, Vec<ImportWarning>) {
    let mut out: NormalisedRow = HashMap::new();
    let mut warnings: Vec<ImportWarning> = Vec::new();

    for (header, field) in &analysis.header_to_field {
        let value = raw.get(header);

        // Explicit storage column — accept 'f'/'c'/'F'/'C' or blank.
        // Anything else becomes null, exactly matching the TS branch.
        if *field == F_STORAGE {
            let v = match value {
                Some(Cell::String(s)) => {
                    let lower = s.trim().to_ascii_lowercase();
                    if lower == "f" || lower == "c" {
                        Value::String(lower)
                    } else {
                        Value::Null
                    }
                }
                _ => Value::Null,
            };
            out.insert(*field, v);
            continue;
        }

        // Meal category — canonicalize + emit warning on normalisation.
        if *field == F_MEAL_CATEGORY {
            match value {
                Some(Cell::String(s)) => {
                    match canonicalize(s) {
                        CanonResult::Canonical {
                            value: canon,
                            normalized,
                            original,
                        } => {
                            out.insert(*field, Value::String(canon.to_string()));
                            if normalized {
                                warnings.push(ImportWarning::MealCaseNormalized {
                                    row: Some(row_number),
                                    message: format!(
                                        "Meal \"{}\" normalized to \"{}\".",
                                        original.trim(),
                                        canon
                                    ),
                                    details: Some(json!({
                                        "from": original,
                                        "to": canon,
                                    })),
                                });
                            }
                        }
                        CanonResult::Unknown => {
                            // Pass through so validation rejects it with
                            // the usual "must be one of ..." message.
                            out.insert(*field, Value::String(s.clone()));
                        }
                    }
                }
                Some(other) => {
                    out.insert(*field, cell_to_json(other));
                }
                None => {
                    out.insert(*field, Value::Null);
                }
            }
            continue;
        }

        // Default path — straight copy as a JSON value.
        let v = match value {
            Some(cell) => cell_to_json(cell),
            None => Value::Null,
        };
        out.insert(*field, v);
    }

    // Split-storage: fold Frozen + Chilled `x`-markers into derived
    // storage value when there's no explicit Storage column.
    if let Some(ss) = &analysis.split_storage {
        let (storage_value, warning) = resolve_split_storage(raw, ss, row_number);
        out.insert(F_STORAGE, storage_value);
        if let Some(w) = warning {
            warnings.push(w);
        }
    }

    (out, warnings)
}

/// Collapse Frozen+Chilled marker cells into `'f' | 'c' | null`.
/// Emits a storage warning when both or neither is marked.
fn resolve_split_storage(
    raw: &HashMap<String, Cell>,
    ss: &SplitStorage,
    row_number: i64,
) -> (Value, Option<ImportWarning>) {
    let frozen = is_x_marker(raw.get(&ss.frozen_header));
    let chilled = is_x_marker(raw.get(&ss.chilled_header));

    match (frozen, chilled) {
        (true, true) => (
            Value::String("f".into()),
            Some(ImportWarning::StorageBothSet {
                row: Some(row_number),
                message: "Both Frozen and Chilled are marked; defaulted to Frozen.".into(),
                details: None,
            }),
        ),
        (true, false) => (Value::String("f".into()), None),
        (false, true) => (Value::String("c".into()), None),
        (false, false) => (
            Value::String("c".into()),
            Some(ImportWarning::StorageMissingDefaultedChilled {
                row: Some(row_number),
                message: "Neither Frozen nor Chilled is marked; defaulted to Chilled.".into(),
                details: None,
            }),
        ),
    }
}

/// `"x"` / `"X"` / `" x "` → true. Anything else → false.
fn is_x_marker(cell: Option<&Cell>) -> bool {
    match cell {
        Some(Cell::String(s)) => s.trim().eq_ignore_ascii_case("x"),
        _ => false,
    }
}

/// Convert a `Cell` into a serde_json Value. The validator later
/// pattern-matches on `Value::{String,Number,Null,Bool}` so this is
/// the single place type-coercion happens.
pub fn cell_to_json(cell: &Cell) -> Value {
    match cell {
        Cell::Null => Value::Null,
        Cell::Bool(b) => Value::Bool(*b),
        Cell::Number(n) => serde_json::Number::from_f64(*n)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Cell::String(s) | Cell::Date(s) => Value::String(s.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headers::analyse;

    fn mkrow(pairs: &[(&str, Cell)]) -> HashMap<String, Cell> {
        pairs
            .iter()
            .cloned()
            .map(|(k, v)| (k.to_string(), v))
            .collect()
    }

    #[test]
    fn canonicalises_lowercase_meal_with_warning() {
        let a = analyse(&["Customer".to_string(), "Meal".to_string()]);
        let raw = mkrow(&[
            ("Customer", Cell::String("Acme".into())),
            ("Meal", Cell::String("lunch".into())),
        ]);
        let (row, warns) = normalise_row(&raw, &a, 2);
        assert_eq!(row["meal_category"], Value::String("Lunch".into()));
        assert_eq!(warns.len(), 1);
        assert_eq!(warns[0].code(), "meal_case_normalized");
    }

    #[test]
    fn split_storage_both_set_defaults_frozen() {
        let a = analyse(&[
            "Customer".to_string(),
            "Frozen".to_string(),
            "Chilled".to_string(),
        ]);
        let raw = mkrow(&[
            ("Customer", Cell::String("Acme".into())),
            ("Frozen", Cell::String("x".into())),
            ("Chilled", Cell::String("X".into())),
        ]);
        let (row, warns) = normalise_row(&raw, &a, 5);
        assert_eq!(row["storage"], Value::String("f".into()));
        assert_eq!(warns.len(), 1);
        assert_eq!(warns[0].code(), "storage_both_set");
    }

    #[test]
    fn split_storage_neither_defaults_chilled() {
        let a = analyse(&["Frozen".to_string(), "Chilled".to_string()]);
        let raw = mkrow(&[("Frozen", Cell::Null), ("Chilled", Cell::Null)]);
        let (row, warns) = normalise_row(&raw, &a, 7);
        assert_eq!(row["storage"], Value::String("c".into()));
        assert_eq!(warns.len(), 1);
        assert_eq!(warns[0].code(), "storage_missing_defaulted_chilled");
    }

    #[test]
    fn explicit_storage_f_passes_through() {
        let a = analyse(&["Storage".to_string()]);
        let raw = mkrow(&[("Storage", Cell::String("F".into()))]);
        let (row, warns) = normalise_row(&raw, &a, 2);
        assert_eq!(row["storage"], Value::String("f".into()));
        assert!(warns.is_empty());
    }

    #[test]
    fn explicit_storage_invalid_becomes_null() {
        let a = analyse(&["Storage".to_string()]);
        let raw = mkrow(&[("Storage", Cell::String("maybe".into()))]);
        let (row, _) = normalise_row(&raw, &a, 2);
        assert_eq!(row["storage"], Value::Null);
    }
}
