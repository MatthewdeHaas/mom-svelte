//! Fuzzy ingredient-name matcher. Mirrors
//! `src/lib/utils/fuzzy-match.ts` 1:1 — same single-row-buffer
//! Levenshtein, same `1 - lev/max` similarity, same `0.65` /
//! `4` defaults, same case-insensitive dedup, same exact-match skip.
//!
//! **UTF-16 semantics on strings matter.** TS indexes with
//! `string[i]`, which steps a UTF-16 code unit. For the ingredient
//! names this importer sees (ASCII + the odd accented char like
//! "Crème" / "Jalapeño"), a BMP char is one `char` in Rust and one
//! code unit in JS, so iterating Rust `char`s produces identical
//! distances. Astral-plane characters (emoji) would diverge, but
//! ingredient names don't carry them in practice. Keeping the
//! choice explicit here so a future "add emoji support" PR knows
//! what to fix.

use std::collections::HashSet;

use domain::recipes::FuzzyMatch;

/// Default threshold — a 0.65 similarity means at most ~35% of the
/// longer string's characters differ. Picked to catch "Flouur" vs
/// "Flour" without false-flagging "Carrot" vs "Celery".
pub const DEFAULT_THRESHOLD: f64 = 0.65;

/// Minimum input length to attempt fuzzy matching. Avoids spamming
/// warnings on 1-3 letter abbreviations where every pair has a high
/// similarity by accident.
pub const DEFAULT_MIN_LENGTH: usize = 4;

/// Levenshtein edit distance. Single-row buffer, O(n*m) time, O(m)
/// space — exact port of the TS implementation.
///
/// Iterates on `chars()` (Unicode scalar values) rather than bytes so
/// non-ASCII ingredient names produce the same distances TS does.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let la = a_chars.len();
    let lb = b_chars.len();
    if la == 0 {
        return lb;
    }
    if lb == 0 {
        return la;
    }

    // Two rolling rows of size (lb + 1).
    let mut prev: Vec<usize> = (0..=lb).collect();
    let mut curr: Vec<usize> = vec![0; lb + 1];

    for i in 1..=la {
        curr[0] = i;
        for j in 1..=lb {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            let del = prev[j] + 1;
            let ins = curr[j - 1] + 1;
            let sub = prev[j - 1] + cost;
            curr[j] = del.min(ins).min(sub);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[lb]
}

/// Normalised similarity: `1 - levenshtein / max(len)`. `0` → unrelated,
/// `1` → identical. Two empty strings are considered identical to
/// match the TS `maxLen === 0 → 1` branch.
pub fn similarity(a: &str, b: &str) -> f64 {
    let la = a.chars().count();
    let lb = b.chars().count();
    let max_len = la.max(lb);
    if max_len == 0 {
        return 1.0;
    }
    let d = levenshtein(a, b) as f64;
    1.0 - d / (max_len as f64)
}

/// For each unique input name that doesn't exactly match an existing
/// name (case-insensitive), return the closest existing name if its
/// similarity meets `threshold`. Inputs below `min_length` after
/// lowercasing+trim are skipped silently.
///
/// Preserves the ORIGINAL casing of the input and of the best match on
/// the way out — the TS surface shows the user what they typed vs.
/// what the DB has.
pub fn find_fuzzy_warnings(
    inputs: &[String],
    existing: &[String],
    threshold: f64,
    min_length: usize,
) -> Vec<FuzzyMatch> {
    let existing_lower: Vec<String> = existing.iter().map(|n| n.to_lowercase()).collect();
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<FuzzyMatch> = Vec::new();

    for input in inputs {
        let lower = input.to_lowercase();
        let lower_trim = lower.trim().to_string();

        // Dedup on the lowercased key. First occurrence wins (TS also
        // takes the first — `seen.has(lower)` guard).
        if !seen.insert(lower_trim.clone()) {
            continue;
        }

        if lower_trim.chars().count() < min_length {
            continue;
        }
        if existing_lower.iter().any(|e| e == &lower_trim) {
            // Exact (case-insensitive) match — no warning.
            continue;
        }

        let mut best_score: f64 = 0.0;
        let mut best_match: &str = "";
        for (i, e_lower) in existing_lower.iter().enumerate() {
            let score = similarity(&lower_trim, e_lower);
            if score > best_score {
                best_score = score;
                // Report with the original (preserved-case) name.
                best_match = existing[i].as_str();
            }
        }

        if best_score >= threshold {
            out.push(FuzzyMatch {
                input: input.clone(),
                match_name: best_match.to_string(),
                score: best_score,
            });
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levenshtein_identical_is_zero() {
        assert_eq!(levenshtein("flour", "flour"), 0);
    }

    #[test]
    fn levenshtein_empty_cases() {
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("", ""), 0);
    }

    #[test]
    fn levenshtein_single_edit() {
        assert_eq!(levenshtein("flour", "flouur"), 1); // insertion
        assert_eq!(levenshtein("flour", "flor"), 1); // deletion
        assert_eq!(levenshtein("flour", "flouz"), 1); // substitution
    }

    #[test]
    fn similarity_identical_strings() {
        assert_eq!(similarity("flour", "flour"), 1.0);
    }

    #[test]
    fn similarity_both_empty_is_one() {
        assert_eq!(similarity("", ""), 1.0);
    }

    #[test]
    fn similarity_unrelated_is_zero_ish() {
        let s = similarity("abc", "xyz");
        assert!(s < 0.1);
    }

    #[test]
    fn finds_near_duplicate_above_threshold() {
        let inputs = vec!["Flouur".to_string()];
        let existing = vec!["Flour".to_string()];
        let out = find_fuzzy_warnings(&inputs, &existing, 0.65, 4);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].input, "Flouur");
        assert_eq!(out[0].match_name, "Flour");
        assert!(out[0].score > 0.65);
    }

    #[test]
    fn exact_case_insensitive_match_emits_no_warning() {
        let inputs = vec!["FLOUR".to_string()];
        let existing = vec!["Flour".to_string()];
        let out = find_fuzzy_warnings(&inputs, &existing, 0.65, 4);
        assert!(out.is_empty());
    }

    #[test]
    fn below_min_length_is_skipped() {
        let inputs = vec!["abc".to_string()]; // 3 chars < default 4
        let existing = vec!["abx".to_string()];
        let out = find_fuzzy_warnings(&inputs, &existing, 0.65, 4);
        assert!(out.is_empty());
    }

    #[test]
    fn deduplicates_inputs_case_insensitively() {
        let inputs = vec![
            "Flouur".to_string(),
            "flouur".to_string(),
            "FLOUUR".to_string(),
        ];
        let existing = vec!["Flour".to_string()];
        let out = find_fuzzy_warnings(&inputs, &existing, 0.65, 4);
        assert_eq!(out.len(), 1); // only one warning for the three variants
        assert_eq!(out[0].input, "Flouur"); // first occurrence wins
    }

    #[test]
    fn preserves_original_case_in_match() {
        let inputs = vec!["flouur".to_string()];
        let existing = vec!["Flour".to_string()]; // DB has capital F
        let out = find_fuzzy_warnings(&inputs, &existing, 0.65, 4);
        assert_eq!(out[0].match_name, "Flour");
    }

    #[test]
    fn no_existing_means_no_warnings() {
        let inputs = vec!["Flour".to_string()];
        let existing: Vec<String> = vec![];
        let out = find_fuzzy_warnings(&inputs, &existing, 0.65, 4);
        assert!(out.is_empty());
    }

    #[test]
    fn unrelated_names_do_not_warn() {
        let inputs = vec!["Chicken".to_string()];
        let existing = vec!["Salt".to_string(), "Butter".to_string()];
        let out = find_fuzzy_warnings(&inputs, &existing, 0.65, 4);
        assert!(out.is_empty());
    }

    #[test]
    fn non_ascii_chars_counted_as_single_units() {
        // "Crème" vs "Creme" — one substitution, length 5.
        assert_eq!(levenshtein("Crème", "Creme"), 1);
        let s = similarity("Crème", "Creme");
        assert!((s - 0.8).abs() < 1e-9);
    }
}
