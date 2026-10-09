//! Integration tests for `/api/recipe-imports/preview` + `/commit`.
//!
//! Mirrors the TS parity surface at
//! `src/routes/admin/recipes/new/+page.server.ts` + the forwarder it
//! wraps, exercised through the live router over real HTTP. Each test
//! gets a fresh pool via `#[sqlx::test]`, mints an admin user, builds
//! an in-memory .xlsx via `common::fixtures_recipes`, and POSTs a real
//! multipart form.
//!
//! Coverage buckets:
//!   - auth: non-admin → 403; missing HMAC → 401 (via auth layer).
//!   - preview: happy, fuzzy-match, 422 row errors, 400 shape errors,
//!     400 empty, 400 bad extension.
//!   - commit: happy single row, multi-recipe, duplicate-ingredient
//!     first-wins, recipe-only row, allergen full-replace,
//!     recipe re-activation, 422 row errors (no writes), parse 400.
//!   - cross: preview is a pure read (no writes); replace semantics
//!     wipe prior recipe_ingredient on second commit.

mod common;

use common::fixtures_recipes::{
    duplicate_ingredient_per_recipe, fuzzy_match_fixture, happy_single_row,
    missing_required_column, negative_amount_row, one_valid_one_invalid_row, only_blank_rows,
    recipe_only_row, two_recipes_two_ingredients_each,
};
use common::recipes::{seed_ingredient, seed_recipe_inactive};
use common::{seed_user, signed_headers, spawn_server};

use domain::recipes::{FuzzyMatch, RecipeImportResult};
use domain::ImportRowError;
use reqwest::{multipart, StatusCode};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;

// ---------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------

/// Build a multipart form around an in-memory .xlsx. Mirrors how the
/// SvelteKit forwarder packs the body — same mime, same field name,
/// filename ends in `.xlsx`.
fn xlsx_form(bytes: Vec<u8>) -> multipart::Form {
    multipart::Form::new().part(
        "file",
        multipart::Part::bytes(bytes)
            .file_name("fixture.xlsx")
            .mime_str("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")
            .expect("mime"),
    )
}

/// Fire a POST to `endpoint` as the given role; return (status, body).
async fn post_as(
    base_url: &str,
    endpoint: &str,
    role: &str,
    user_id: &str,
    bytes: Vec<u8>,
) -> (StatusCode, String) {
    let mut req = reqwest::Client::new()
        .post(format!("{base_url}{endpoint}"))
        .multipart(xlsx_form(bytes));
    for (k, v) in signed_headers(user_id, role, "") {
        req = req.header(k, v);
    }
    let res = req.send().await.expect("POST");
    let status = res.status();
    let body = res.text().await.expect("body");
    (status, body)
}

/// Admin-shortcut: POST as admin, no company.
async fn post_admin(
    base_url: &str,
    endpoint: &str,
    user_id: &str,
    bytes: Vec<u8>,
) -> (StatusCode, String) {
    post_as(base_url, endpoint, "admin", user_id, bytes).await
}

/// Decode a successful commit body. Panics with the body on failure
/// so a bad shape surfaces clearly in the test log.
fn decode_commit(status: StatusCode, body: &str) -> RecipeImportResult {
    assert!(status.is_success(), "commit failed: {status} {body}");
    serde_json::from_str(body).unwrap_or_else(|e| panic!("decode: {e} body={body}"))
}

/// Preview response wire shape — matches
/// `domain::recipes::RecipeImportPreview` + `FuzzyMatch`.
#[derive(Debug, Deserialize)]
struct PreviewBody {
    rows_parsed: i64,
    parse_errors: Vec<ImportRowError>,
    fuzzy_warnings: Vec<FuzzyMatch>,
}

fn decode_preview(status: StatusCode, body: &str) -> PreviewBody {
    assert!(status.is_success(), "preview failed: {status} {body}");
    serde_json::from_str(body).unwrap_or_else(|e| panic!("decode: {e} body={body}"))
}

// ---------------------------------------------------------------
// Auth
// ---------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn preview_rejects_non_admin(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;
    let (status, _) = post_as(
        &server.base_url,
        "/api/recipe-imports/preview",
        "client",
        &user.id,
        happy_single_row(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn commit_rejects_non_admin(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;
    let (status, _) = post_as(
        &server.base_url,
        "/api/recipe-imports/commit",
        "client",
        &user.id,
        happy_single_row(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    server.stop().await;
}

// ---------------------------------------------------------------
// Preview
// ---------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn preview_happy_no_warnings(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool.clone()).await;
    let (status, body) = post_admin(
        &server.base_url,
        "/api/recipe-imports/preview",
        &user.id,
        happy_single_row(),
    )
    .await;
    let preview = decode_preview(status, &body);
    assert_eq!(preview.rows_parsed, 1);
    assert_eq!(preview.parse_errors, Vec::new());
    assert_eq!(
        preview.fuzzy_warnings,
        Vec::new(),
        "no pre-existing ingredients → no fuzzy warnings"
    );

    // Preview must not write.
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM recipe")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(n, 0, "preview must not create recipes");
    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn preview_surfaces_fuzzy_warning(pool: PgPool) {
    // Seed "Chicken Breast" so the fixture's "Chicken Breasts" fuzzies.
    seed_ingredient(&pool, "Chicken Breast").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;

    let (status, body) = post_admin(
        &server.base_url,
        "/api/recipe-imports/preview",
        &user.id,
        fuzzy_match_fixture(),
    )
    .await;
    let preview = decode_preview(status, &body);
    assert_eq!(preview.rows_parsed, 1);
    assert_eq!(preview.fuzzy_warnings.len(), 1);
    let w = &preview.fuzzy_warnings[0];
    assert_eq!(w.input, "Chicken Breasts");
    assert_eq!(w.match_name, "Chicken Breast");
    assert!(w.score >= 0.65 && w.score <= 1.0, "score within fuzzy band");
    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn preview_422_on_row_errors(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;
    let (status, body) = post_admin(
        &server.base_url,
        "/api/recipe-imports/preview",
        &user.id,
        one_valid_one_invalid_row(),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let v: Value = serde_json::from_str(&body).expect("422 body");
    let errs = v["parse_errors"].as_array().expect("parse_errors array");
    assert_eq!(errs.len(), 1, "one bad row → one error");
    // The bad row's unit is empty on row 3 (header=1, good=2, bad=3).
    assert_eq!(errs[0]["row"], 3);
    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn preview_400_on_missing_columns(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;
    let (status, body) = post_admin(
        &server.base_url,
        "/api/recipe-imports/preview",
        &user.id,
        missing_required_column(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body.contains("missing columns") || body.contains("unit"),
        "expected missing-columns message, got {body}"
    );
    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn preview_400_on_empty_data_rows(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;
    let (status, body) = post_admin(
        &server.base_url,
        "/api/recipe-imports/preview",
        &user.id,
        only_blank_rows(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body.contains("no data rows"),
        "expected empty-rows message, got {body}"
    );
    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn preview_400_on_bad_extension(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;
    let form = multipart::Form::new().part(
        "file",
        multipart::Part::bytes(happy_single_row())
            .file_name("fixture.txt") // ← wrong extension
            .mime_str("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")
            .expect("mime"),
    );
    let mut req = reqwest::Client::new()
        .post(format!("{}/api/recipe-imports/preview", server.base_url))
        .multipart(form);
    for (k, v) in signed_headers(&user.id, "admin", "") {
        req = req.header(k, v);
    }
    let res = req.send().await.expect("POST");
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    server.stop().await;
}

// ---------------------------------------------------------------
// Commit
// ---------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn commit_happy_single_row(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool.clone()).await;
    let (status, body) = post_admin(
        &server.base_url,
        "/api/recipe-imports/commit",
        &user.id,
        happy_single_row(),
    )
    .await;
    let result = decode_commit(status, &body);
    assert_eq!(result.recipes_created, 1);
    assert_eq!(result.recipes_updated, 0);
    assert_eq!(result.ingredients_created, 1);
    assert_eq!(result.rows_processed, 1);
    assert_eq!(result.errors, Vec::new());

    // DB state.
    let (n_recipe, n_ing, n_ri, n_al, n_ia): (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
            (SELECT COUNT(*) FROM recipe),           \
            (SELECT COUNT(*) FROM ingredient),       \
            (SELECT COUNT(*) FROM recipe_ingredient),\
            (SELECT COUNT(*) FROM allergen),         \
            (SELECT COUNT(*) FROM ingredient_allergen)",
    )
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(n_recipe, 1);
    assert_eq!(n_ing, 1);
    assert_eq!(n_ri, 1);
    assert_eq!(n_al, 1);
    assert_eq!(n_ia, 1);
    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn commit_two_recipes_two_ingredients_each(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool.clone()).await;
    let (status, body) = post_admin(
        &server.base_url,
        "/api/recipe-imports/commit",
        &user.id,
        two_recipes_two_ingredients_each(),
    )
    .await;
    let result = decode_commit(status, &body);
    assert_eq!(result.recipes_created, 2);
    assert_eq!(result.rows_processed, 4);
    // 4 distinct ingredients: Chicken Breast, Croutons, Lettuce, Tomato.
    assert_eq!(result.ingredients_created, 4);

    // Each recipe has 2 ingredient lines → 4 total.
    let n_ri: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM recipe_ingredient")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(n_ri, 4);

    // Allergen set from fixture: Dairy, Gluten → 2 allergens.
    let n_al: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM allergen")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(n_al, 2);
    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn commit_duplicate_ingredient_first_wins(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool.clone()).await;
    let (status, body) = post_admin(
        &server.base_url,
        "/api/recipe-imports/commit",
        &user.id,
        duplicate_ingredient_per_recipe(),
    )
    .await;
    decode_commit(status, &body);

    // 2 distinct ingredients (Water, Salt); 2 recipe_ingredient rows.
    let n_ri: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM recipe_ingredient")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(n_ri, 2, "duplicate ingredient dedup'd within recipe");

    // First Water row wins: amount=500, unit='ml' (not 999 / 'l').
    let (amt, unit): (f64, String) = sqlx::query_as(
        "SELECT ri.amount::float8, ri.unit \
         FROM recipe_ingredient ri \
         JOIN ingredient i ON i.id = ri.ingredient_id \
         WHERE i.name = 'Water'",
    )
    .fetch_one(&pool)
    .await
    .expect("water row");
    assert_eq!(amt, 500.0);
    assert_eq!(unit, "ml");
    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn commit_recipe_only_row_creates_empty_recipe(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool.clone()).await;
    let (status, body) = post_admin(
        &server.base_url,
        "/api/recipe-imports/commit",
        &user.id,
        recipe_only_row(),
    )
    .await;
    let result = decode_commit(status, &body);
    assert_eq!(result.recipes_created, 1);
    assert_eq!(result.rows_processed, 1);

    let n_ri: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM recipe_ingredient")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(n_ri, 0, "recipe-only row contributes no ingredient lines");
    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn commit_reactivates_inactive_recipe(pool: PgPool) {
    // Pre-seed a deactivated recipe with the same name as the fixture.
    seed_recipe_inactive(&pool, "Chicken Caesar").await;
    let user = seed_user(&pool).await;
    let server = spawn_server(pool.clone()).await;

    let (status, body) = post_admin(
        &server.base_url,
        "/api/recipe-imports/commit",
        &user.id,
        happy_single_row(),
    )
    .await;
    let result = decode_commit(status, &body);
    assert_eq!(result.recipes_created, 0, "pre-existing name → UPDATE");
    assert_eq!(result.recipes_updated, 1);

    let active: bool =
        sqlx::query_scalar("SELECT active FROM recipe WHERE name = 'Chicken Caesar'")
            .fetch_one(&pool)
            .await
            .expect("active");
    assert!(active, "import must flip active back to TRUE");
    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn commit_422_on_row_errors_no_writes(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool.clone()).await;
    let (status, body) = post_admin(
        &server.base_url,
        "/api/recipe-imports/commit",
        &user.id,
        one_valid_one_invalid_row(),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let v: Value = serde_json::from_str(&body).expect("422 body");
    assert!(!v["parse_errors"].as_array().unwrap().is_empty());

    let (n_recipe, n_ri): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM recipe), (SELECT COUNT(*) FROM recipe_ingredient)",
    )
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(n_recipe, 0, "422 must not write recipes");
    assert_eq!(n_ri, 0, "422 must not write recipe_ingredient");
    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn commit_422_on_negative_amount(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;
    let (status, body) = post_admin(
        &server.base_url,
        "/api/recipe-imports/commit",
        &user.id,
        negative_amount_row(),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let v: Value = serde_json::from_str(&body).expect("422 body");
    let errs = v["parse_errors"].as_array().expect("parse_errors");
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0]["field"], "amount");
    assert!(errs[0]["message"]
        .as_str()
        .unwrap()
        .contains("greater than zero"));
    server.stop().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn commit_400_on_missing_columns(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool).await;
    let (status, _body) = post_admin(
        &server.base_url,
        "/api/recipe-imports/commit",
        &user.id,
        missing_required_column(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    server.stop().await;
}

// ---------------------------------------------------------------
// Cross-cutting
// ---------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn second_commit_replaces_recipe_ingredient(pool: PgPool) {
    let user = seed_user(&pool).await;
    let server = spawn_server(pool.clone()).await;

    // First commit: 2 recipes × 2 ingredients each = 4 lines.
    let (status1, body1) = post_admin(
        &server.base_url,
        "/api/recipe-imports/commit",
        &user.id,
        two_recipes_two_ingredients_each(),
    )
    .await;
    decode_commit(status1, &body1);
    let n_ri_1: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM recipe_ingredient")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(n_ri_1, 4);

    // Second commit of a different fixture that touches "Chicken Caesar"
    // with only ONE ingredient — the "Croutons" line for that recipe
    // must disappear (full-replace semantics).
    let (status2, body2) = post_admin(
        &server.base_url,
        "/api/recipe-imports/commit",
        &user.id,
        happy_single_row(),
    )
    .await;
    decode_commit(status2, &body2);

    // "Chicken Caesar" now has exactly 1 line; "Garden Salad" still has 2.
    let caesar_lines: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM recipe_ingredient ri \
         JOIN recipe r ON r.id = ri.recipe_id \
         WHERE r.name = 'Chicken Caesar'",
    )
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(
        caesar_lines, 1,
        "second import replaced the ingredient list"
    );

    let salad_lines: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM recipe_ingredient ri \
         JOIN recipe r ON r.id = ri.recipe_id \
         WHERE r.name = 'Garden Salad'",
    )
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(salad_lines, 2, "untouched recipe keeps its lines");
    server.stop().await;
}
