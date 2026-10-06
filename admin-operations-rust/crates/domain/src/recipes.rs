//! Recipe-import domain types.
//!
//! Lives in the pure-types `domain` crate because both `excel` (parser)
//! and `db` (query layer) need to name `RecipeExcelRowParsed`, and
//! `excel` already depends on `db` for the orders importer. Putting
//! the type here breaks the would-be cycle without widening the
//! `excel` → `db` edge.
//!
//! Wire shapes mirror `src/lib/types.ts`:
//! - `RecipeExcelRowParsed` → `RecipeExcelRowParsed` (TS).
//! - `RecipeImportResult`   → `RecipeImportResult`   (TS).
//! - `FuzzyMatch`           → `FuzzyMatch`           (TS).
//! - `RecipeImportPreview`  is Rust-only (preview endpoint wire shape).
//!
//! The TS side types ids as `number`; pg actually returns UUID strings
//! for recipe/ingredient/allergen. We use `Uuid` throughout — parity
//! holds because the ids are only ever used as opaque FKs on both
//! sides. See `plans/agile-finding-scone.md` for the one-paragraph
//! history of that lie.
//!
//! Validation + parse errors are reused from `domain::import` so the
//! taxonomy (`ImportRowError`) matches the orders importer verbatim.

use serde::{Deserialize, Serialize};

use crate::import::ImportRowError;

/// One parsed+validated row off the recipe xlsx. Mirrors TS
/// `RecipeExcelRowParsed` at `src/lib/types.ts`.
///
/// `ingredient` / `amount` / `unit` are jointly nullable: a row with
/// just a recipe name (and no ingredient) is a "declare the recipe
/// exists, no ingredients" row. Validation rules live in the parser,
/// not here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecipeExcelRowParsed {
    pub recipe: String,
    pub ingredient: Option<String>,
    pub amount: Option<f64>,
    pub unit: Option<String>,
    /// Comma-separated allergen names, raw. Split and trimmed by the
    /// query layer so the wire shape matches TS verbatim.
    pub allergens: Option<String>,
}

/// Response shape from the Rust parser. Parse errors land in `errors`
/// (one per bad row); the caller aborts the import when non-empty.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseRecipesResult {
    pub rows: Vec<RecipeExcelRowParsed>,
    pub errors: Vec<ImportRowError>,
}

/// Commit response. Byte-shape must stay identical to TS
/// `RecipeImportResult` at `src/lib/types.ts`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecipeImportResult {
    pub recipes_created: i64,
    pub recipes_updated: i64,
    /// Count of distinct ingredient names seen in the input. Matches
    /// the TS approximation at `recipe-imports.ts:208` — a true "newly
    /// inserted" count would need to diff before/after and no caller
    /// does.
    pub ingredients_created: i64,
    pub rows_processed: i64,
    /// Always empty on a successful commit — a non-empty `errors` on
    /// the input means the parser short-circuited before this struct
    /// was built. Kept for shape parity with TS.
    pub errors: Vec<ImportRowError>,
}

/// Fuzzy ingredient-name match surfaced by the preview endpoint. The
/// TS side uses `match` as the field name (`FuzzyMatch` in
/// `src/lib/utils/fuzzy-match.ts`); `match` is a Rust keyword so we
/// rename on the wire.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FuzzyMatch {
    pub input: String,
    #[serde(rename = "match")]
    pub match_name: String,
    pub score: f64,
}

/// Preview response. Rust-only shape — the TS route builds its own
/// `{warnings, parsedRowCount, mode}` surface from this; see
/// `src/routes/admin/recipes/new/+page.server.ts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeImportPreview {
    pub rows_parsed: i64,
    pub parse_errors: Vec<ImportRowError>,
    pub fuzzy_warnings: Vec<FuzzyMatch>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fuzzy_match_wire_renames_to_match() {
        let m = FuzzyMatch {
            input: "Flouur".into(),
            match_name: "Flour".into(),
            score: 0.83,
        };
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v["input"], "Flouur");
        assert_eq!(v["match"], "Flour");
        assert_eq!(v["score"], json!(0.83));
    }

    #[test]
    fn row_round_trips_with_null_optionals() {
        let r = RecipeExcelRowParsed {
            recipe: "Chicken Caesar".into(),
            ingredient: None,
            amount: None,
            unit: None,
            allergens: None,
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: RecipeExcelRowParsed = serde_json::from_str(&s).unwrap();
        assert_eq!(back, r);
    }
}
