//! Excel-import domain types.
//!
//! The on-the-wire shape here MUST stay byte-identical to the TypeScript
//! `ImportWarning` / `ImportRowError` / `ImportResult` definitions in
//! `src/lib/types.ts` — the SvelteKit route forwards whatever we return
//! straight to the page without translation, and
//! `tests/integration/excel/real-client.test.ts` asserts the warning
//! taxonomy by exact code strings.
//!
//! Specifically:
//! - `ImportWarning` is a tagged enum, tag field `"code"`, variants
//!   serialised in `snake_case` to match the TS codes verbatim.
//! - `row: null` on the wire means "sheet-level" — mirrored by
//!   `row: Option<i64>` here.
//! - `details` is a free-form map in TS; keep it as `serde_json::Value`
//!   so we don't lock variant payloads to a schema that'll churn as
//!   normalization rules evolve.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Non-fatal normalisation notice surfaced after an import attempt.
///
/// Codes are kept in lockstep with `ImportWarningCode` in
/// `src/lib/types.ts`. Changing one here means changing both — the
/// real-client fixture test asserts against the string codes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum ImportWarning {
    /// Sheet-level: additional worksheets beyond sheet 1 were ignored.
    ExtraSheetsIgnored {
        #[serde(default)]
        row: Option<i64>,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        details: Option<Value>,
    },
    /// Sheet-level: the `Item` column was present and ignored
    /// (duplicates `Recipe`).
    ItemColumnIgnored {
        #[serde(default)]
        row: Option<i64>,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        details: Option<Value>,
    },
    /// Sheet-level: the `Quantity to Produce/Ship` column (a spreadsheet
    /// formula) was ignored.
    QtyFormulaIgnored {
        #[serde(default)]
        row: Option<i64>,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        details: Option<Value>,
    },
    /// Row-level: `Item` cell differs from `Recipe` cell. Recipe wins.
    ItemRecipeMismatch {
        #[serde(default)]
        row: Option<i64>,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        details: Option<Value>,
    },
    /// Row-level: both Frozen and Chilled columns are marked. Defaults
    /// to frozen.
    StorageBothSet {
        #[serde(default)]
        row: Option<i64>,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        details: Option<Value>,
    },
    /// Row-level: neither Frozen nor Chilled is marked. Defaults to
    /// chilled.
    StorageMissingDefaultedChilled {
        #[serde(default)]
        row: Option<i64>,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        details: Option<Value>,
    },
    /// Row-level: meal_category value was case-normalised to its
    /// canonical form (e.g. `"LUnch"` → `"Lunch"`).
    MealCaseNormalized {
        #[serde(default)]
        row: Option<i64>,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        details: Option<Value>,
    },
    /// Sheet-level: a boilerplate marker row (packing-slip header,
    /// office-use block, etc.) was reached. Reading stopped there.
    BoilerplateRowStopped {
        #[serde(default)]
        row: Option<i64>,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        details: Option<Value>,
    },
}

impl ImportWarning {
    /// String form of the discriminator — handy in tests and logs
    /// that need to count by code without match-destructuring.
    pub fn code(&self) -> &'static str {
        match self {
            Self::ExtraSheetsIgnored { .. } => "extra_sheets_ignored",
            Self::ItemColumnIgnored { .. } => "item_column_ignored",
            Self::QtyFormulaIgnored { .. } => "qty_formula_ignored",
            Self::ItemRecipeMismatch { .. } => "item_recipe_mismatch",
            Self::StorageBothSet { .. } => "storage_both_set",
            Self::StorageMissingDefaultedChilled { .. } => "storage_missing_defaulted_chilled",
            Self::MealCaseNormalized { .. } => "meal_case_normalized",
            Self::BoilerplateRowStopped { .. } => "boilerplate_row_stopped",
        }
    }

    /// Sheet row this warning attaches to, if any.
    pub fn row(&self) -> Option<i64> {
        match self {
            Self::ExtraSheetsIgnored { row, .. }
            | Self::ItemColumnIgnored { row, .. }
            | Self::QtyFormulaIgnored { row, .. }
            | Self::ItemRecipeMismatch { row, .. }
            | Self::StorageBothSet { row, .. }
            | Self::StorageMissingDefaultedChilled { row, .. }
            | Self::MealCaseNormalized { row, .. }
            | Self::BoilerplateRowStopped { row, .. } => *row,
        }
    }
}

/// Hard error attached to a specific row. Mirrors TS `ImportRowError`:
/// a validation / resolution failure that aborts the whole import
/// (none of the rows get inserted when any row produces an error).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportRowError {
    /// 1-based sheet row index (header row is row 1 → first data row
    /// is 2).
    pub row: i64,
    /// Field name on the parsed row, or `None` for row-level errors
    /// (company/recipe not found).
    #[serde(default)]
    pub field: Option<String>,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_value: Option<String>,
    /// `"error"` by default; the TS side lets a row carry a `"warning"`
    /// severity, which we keep for shape compatibility even though the
    /// current importer only emits errors here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

/// Response from the commit endpoint. Byte-shape must stay identical
/// to the TS `ImportResult` so the SvelteKit route can forward it
/// without translation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportResult {
    pub inserted: i64,
    pub skipped: i64,
    pub errors: Vec<ImportRowError>,
    pub warnings: Vec<ImportWarning>,
}

/// Response from the preview endpoint. Distinct shape from
/// `ImportResult` — `would_insert` instead of `inserted` so a caller
/// can't accidentally render preview numbers as if rows were
/// committed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportPreview {
    pub would_insert: i64,
    pub skipped: i64,
    pub errors: Vec<ImportRowError>,
    pub warnings: Vec<ImportWarning>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn warning_serializes_with_snake_case_code() {
        let w = ImportWarning::ItemColumnIgnored {
            row: None,
            message: "Item column duplicates Recipe; ignored.".into(),
            details: None,
        };
        let v = serde_json::to_value(&w).unwrap();
        assert_eq!(v["code"], "item_column_ignored");
        assert_eq!(v["row"], Value::Null);
        assert_eq!(v["message"], "Item column duplicates Recipe; ignored.");
        // `details: None` collapses — matches TS where the field is
        // optional and absent when there's nothing to attach.
        assert!(v.get("details").is_none());
    }

    #[test]
    fn warning_round_trips_item_recipe_mismatch() {
        let w = ImportWarning::ItemRecipeMismatch {
            row: Some(41),
            message: "Item \"Banana\" differs from Recipe \"Melon\"; Recipe wins.".into(),
            details: Some(json!({ "item": "Banana", "recipe": "Melon" })),
        };
        let s = serde_json::to_string(&w).unwrap();
        let back: ImportWarning = serde_json::from_str(&s).unwrap();
        assert_eq!(back, w);
    }

    #[test]
    fn warning_code_accessor() {
        let w = ImportWarning::StorageMissingDefaultedChilled {
            row: Some(42),
            message: "Neither Frozen nor Chilled is marked; defaulted to Chilled.".into(),
            details: None,
        };
        assert_eq!(w.code(), "storage_missing_defaulted_chilled");
        assert_eq!(w.row(), Some(42));
    }

    #[test]
    fn result_shape_matches_ts() {
        let r = ImportResult {
            inserted: 350,
            skipped: 0,
            errors: vec![],
            warnings: vec![],
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["inserted"], 350);
        assert_eq!(v["skipped"], 0);
        assert!(v["errors"].is_array());
        assert!(v["warnings"].is_array());
    }
}
