//! Boilerplate-row detection. Hand-maintained workbooks trail off
//! into packing-slip templates and office-use notes once the real
//! orders end — the importer stops at the first row whose
//! Customer/Recipe/Item cell matches one of the known tokens, or
//! which looks like a meal counter (`#3 of 5`).
//!
//! Mirrors `BOILERPLATE_TOKENS` + `isBoilerplateRow` in
//! `src/lib/server/queries/imports.ts:433-460`.

use once_cell::sync::Lazy;
use regex::Regex;

/// Exact lowercase strings that signal "end of real data". Match is
/// `trim().to_lowercase() == token` OR starts-with — TS uses the
/// same two-pronged match.
const BOILERPLATE_TOKENS: &[&str] = &[
    "packing slip",
    "office use only",
    "cambros",
    "packed by",
    "ice packs - please return to mom",
];

/// `#3 of 5` style meal counters. Case-insensitive; matches "# of"
/// with any text in between (word-boundary on "of" so random words
/// like "offset" don't trigger).
static COUNTER_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^#.*\bof\b").unwrap());

/// Decide whether a row is boilerplate given its Customer/Recipe/Item
/// cell values. Only string cells are considered — a numeric amount
/// column alone never triggers the stop.
pub fn is_boilerplate(customer: Option<&str>, recipe: Option<&str>, item: Option<&str>) -> bool {
    for raw in [customer, recipe, item].into_iter().flatten() {
        let t = raw.trim().to_ascii_lowercase();
        if t.is_empty() {
            continue;
        }
        if BOILERPLATE_TOKENS
            .iter()
            .any(|tok| t == *tok || t.starts_with(tok))
        {
            return true;
        }
        if COUNTER_RE.is_match(&t) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_packing_slip() {
        assert!(is_boilerplate(Some("PACKING SLIP"), None, None));
        assert!(is_boilerplate(None, Some("Packing Slip"), None));
    }

    #[test]
    fn matches_starts_with() {
        // Real-client fixture: "PACKING SLIP — Company A" row.
        assert!(is_boilerplate(Some("Packing Slip — Company A"), None, None));
    }

    #[test]
    fn matches_counter_pattern() {
        assert!(is_boilerplate(Some("#3 of 5"), None, None));
        assert!(is_boilerplate(Some("# 10 OF 12"), None, None));
    }

    #[test]
    fn ignores_normal_rows() {
        assert!(!is_boilerplate(
            Some("Acme Catering"),
            Some("Chicken Curry"),
            None
        ));
        assert!(!is_boilerplate(Some(""), Some(""), Some("")));
        assert!(!is_boilerplate(None, None, None));
    }

    #[test]
    fn ignores_substrings_without_boundary() {
        // "office supplies" doesn't start with "office use only".
        assert!(!is_boilerplate(Some("office supplies"), None, None));
    }

    #[test]
    fn counter_needs_word_boundary_on_of() {
        // "offset" should not trigger — regex requires `\bof\b`.
        assert!(!is_boilerplate(Some("#1 offset"), None, None));
    }

    #[test]
    fn item_column_also_scanned() {
        assert!(is_boilerplate(None, None, Some("Cambros")));
    }
}
