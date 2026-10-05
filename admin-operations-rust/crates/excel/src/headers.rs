//! Header analysis — maps raw sheet headers to canonical schema
//! fields and surfaces sheet-level warnings about redundant columns.
//!
//! Mirrors `COLUMN_MAP` / `WARN_REDUNDANT_COLUMNS` / `analyseHeaders`
//! in `src/lib/server/queries/imports.ts`. The TS normalise step is
//! `h.trim().toLowerCase().replace(/\s+/g, ' ')` — the Rust
//! equivalent uses `trim().to_ascii_lowercase().split_whitespace()`
//! which produces identical output for the ASCII headers both the
//! template and the real-client fixture use.

use std::collections::HashMap;

use domain::ImportWarning;
use once_cell::sync::Lazy;
use serde_json::json;

/// Canonical schema fields the importer consumes. One per column the
/// normalised row map carries into validation. Kept as `&'static str`
/// so lookup results can be copied cheaply.
pub const F_COMPANY: &str = "company";
pub const F_RECIPE: &str = "recipe";
pub const F_SERVING_DATE: &str = "serving_date";
pub const F_SHIPMENT_DATE: &str = "shipment_date";
pub const F_MEAL_CATEGORY: &str = "meal_category";
pub const F_AMOUNT: &str = "amount";
pub const F_PORTION_SIZE: &str = "portion_size";
pub const F_DAY: &str = "day";
pub const F_STORAGE: &str = "storage";
pub const F_CLIENT: &str = "client";

/// Normalised header string → canonical field name.
///
/// `frozen` and `chilled` are deliberately absent — those two headers
/// are detected separately in `analyse` and fold into a derived
/// `storage` value at the per-row step.
static COLUMN_MAP: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    HashMap::from([
        ("company", F_COMPANY),
        ("customer", F_COMPANY),
        ("recipe", F_RECIPE),
        ("serving date", F_SERVING_DATE),
        ("serving_date", F_SERVING_DATE),
        ("servingdate", F_SERVING_DATE),
        ("shipment date", F_SHIPMENT_DATE),
        ("shipment_date", F_SHIPMENT_DATE),
        ("shipmentdate", F_SHIPMENT_DATE),
        ("meal", F_MEAL_CATEGORY),
        ("meal category", F_MEAL_CATEGORY),
        ("meal_category", F_MEAL_CATEGORY),
        ("mealcategory", F_MEAL_CATEGORY),
        ("meal type", F_MEAL_CATEGORY),
        ("meal_type", F_MEAL_CATEGORY),
        ("mealtype", F_MEAL_CATEGORY),
        ("amount", F_AMOUNT),
        ("order amount", F_AMOUNT),
        ("order_amount", F_AMOUNT),
        ("orderamount", F_AMOUNT),
        ("portion size", F_PORTION_SIZE),
        ("portion_size", F_PORTION_SIZE),
        ("portionsize", F_PORTION_SIZE),
        ("day", F_DAY),
        ("storage", F_STORAGE),
        ("frozen/chilled", F_STORAGE),
        ("client", F_CLIENT),
    ])
});

/// A column that duplicates data already in a canonical column.
/// Each entry maps `(normalised header → (warning code, message))`.
struct RedundantSpec {
    code: RedundantCode,
    message: &'static str,
}

#[derive(Clone, Copy)]
enum RedundantCode {
    ItemColumnIgnored,
    QtyFormulaIgnored,
}

static WARN_REDUNDANT_COLUMNS: Lazy<HashMap<&'static str, RedundantSpec>> = Lazy::new(|| {
    const QTY_MSG: &str =
        "Quantity to Produce/Ship is computed from Order Amount × Portion Size; ignored.";
    HashMap::from([
        (
            "item",
            RedundantSpec {
                code: RedundantCode::ItemColumnIgnored,
                message: "Item column duplicates Recipe; ignored.",
            },
        ),
        (
            "quantity to produce/ship",
            RedundantSpec {
                code: RedundantCode::QtyFormulaIgnored,
                message: QTY_MSG,
            },
        ),
        (
            "quantity to produce",
            RedundantSpec {
                code: RedundantCode::QtyFormulaIgnored,
                message: QTY_MSG,
            },
        ),
        (
            "quantity_to_produce",
            RedundantSpec {
                code: RedundantCode::QtyFormulaIgnored,
                message: QTY_MSG,
            },
        ),
        (
            "qty",
            RedundantSpec {
                code: RedundantCode::QtyFormulaIgnored,
                message: QTY_MSG,
            },
        ),
    ])
});

/// Normalise a raw header string: trim, lower-case, collapse runs of
/// whitespace into single spaces. ASCII-only (fast path) because the
/// real-client fixture and the template never carry non-ASCII headers;
/// if that ever changes, swap to a `unicase`-backed implementation.
fn normalise_header(h: &str) -> String {
    h.trim()
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Where to find the Frozen / Chilled columns on the sheet when the
/// sheet uses the split-storage shape instead of a single `Storage`
/// column. The headers are preserved verbatim so a round-trip through
/// `calamine` → importer → re-serialise sees the same bytes
/// (critical for the sanitiser rebuild).
#[derive(Debug, Clone)]
pub struct SplitStorage {
    pub frozen_header: String,
    pub chilled_header: String,
}

/// Result of analysing the header row. `header_to_field` is keyed by
/// the ORIGINAL header string (not the normalised one) so the per-row
/// step can read values straight off a `HashMap<String, Cell>` built
/// at parse time without re-normalising per cell.
#[derive(Debug)]
pub struct HeaderAnalysis {
    pub header_to_field: HashMap<String, &'static str>,
    pub split_storage: Option<SplitStorage>,
    pub redundant_warnings: Vec<ImportWarning>,
    /// Present when the sheet contains an `Item` column; the mismatch
    /// scan reads this header off each row to compare against Recipe.
    pub item_header: Option<String>,
}

/// Analyse the raw header strings exactly as `analyseHeaders` does
/// on the TS side. Preserves original header casing in the output
/// map so parsers that key cells by original header string can look
/// up the canonical field in one step.
pub fn analyse(headers: &[String]) -> HeaderAnalysis {
    let mut header_to_field: HashMap<String, &'static str> = HashMap::new();
    let mut redundant_warnings: Vec<ImportWarning> = Vec::new();
    let mut seen_item = false;
    let mut seen_qty = false;
    let mut item_header: Option<String> = None;
    let mut frozen_header: Option<String> = None;
    let mut chilled_header: Option<String> = None;
    let mut has_storage = false;

    for original in headers {
        let normalised = normalise_header(original);
        if normalised.is_empty() {
            continue;
        }

        // Redundant columns — emit one sheet-level warning per code
        // and skip the column so validation never sees it.
        if let Some(spec) = WARN_REDUNDANT_COLUMNS.get(normalised.as_str()) {
            match spec.code {
                RedundantCode::ItemColumnIgnored => {
                    item_header = Some(original.clone());
                    if !seen_item {
                        seen_item = true;
                        redundant_warnings.push(ImportWarning::ItemColumnIgnored {
                            row: None,
                            message: spec.message.to_string(),
                            details: None,
                        });
                    }
                }
                RedundantCode::QtyFormulaIgnored => {
                    if !seen_qty {
                        seen_qty = true;
                        redundant_warnings.push(ImportWarning::QtyFormulaIgnored {
                            row: None,
                            message: spec.message.to_string(),
                            details: None,
                        });
                    }
                }
            }
            continue;
        }

        // Split-storage detection — track Frozen/Chilled separately
        // so the per-row step can fold both into a derived storage
        // value and emit `storage_both_set` / `storage_missing_...`
        // row-level warnings.
        if normalised == "frozen" {
            frozen_header = Some(original.clone());
            continue;
        }
        if normalised == "chilled" {
            chilled_header = Some(original.clone());
            continue;
        }
        if normalised == "storage" {
            has_storage = true;
        }

        if let Some(field) = COLUMN_MAP.get(normalised.as_str()).copied() {
            header_to_field.insert(original.clone(), field);
        }
    }

    // Collapse Frozen+Chilled → storage iff there's no explicit
    // Storage column (otherwise the explicit column wins and the
    // split headers are dropped silently — matches TS behavior).
    let split_storage = match (frozen_header, chilled_header, has_storage) {
        (Some(f), Some(c), false) => Some(SplitStorage {
            frozen_header: f,
            chilled_header: c,
        }),
        _ => None,
    };

    // Details on the extra-sheets warning aren't built here (that
    // lives in `import::plan_import`) — just the redundant-column
    // warnings. Avoid the unused import.
    let _ = json!({});

    HeaderAnalysis {
        header_to_field,
        split_storage,
        redundant_warnings,
        item_header,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_canonical_headers() {
        let a = analyse(&[
            "Customer".to_string(),
            "Recipe".to_string(),
            "Serving Date".to_string(),
            "Shipment Date".to_string(),
            "Meal".to_string(),
            "Order Amount".to_string(),
            "Portion Size".to_string(),
            "Day".to_string(),
            "Storage".to_string(),
            "Client".to_string(),
        ]);
        assert_eq!(a.header_to_field["Customer"], F_COMPANY);
        assert_eq!(a.header_to_field["Recipe"], F_RECIPE);
        assert_eq!(a.header_to_field["Serving Date"], F_SERVING_DATE);
        assert_eq!(a.header_to_field["Meal"], F_MEAL_CATEGORY);
        assert_eq!(a.header_to_field["Order Amount"], F_AMOUNT);
        assert_eq!(a.header_to_field["Storage"], F_STORAGE);
        assert!(a.split_storage.is_none());
        assert!(a.redundant_warnings.is_empty());
        assert!(a.item_header.is_none());
    }

    #[test]
    fn detects_split_storage() {
        let a = analyse(&[
            "Customer".to_string(),
            "Recipe".to_string(),
            "Frozen".to_string(),
            " Chilled".to_string(), // leading space on the real fixture
        ]);
        let ss = a.split_storage.unwrap();
        assert_eq!(ss.frozen_header, "Frozen");
        assert_eq!(ss.chilled_header, " Chilled"); // verbatim
    }

    #[test]
    fn explicit_storage_wins_over_split() {
        let a = analyse(&[
            "Storage".to_string(),
            "Frozen".to_string(),
            "Chilled".to_string(),
        ]);
        assert!(a.split_storage.is_none());
    }

    #[test]
    fn emits_item_and_qty_warnings() {
        let a = analyse(&[
            "Customer".to_string(),
            "Recipe".to_string(),
            "Item".to_string(),
            "Quantity to Produce/Ship".to_string(),
        ]);
        assert_eq!(a.redundant_warnings.len(), 2);
        let codes: Vec<_> = a.redundant_warnings.iter().map(|w| w.code()).collect();
        assert!(codes.contains(&"item_column_ignored"));
        assert!(codes.contains(&"qty_formula_ignored"));
        assert_eq!(a.item_header.as_deref(), Some("Item"));
    }

    #[test]
    fn one_warning_per_code_even_with_multiple_qty_variants() {
        let a = analyse(&[
            "qty".to_string(),
            "Quantity to Produce".to_string(),
            "Quantity to Produce/Ship".to_string(),
        ]);
        let qty_count = a
            .redundant_warnings
            .iter()
            .filter(|w| w.code() == "qty_formula_ignored")
            .count();
        assert_eq!(qty_count, 1);
    }

    #[test]
    fn case_and_whitespace_normalised() {
        let a = analyse(&["  CUSTOMER  ".to_string(), "shipment\tdate".to_string()]);
        assert_eq!(a.header_to_field["  CUSTOMER  "], F_COMPANY);
        assert_eq!(a.header_to_field["shipment\tdate"], F_SHIPMENT_DATE);
    }
}
