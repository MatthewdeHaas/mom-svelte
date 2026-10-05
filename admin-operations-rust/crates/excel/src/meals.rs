//! Meal-category canonicalisation. The sheet carries values like
//! " lunch", "LUnch", "soup r" — the schema only accepts the eight
//! canonical forms. We lower-case + collapse-whitespace + strip-trim
//! the raw value, look it up, and either return the canonical form
//! (plus a `normalized: true` flag so the importer can emit a
//! `meal_case_normalized` warning) or return the raw value unchanged
//! so the Zod/validator step rejects it with a readable error.
//!
//! Mirrors `MEAL_CATEGORIES` + `MEAL_CANONICAL` + `canonicalizeMeal`
//! in `src/lib/server/queries/imports.ts:26-120`.

use once_cell::sync::Lazy;
use std::collections::HashMap;

/// Canonical meal values accepted by the schema. The `R` suffix on
/// `Lunch R` / `Snack R` / `Soup R` is real client data (meaning
/// unclear — kept intact so downstream grouping stays faithful).
pub const MEAL_CATEGORIES: [&str; 8] = [
    "Breakfast",
    "Lunch",
    "Lunch R",
    "Snack",
    "Snack R",
    "Soup",
    "Soup R",
    "Dinner",
];

/// Normalised-key → canonical form. Keys are lower-cased with runs
/// of whitespace collapsed to a single space.
static MEAL_CANONICAL: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    HashMap::from([
        ("breakfast", "Breakfast"),
        ("lunch", "Lunch"),
        ("lunch r", "Lunch R"),
        ("snack", "Snack"),
        ("snack r", "Snack R"),
        ("soup", "Soup"),
        ("soup r", "Soup R"),
        ("dinner", "Dinner"),
    ])
});

/// Outcome of a canonicalisation attempt.
///
/// - `Canonical(form)` — raw matched a known meal; `normalized` tells
///   whether the raw string was already in canonical form.
/// - `Unknown` — no match; return the raw value unchanged and let
///   validation reject it with the normal "must be one of ..."
///   error.
pub enum CanonResult<'a> {
    Canonical {
        value: &'static str,
        normalized: bool,
        original: &'a str,
    },
    Unknown,
}

pub fn canonicalize(raw: &str) -> CanonResult<'_> {
    // Mirror TS `raw.replace(/\s+/g, ' ').trim().toLowerCase()`.
    let key = raw
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    match MEAL_CANONICAL.get(key.as_str()) {
        Some(&canonical) => CanonResult::Canonical {
            value: canonical,
            // "already canonical" means byte-identical; the TS code
            // compares `canonical !== raw` (object-identity after
            // lookup, which equates to string equality in JS).
            normalized: canonical != raw,
            original: raw,
        },
        None => CanonResult::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn as_canonical<'a>(r: CanonResult<'a>) -> (&'static str, bool) {
        match r {
            CanonResult::Canonical {
                value, normalized, ..
            } => (value, normalized),
            CanonResult::Unknown => panic!("expected canonical, got Unknown"),
        }
    }

    #[test]
    fn canonicalises_lowercase() {
        let (v, n) = as_canonical(canonicalize("lunch"));
        assert_eq!(v, "Lunch");
        assert!(n);
    }

    #[test]
    fn already_canonical_sets_normalized_false() {
        let (v, n) = as_canonical(canonicalize("Lunch"));
        assert_eq!(v, "Lunch");
        assert!(!n);
    }

    #[test]
    fn handles_r_variants() {
        assert!(matches!(
            canonicalize("Lunch R"),
            CanonResult::Canonical {
                value: "Lunch R",
                normalized: false,
                ..
            }
        ));
        assert!(matches!(
            canonicalize("soup r"),
            CanonResult::Canonical {
                value: "Soup R",
                normalized: true,
                ..
            }
        ));
        assert!(matches!(
            canonicalize("SNACK  R"),
            CanonResult::Canonical {
                value: "Snack R",
                normalized: true,
                ..
            }
        ));
    }

    #[test]
    fn trims_whitespace() {
        let (v, n) = as_canonical(canonicalize(" LUnch "));
        assert_eq!(v, "Lunch");
        assert!(n);
    }

    #[test]
    fn unknown_passes_through() {
        assert!(matches!(canonicalize("Pizza"), CanonResult::Unknown));
    }
}
