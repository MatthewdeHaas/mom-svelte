//! `recipes-normalise` — one-shot tool that converts the admin's
//! 3-column (`Recipe, Amount, Ingredient`) Recipes MoM sheet into the
//! 5-column shape the recipes importer expects
//! (`Recipe, Ingredient, Amount, Unit, Allergens`).
//!
//! Why: the real-client `data/Recipes MoM.xlsx` carries unit-baked-in
//! Amount cells (`"100g"`, `"1.00 ml"`, `"0.10 g"`) and no Allergens
//! column at all. The parser now accepts a column-absent Allergens
//! (D2) and emits a friendly error for `<number><letters>` Amount
//! cells (D3), but asking the admin to split 1,698 cells by hand
//! isn't realistic. This binary does the split in one pass, pre-fills
//! Allergens from a tight exact-match whitelist, and writes a sibling
//! file with ` - normalized` appended.
//!
//! Usage:
//!     cargo run -p excel --example recipes-normalise -- path/to/Recipes.xlsx
//!     cargo run --release -p excel --example recipes-normalise -- "data/Recipes MoM.xlsx"
//!
//! Output: `<input stem> - normalized.xlsx` beside the input. Nothing
//! gets committed — the admin reviews the sheet and uploads it through
//! `/admin/recipes/new` as they would any other recipe sheet.
//!
//! Audit log goes to stderr in two sections — ambiguous Amount cells
//! (left blank in output, parser will error on upload) and allergen
//! pre-fills applied (so the admin can override any judgment call,
//! e.g. Oats → Gluten which is cross-contamination-driven rather than
//! strictly correct). See D4 + D5 of
//! `docs/handoff-recipes-importer.md` for the decisions here.
//!
//! The allergen whitelist is INTENTIONALLY narrow — allergens are
//! legal/safety data, and a fuzzy classifier that gets it wrong is
//! worse than a blank cell. See `ALLERGEN_WHITELIST` below.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use calamine::{open_workbook_auto, Data, Reader};
use rust_xlsxwriter::Workbook;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let Some(input) = args.get(1) else {
        eprintln!("usage: recipes-normalise <input.xlsx>");
        return ExitCode::from(2);
    };
    if args.len() > 2 {
        eprintln!("expected one argument, got {}", args.len() - 1);
        return ExitCode::from(2);
    }

    match run(Path::new(input)) {
        Ok(output_path) => {
            eprintln!("\nwrote {}", output_path.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

/// End-to-end: read input xlsx → split rows → write output xlsx.
///
/// `run` deliberately keeps the three steps separate (read, transform,
/// write) so the transform is pure and testable (see the `#[cfg(test)]`
/// module at the bottom). I/O lives at the edges.
fn run(input: &Path) -> Result<PathBuf, String> {
    if !input.exists() {
        return Err(format!("input not found: {}", input.display()));
    }

    let rows = read_input(input)?;
    let output_path = output_path_for(input);
    let (normalised, report) = normalise_rows(&rows);
    write_output(&output_path, &normalised)?;
    emit_report(&report);
    Ok(output_path)
}

// ---------------------------------------------------------------------------
// Input / output shapes
// ---------------------------------------------------------------------------

/// One raw row off the admin's sheet — all fields optional because the
/// transform decides how to handle blanks row-by-row. `_row_number` is
/// the 1-based sheet row index (row 1 = header, data starts at row 2).
#[derive(Debug, Clone)]
struct RawRow {
    row_number: usize,
    recipe: Option<String>,
    amount_cell: Option<String>,
    ingredient: Option<String>,
}

/// One normalised output row — exactly five columns, in the order the
/// recipes importer expects. Each field is `Option<String>` so we
/// write blanks as truly empty cells rather than empty strings (the
/// importer treats both the same, but the sheet looks cleaner).
#[derive(Debug, Clone)]
struct OutputRow {
    recipe: Option<String>,
    ingredient: Option<String>,
    amount: Option<String>,
    unit: Option<String>,
    allergens: Option<String>,
}

/// Audit entries we emit to stderr. Collected during the transform so
/// the test module can assert on them without parsing stderr.
#[derive(Debug, Default)]
struct Report {
    ambiguous_amounts: Vec<AmbiguousAmount>,
    allergen_prefills: Vec<AllergenPrefill>,
}

#[derive(Debug, Clone, PartialEq)]
struct AmbiguousAmount {
    row: usize,
    raw: String,
}

#[derive(Debug, Clone, PartialEq)]
struct AllergenPrefill {
    row: usize,
    ingredient: String,
    allergens: String,
    judgment_call: bool,
}

// ---------------------------------------------------------------------------
// I/O
// ---------------------------------------------------------------------------

/// Read the first worksheet via calamine. Column lookup is tolerant:
/// accepts any case / whitespace variant of `Recipe`, `Amount`,
/// `Ingredient` because the admin's sheets drift over time.
fn read_input(path: &Path) -> Result<Vec<RawRow>, String> {
    let mut wb = open_workbook_auto(path).map_err(|e| format!("open xlsx: {e}"))?;
    let sheet_name = wb
        .sheet_names()
        .first()
        .cloned()
        .ok_or_else(|| "no sheets in workbook".to_string())?;
    let range = wb
        .worksheet_range(&sheet_name)
        .map_err(|e| format!("read sheet 1: {e}"))?;

    let mut iter = range.rows();
    let header = iter
        .next()
        .ok_or_else(|| "sheet is empty (no header row)".to_string())?;

    // Find columns by header name — order of the admin's sheet is
    // `Recipe, Amount, Ingredient` but we don't rely on it.
    let find = |name: &str| -> Option<usize> {
        header
            .iter()
            .position(|c| normalise_header(&cell_text(c)) == name)
    };
    let i_recipe = find("recipe").ok_or_else(|| "missing header: Recipe".to_string())?;
    let i_amount = find("amount").ok_or_else(|| "missing header: Amount".to_string())?;
    let i_ingredient =
        find("ingredient").ok_or_else(|| "missing header: Ingredient".to_string())?;

    let mut out = Vec::with_capacity(range.height());
    for (idx, row) in iter.enumerate() {
        let row_number = idx + 2;
        let recipe = row.get(i_recipe).map(cell_text).and_then(non_empty);
        let amount_cell = row.get(i_amount).map(cell_text).and_then(non_empty);
        let ingredient = row.get(i_ingredient).map(cell_text).and_then(non_empty);
        // Skip wholly-blank rows — the admin's sheet has them scattered
        // through for visual spacing.
        if recipe.is_none() && amount_cell.is_none() && ingredient.is_none() {
            continue;
        }
        out.push(RawRow {
            row_number,
            recipe,
            amount_cell,
            ingredient,
        });
    }
    Ok(out)
}

/// Build the output path by appending ` - normalized` to the stem,
/// keeping the parent directory and extension unchanged. Preserves
/// unicode / spaces in the original filename.
fn output_path_for(input: &Path) -> PathBuf {
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "recipes".to_string());
    let ext = input
        .extension()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "xlsx".to_string());
    parent.join(format!("{stem} - normalized.{ext}"))
}

/// Write the 5-column output with `rust_xlsxwriter`. Overwrites any
/// existing file at the target path — the admin is expected to
/// discard and re-run the tool if they want a fresh output.
fn write_output(path: &Path, rows: &[OutputRow]) -> Result<(), String> {
    // Overwrite any stale output. Separate check so the user sees a
    // specific "replacing existing output" line rather than
    // `rust_xlsxwriter`'s underlying I/O error for an open file.
    if path.exists() {
        fs::remove_file(path).map_err(|e| format!("remove stale output: {e}"))?;
    }

    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    let headers = ["Recipe", "Ingredient", "Amount", "Unit", "Allergens"];
    for (col, h) in headers.iter().enumerate() {
        ws.write_string(0, col as u16, *h)
            .map_err(|e| format!("write header: {e}"))?;
    }
    for (ri, row) in rows.iter().enumerate() {
        let r = (ri + 1) as u32;
        if let Some(ref s) = row.recipe {
            ws.write_string(r, 0, s).map_err(stringify_write_err)?;
        }
        if let Some(ref s) = row.ingredient {
            ws.write_string(r, 1, s).map_err(stringify_write_err)?;
        }
        if let Some(ref s) = row.amount {
            // Amount goes out as a number when it round-trips through
            // f64 — otherwise the importer sees it as a string and
            // picks it up via the fallback path. Both work; writing
            // the number is a courtesy for readability in Excel.
            if let Ok(n) = s.parse::<f64>() {
                ws.write_number(r, 2, n).map_err(stringify_write_err)?;
            } else {
                ws.write_string(r, 2, s).map_err(stringify_write_err)?;
            }
        }
        if let Some(ref s) = row.unit {
            ws.write_string(r, 3, s).map_err(stringify_write_err)?;
        }
        if let Some(ref s) = row.allergens {
            ws.write_string(r, 4, s).map_err(stringify_write_err)?;
        }
    }
    wb.save(path).map_err(|e| format!("save xlsx: {e}"))?;
    Ok(())
}

fn stringify_write_err(e: rust_xlsxwriter::XlsxError) -> String {
    format!("write cell: {e}")
}

fn emit_report(r: &Report) {
    eprintln!("=== ambiguous Amount cells (left blank in output) ===");
    if r.ambiguous_amounts.is_empty() {
        eprintln!("(none)");
    } else {
        for a in &r.ambiguous_amounts {
            eprintln!("row {}: {:?}", a.row, a.raw);
        }
    }

    eprintln!();
    eprintln!("=== allergen pre-fills applied ===");
    if r.allergen_prefills.is_empty() {
        eprintln!("(none)");
    } else {
        for p in &r.allergen_prefills {
            let flag = if p.judgment_call { " [judgment]" } else { "" };
            eprintln!(
                "row {}: Ingredient {:?} → Allergens {:?}{}",
                p.row, p.ingredient, p.allergens, flag
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Transform — the pure core
// ---------------------------------------------------------------------------

/// Split each `RawRow` into an `OutputRow` + append audit entries to a
/// shared `Report`. Fully pure: tests feed in a `&[RawRow]` and
/// inspect both outputs without touching the filesystem.
fn normalise_rows(rows: &[RawRow]) -> (Vec<OutputRow>, Report) {
    let mut out = Vec::with_capacity(rows.len());
    let mut report = Report::default();

    for raw in rows {
        let (amount, unit) = match raw.amount_cell.as_deref() {
            None => (None, None),
            Some(cell) => match split_amount_cell(cell) {
                Some((a, u)) => (Some(a), Some(u)),
                None => {
                    // Ambiguous shape — log it and leave Amount+Unit
                    // blank so the importer errors on upload with the
                    // "ingredient rows must have amount and unit"
                    // message the admin can act on. Trying to guess
                    // further ("a pinch" → 1 tsp?) is exactly the
                    // kind of silent correction allergen-adjacent
                    // data mustn't carry.
                    report.ambiguous_amounts.push(AmbiguousAmount {
                        row: raw.row_number,
                        raw: cell.to_string(),
                    });
                    (None, None)
                }
            },
        };

        let (allergens, prefill) = match raw.ingredient.as_deref() {
            Some(name) => match lookup_allergens(name) {
                Some(a) => {
                    report.allergen_prefills.push(AllergenPrefill {
                        row: raw.row_number,
                        ingredient: name.to_string(),
                        allergens: a.allergens.to_string(),
                        judgment_call: a.judgment_call,
                    });
                    (Some(a.allergens.to_string()), true)
                }
                None => (None, false),
            },
            None => (None, false),
        };
        let _ = prefill; // retained for symmetry with ambiguous-amount pattern

        out.push(OutputRow {
            recipe: raw.recipe.clone(),
            ingredient: raw.ingredient.clone(),
            amount,
            unit,
            allergens,
        });
    }

    (out, report)
}

/// Try to split an Amount cell into `(number_string, unit_string)`.
///
/// Grammar matches the D4 regex `^(\d+(?:\.\d+)?)\s*([a-zA-Z]+)$`
/// with one accommodation: a bare numeric cell (`"100"`) also
/// succeeds, returning `(number_string, "".to_string())` — this
/// path is NOT taken, because callers want ambiguous → blank, not
/// ambiguous → unit-missing. So we strictly require letters at the
/// end. Pure numeric cells also return `None` (ambiguous), which
/// prompts the admin to add a unit by hand.
///
/// Reuses the hand-rolled scanner from
/// `crates/excel/src/recipes.rs::split_amount_with_unit` conceptually
/// — kept inline here to keep the example self-contained (D7 of the
/// handoff is that the two binaries are standalone from each other).
fn split_amount_cell(s: &str) -> Option<(String, String)> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let bytes = t.as_bytes();

    // Grammar: `\d*(\.\d+)?\s*[a-zA-Z]+$` — the integer part is now
    // optional so leading-decimal cells like `.06 ea`, `.2 g` (common
    // on the real-client Recipes MoM sheet) succeed. A pure dot or a
    // bare unit still rejects: at least one digit must appear either
    // before or after the dot.
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    let int_end = i;
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
    // Need at least one digit SOMEWHERE in the number.
    if int_end == 0 && i == 0 {
        return None;
    }
    let num_end = i;

    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    let unit_start = i;

    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
        i += 1;
    }
    if i == unit_start {
        return None;
    }
    if i != bytes.len() {
        return None;
    }

    // Leading-decimal values (`.06`) become `0.06` so the output cell
    // round-trips through f64 cleanly — Excel displays `.06` as the
    // string `.06` on write, which looks like a formatting glitch.
    let num_str = &t[..num_end];
    let normalised_num = if num_str.starts_with('.') {
        format!("0{num_str}")
    } else {
        num_str.to_string()
    };
    Some((normalised_num, t[unit_start..].to_lowercase()))
}

// ---------------------------------------------------------------------------
// D5 — Allergen whitelist
// ---------------------------------------------------------------------------

/// One whitelist entry — the Ingredient cell text (matched
/// case-insensitively, exact match on the trimmed string) and the
/// comma-separated allergens string to pre-fill.
///
/// `judgment_call` flags entries where the mapping is defensible but
/// not strictly correct — the Oats → Gluten case especially, since
/// pure oats are gluten-free but cross-contamination with wheat is
/// the norm for commercial oats. The admin can override by editing
/// the Allergens cell in the output sheet.
#[derive(Debug, Clone, Copy)]
struct AllergenEntry {
    ingredient: &'static str,
    allergens: &'static str,
    judgment_call: bool,
}

/// Exact-match allergen pre-fills. Deliberately narrow — allergens are
/// legal/safety data, and a wrong auto-fill is worse than a blank
/// cell. Everything else stays blank; the admin can fill those in
/// before upload.
///
/// Entries sourced from the D5 spec in
/// `docs/handoff-recipes-importer.md`. Add new entries by editing this
/// list, not by training a classifier.
const ALLERGEN_WHITELIST: &[AllergenEntry] = &[
    AllergenEntry {
        ingredient: "Milk",
        allergens: "Dairy",
        judgment_call: false,
    },
    AllergenEntry {
        ingredient: "Butter",
        allergens: "Dairy",
        judgment_call: false,
    },
    AllergenEntry {
        ingredient: "Cream",
        allergens: "Dairy",
        judgment_call: false,
    },
    AllergenEntry {
        ingredient: "Cheese",
        allergens: "Dairy",
        judgment_call: false,
    },
    AllergenEntry {
        ingredient: "Yogurt",
        allergens: "Dairy",
        judgment_call: false,
    },
    AllergenEntry {
        ingredient: "Egg",
        allergens: "Egg",
        judgment_call: false,
    },
    AllergenEntry {
        ingredient: "Eggs",
        allergens: "Egg",
        judgment_call: false,
    },
    AllergenEntry {
        ingredient: "Wheat Flour",
        allergens: "Gluten",
        judgment_call: false,
    },
    AllergenEntry {
        ingredient: "Peanuts",
        allergens: "Peanuts",
        judgment_call: false,
    },
    AllergenEntry {
        ingredient: "Peanut Butter",
        allergens: "Peanuts",
        judgment_call: false,
    },
    // Soy sauce is almost always wheat-containing — Soy + Gluten is the
    // safe default. Tamari (true wheat-free soy sauce) is an admin
    // override, not an auto-detect.
    AllergenEntry {
        ingredient: "Soy Sauce",
        allergens: "Soy, Gluten",
        judgment_call: true,
    },
    AllergenEntry {
        ingredient: "Tofu",
        allergens: "Soy",
        judgment_call: false,
    },
    AllergenEntry {
        ingredient: "Shrimp",
        allergens: "Shellfish",
        judgment_call: false,
    },
    AllergenEntry {
        ingredient: "Lobster",
        allergens: "Shellfish",
        judgment_call: false,
    },
    AllergenEntry {
        ingredient: "Crab",
        allergens: "Shellfish",
        judgment_call: false,
    },
    AllergenEntry {
        ingredient: "Almonds",
        allergens: "Tree Nuts",
        judgment_call: false,
    },
    AllergenEntry {
        ingredient: "Cashews",
        allergens: "Tree Nuts",
        judgment_call: false,
    },
    AllergenEntry {
        ingredient: "Walnuts",
        allergens: "Tree Nuts",
        judgment_call: false,
    },
    AllergenEntry {
        ingredient: "Hazelnuts",
        allergens: "Tree Nuts",
        judgment_call: false,
    },
    AllergenEntry {
        ingredient: "Salmon",
        allergens: "Fish",
        judgment_call: false,
    },
    AllergenEntry {
        ingredient: "Tuna",
        allergens: "Fish",
        judgment_call: false,
    },
    AllergenEntry {
        ingredient: "Cod",
        allergens: "Fish",
        judgment_call: false,
    },
    // Pure oats are gluten-free; commercial oats are nearly always
    // cross-contaminated with wheat. Default to Gluten — safer wrong
    // than silently blank — and surface it in the audit log as a
    // judgment call so the admin can override.
    AllergenEntry {
        ingredient: "Oats",
        allergens: "Gluten",
        judgment_call: true,
    },
];

/// Case-insensitive match. Tries two shapes in order:
///
/// 1. Full-string exact match on the trimmed Ingredient cell
///    (`"Milk"` → Dairy).
/// 2. Leading-token match on the segment before ` - ` — the admin's
///    naming convention on Recipes MoM uses hyphen-qualified names
///    like `"Butter - Unsalted"`, `"Salmon - Fillet"`. The leading
///    token carries the allergen; everything after it is a qualifier
///    the admin added for the kitchen.
///
/// Deliberately exact at the token level — fuzzy / substring matching
/// would catch `"Oil - Sunflower"` → Fish via `"Fish"` substring, which
/// is exactly the kind of silent wrong-answer we want to avoid on
/// allergen data.
fn lookup_allergens(ingredient: &str) -> Option<&'static AllergenEntry> {
    let needle = ingredient.trim();
    if let Some(hit) = ALLERGEN_WHITELIST
        .iter()
        .find(|e| e.ingredient.eq_ignore_ascii_case(needle))
    {
        return Some(hit);
    }

    // Try the leading token before ` - `. Admin convention: the first
    // segment is the ingredient's root name; qualifiers (brand, cut,
    // variety) follow. Matching ` - ` with spaces (not bare `-`) is
    // intentional — compound words like `"Flat-leaf parsley"` must
    // not split.
    if let Some((head, _rest)) = needle.split_once(" - ") {
        let head = head.trim();
        if !head.is_empty() {
            return ALLERGEN_WHITELIST
                .iter()
                .find(|e| e.ingredient.eq_ignore_ascii_case(head));
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// Flatten a calamine cell to a trimmed string. Numbers render without
/// a trailing `.0` for integers so the output xlsx roundtrips cleanly.
fn cell_text(c: &Data) -> String {
    match c {
        Data::String(s) => s.to_string(),
        Data::Float(f) => {
            if f.fract() == 0.0 && f.is_finite() {
                format!("{}", *f as i64)
            } else {
                format!("{f}")
            }
        }
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(d) => d.to_string(),
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.to_string(),
        Data::Error(_) | Data::Empty => String::new(),
    }
}

fn non_empty(s: String) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

/// Normalise a header for lookup — trim, lowercase, collapse any run
/// of ASCII whitespace to a single space. Matches the tolerant header
/// matching in the recipes parser (`normalise_header`) but keeps
/// spaces instead of underscores because this binary cares about the
/// exact tokens `"recipe"` / `"amount"` / `"ingredient"` only.
fn normalise_header(raw: &str) -> String {
    let trimmed = raw.trim().to_ascii_lowercase();
    let mut out = String::with_capacity(trimmed.len());
    let mut in_ws = false;
    for ch in trimmed.chars() {
        if ch.is_ascii_whitespace() {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            out.push(ch);
            in_ws = false;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests — pure transform only. I/O paths are smoke-tested by running
// the binary against the real data/ files per D4's verification plan.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(row: usize, recipe: Option<&str>, amount: Option<&str>, ing: Option<&str>) -> RawRow {
        RawRow {
            row_number: row,
            recipe: recipe.map(str::to_string),
            amount_cell: amount.map(str::to_string),
            ingredient: ing.map(str::to_string),
        }
    }

    #[test]
    fn splits_amount_with_unit() {
        assert_eq!(
            split_amount_cell("100g"),
            Some(("100".to_string(), "g".to_string()))
        );
        assert_eq!(
            split_amount_cell("1.00 ml"),
            Some(("1.00".to_string(), "ml".to_string()))
        );
        assert_eq!(
            split_amount_cell("0.10 g"),
            Some(("0.10".to_string(), "g".to_string()))
        );
    }

    #[test]
    fn ambiguous_amount_cells_return_none() {
        // Bare numbers reject — we want the admin to add a unit by hand.
        assert_eq!(split_amount_cell("100"), None);
        // Free-text rejects.
        assert_eq!(split_amount_cell("a pinch"), None);
        assert_eq!(split_amount_cell("to taste"), None);
        // Leading sign / exponent reject.
        assert_eq!(split_amount_cell("-1g"), None);
        assert_eq!(split_amount_cell("1e3g"), None);
        // Pure dot or bare unit must still reject.
        assert_eq!(split_amount_cell(".g"), None);
        assert_eq!(split_amount_cell("."), None);
    }

    #[test]
    fn leading_decimal_amounts_parse() {
        // Real-data shapes: `.06 ea`, `.08 ea`, `.2 g`. Leading-decimal
        // amounts are a common admin shorthand that used to reject
        // and surface as ambiguous.
        assert_eq!(
            split_amount_cell(".06 ea"),
            Some(("0.06".to_string(), "ea".to_string()))
        );
        assert_eq!(
            split_amount_cell(".2 g"),
            Some(("0.2".to_string(), "g".to_string()))
        );
        assert_eq!(
            split_amount_cell(".08ea"),
            Some(("0.08".to_string(), "ea".to_string()))
        );
    }

    #[test]
    fn allergen_whitelist_matches_leading_token_before_dash() {
        // Admin's naming convention: `"Butter - Unsalted"`,
        // `"Salmon - Fillet"`, `"Oil - Sunflower"`. The leading token
        // decides the allergen; qualifiers after ` - ` are ignored.
        let butter = lookup_allergens("Butter - Unsalted").expect("butter root matches");
        assert_eq!(butter.allergens, "Dairy");

        let salmon = lookup_allergens("Salmon - Fillet").expect("salmon root matches");
        assert_eq!(salmon.allergens, "Fish");

        // `"Oil - Sunflower"` must NOT match any whitelist entry —
        // `"Oil"` isn't in the list, and the qualifier `"Sunflower"`
        // mustn't be probed either (that's the exact silent-wrong-
        // answer trap this test defends against).
        assert!(lookup_allergens("Oil - Sunflower").is_none());

        // Compound words with bare `-` (no spaces) MUST NOT split.
        // `"Flat-leaf parsley"` has `-` in the middle; it's one token.
        assert!(lookup_allergens("Flat-leaf parsley").is_none());
    }

    #[test]
    fn lowercases_the_unit() {
        // Admin's sheet mixes case; the output should be consistent.
        assert_eq!(
            split_amount_cell("100G"),
            Some(("100".to_string(), "g".to_string()))
        );
        assert_eq!(
            split_amount_cell("1 ML"),
            Some(("1".to_string(), "ml".to_string()))
        );
    }

    #[test]
    fn allergen_whitelist_exact_match_case_insensitive() {
        let hit = lookup_allergens("milk").expect("milk is in the whitelist");
        assert_eq!(hit.allergens, "Dairy");
        assert!(!hit.judgment_call);

        let soy = lookup_allergens("Soy Sauce").expect("soy sauce in the whitelist");
        assert_eq!(soy.allergens, "Soy, Gluten");
        assert!(soy.judgment_call);
    }

    #[test]
    fn allergen_whitelist_rejects_fuzzy_matches() {
        // Exact-match only. "Chocolate milk" is NOT "Milk".
        assert!(lookup_allergens("Chocolate milk").is_none());
        assert!(lookup_allergens("Egg Whites").is_none());
        assert!(lookup_allergens("Peanut Oil").is_none());
    }

    #[test]
    fn normalise_rows_splits_and_prefills() {
        let rows = [
            raw(2, Some("Caesar"), Some("100g"), Some("Chicken")),
            raw(3, Some("Caesar"), Some("1.00 ml"), Some("Milk")),
            raw(4, Some("Smoothie"), Some("to taste"), Some("Banana")),
        ];
        let (out, report) = normalise_rows(&rows);

        assert_eq!(out.len(), 3);
        assert_eq!(out[0].amount.as_deref(), Some("100"));
        assert_eq!(out[0].unit.as_deref(), Some("g"));
        assert_eq!(out[0].allergens, None);

        assert_eq!(out[1].amount.as_deref(), Some("1.00"));
        assert_eq!(out[1].unit.as_deref(), Some("ml"));
        assert_eq!(out[1].allergens.as_deref(), Some("Dairy"));

        // Ambiguous Amount → both cells blank, row logged.
        assert_eq!(out[2].amount, None);
        assert_eq!(out[2].unit, None);
        assert_eq!(
            report.ambiguous_amounts,
            vec![AmbiguousAmount {
                row: 4,
                raw: "to taste".to_string(),
            }]
        );

        // Allergen prefill logged with the correct row.
        assert_eq!(report.allergen_prefills.len(), 1);
        assert_eq!(report.allergen_prefills[0].row, 3);
        assert_eq!(report.allergen_prefills[0].ingredient, "Milk");
    }

    #[test]
    fn judgment_call_flagged_in_report() {
        let rows = [raw(2, Some("Granola"), Some("50g"), Some("Oats"))];
        let (_, report) = normalise_rows(&rows);
        assert_eq!(report.allergen_prefills.len(), 1);
        assert!(report.allergen_prefills[0].judgment_call);
    }

    #[test]
    fn blank_amount_passes_through_without_logging() {
        // Recipe-only row (no ingredient, no amount) is valid in the
        // 5-column schema. The transform must preserve that without
        // adding noise to the audit log.
        let rows = [raw(2, Some("Caesar"), None, None)];
        let (out, report) = normalise_rows(&rows);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].recipe.as_deref(), Some("Caesar"));
        assert_eq!(out[0].amount, None);
        assert!(report.ambiguous_amounts.is_empty());
        assert!(report.allergen_prefills.is_empty());
    }

    #[test]
    fn output_path_appends_normalized_suffix() {
        let p = output_path_for(Path::new("/tmp/data/Recipes MoM.xlsx"));
        assert_eq!(p, PathBuf::from("/tmp/data/Recipes MoM - normalized.xlsx"));

        let p2 = output_path_for(Path::new("recipes.xlsx"));
        assert_eq!(p2, PathBuf::from("recipes - normalized.xlsx"));
    }
}
