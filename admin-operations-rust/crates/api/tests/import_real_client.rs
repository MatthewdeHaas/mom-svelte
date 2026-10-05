//! Rust-side parity test for the real-client Excel fixture.
//!
//! Mirrors `tests/integration/excel/real-client.test.ts` — same two
//! fixtures (`real-client-week-3.xlsx` messy + `-clean.xlsx` sanitized),
//! same assertions: 350 inserts, no errors, the exact 5-code warning
//! taxonomy with specific counts.
//!
//! Each test gets a fresh DB via `#[sqlx::test]`, so there's no
//! namespace-prefix dance like the TS test does; we seed the raw
//! Customer/Recipe names straight from the fixture.
//!
//! Transport: real HTTP from `reqwest` to the live router mounted on an
//! ephemeral `TcpListener`. If the HMAC middleware ever regresses, this
//! test will 401 before even starting the import.

mod common;

use std::collections::HashMap;

use common::{
    extract_fixture_names, seed_companies_and_recipes, seed_user, signed_headers, spawn_server,
};
use domain::{ImportResult, ImportWarning};
use reqwest::multipart;
use sqlx::PgPool;

const MESSY: &[u8] =
    include_bytes!("../../../../tests/integration/excel/fixtures/real-client-week-3.xlsx");
const CLEAN: &[u8] =
    include_bytes!("../../../../tests/integration/excel/fixtures/real-client-week-3-clean.xlsx");

/// Count warnings by code — handy for the taxonomy assertions.
fn count_by_code(warnings: &[ImportWarning]) -> HashMap<&'static str, usize> {
    let mut counts = HashMap::new();
    for w in warnings {
        *counts.entry(w.code()).or_insert(0) += 1;
    }
    counts
}

/// Common path: seed fixture names + user, POST to `/api/import/commit`
/// with HMAC headers, return the decoded body. Panics on anything that
/// isn't a 200 — a non-200 means the test harness is wrong, not the
/// importer.
async fn import_fixture(pool: PgPool, bytes: &'static [u8]) -> ImportResult {
    let names = extract_fixture_names(bytes);
    seed_companies_and_recipes(&pool, &names).await;
    let user = seed_user(&pool).await;

    let server = spawn_server(pool).await;

    let form = multipart::Form::new().part(
        "file",
        multipart::Part::bytes(bytes)
            .file_name("fixture.xlsx")
            .mime_str("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")
            .expect("mime"),
    );

    let mut req = reqwest::Client::new()
        .post(format!("{}/api/import/commit", server.base_url))
        .multipart(form);
    for (k, v) in signed_headers(&user.id, "admin", "") {
        req = req.header(k, v);
    }
    let res = req.send().await.expect("POST /api/import/commit");

    let status = res.status();
    let body = res.text().await.expect("body");
    assert!(
        status.is_success(),
        "import failed: status={status} body={body}"
    );

    let result: ImportResult = serde_json::from_str(&body).expect("decode ImportResult");

    server.stop().await;
    result
}

// ---------------------------------------------------------------------------
// Messy fixture — the real client's week-3 workbook, structurally
// faithful. Expected: 350 inserts, 5-code taxonomy, no errors.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn messy_fixture_inserts_350_and_emits_5_code_taxonomy(pool: PgPool) {
    let result = import_fixture(pool, MESSY).await;

    assert_eq!(result.errors, Vec::new(), "no row errors expected");
    assert_eq!(result.inserted, 350);
    assert_eq!(result.skipped, 0);

    let counts = count_by_code(&result.warnings);

    // Sheet-level — one of each.
    assert_eq!(
        counts.get("extra_sheets_ignored").copied(),
        Some(1),
        "exactly 1 extra_sheets_ignored (6 sheets dropped)"
    );
    assert_eq!(
        counts.get("item_column_ignored").copied(),
        Some(1),
        "exactly 1 item_column_ignored"
    );
    assert_eq!(
        counts.get("qty_formula_ignored").copied(),
        Some(1),
        "exactly 1 qty_formula_ignored"
    );

    // Row-level.
    assert_eq!(
        counts.get("storage_missing_defaulted_chilled").copied(),
        Some(8),
        "exactly 8 rows default to chilled"
    );
    assert_eq!(
        counts.get("item_recipe_mismatch").copied(),
        Some(1),
        "exactly 1 item_recipe_mismatch (row 41 Banana vs Melon)"
    );

    // Must-not-emit codes.
    assert_eq!(
        counts.get("boilerplate_row_stopped").copied(),
        None,
        "sheet 1 has no boilerplate trailer"
    );
    assert_eq!(
        counts.get("storage_both_set").copied(),
        None,
        "no rows have both Frozen and Chilled set"
    );
    assert_eq!(
        counts.get("meal_case_normalized").copied(),
        None,
        "meal values already canonical in this fixture"
    );

    // Row 41 identity + payload details — the specific-row assertion
    // the TS oracle relies on.
    let mismatch = result
        .warnings
        .iter()
        .find(|w| w.code() == "item_recipe_mismatch")
        .expect("item_recipe_mismatch present");
    assert_eq!(
        mismatch.row(),
        Some(41),
        "item/recipe mismatch is on row 41"
    );
    if let ImportWarning::ItemRecipeMismatch { details, .. } = mismatch {
        let d = details.as_ref().expect("details populated");
        assert_eq!(d.get("item").and_then(|v| v.as_str()), Some("Banana"));
        assert_eq!(d.get("recipe").and_then(|v| v.as_str()), Some("Melon"));
    } else {
        unreachable!("code() narrows the variant");
    }

    // Taxonomy guard — nothing we didn't expect.
    let expected = [
        "extra_sheets_ignored",
        "item_column_ignored",
        "qty_formula_ignored",
        "storage_missing_defaulted_chilled",
        "item_recipe_mismatch",
    ];
    for code in counts.keys() {
        assert!(
            expected.contains(code),
            "unexpected warning code {code:?} — update the taxonomy guard if intentional"
        );
    }
}

// ---------------------------------------------------------------------------
// Clean fixture — same 350 orders through the clean 10-column template.
// Expected: 350 inserts, zero warnings.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn clean_fixture_inserts_350_with_no_warnings(pool: PgPool) {
    let result = import_fixture(pool, CLEAN).await;

    assert_eq!(result.errors, Vec::new(), "no row errors expected");
    assert_eq!(result.inserted, 350);
    assert_eq!(result.skipped, 0);
    assert_eq!(
        result.warnings,
        Vec::new(),
        "clean fixture should emit zero warnings"
    );
}
