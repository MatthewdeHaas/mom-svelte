//! `/api/recipe-imports/preview` and `/api/recipe-imports/commit`.
//!
//! Admin-only. Both endpoints accept the same multipart (`file=<xlsx>`,
//! 5 MiB cap) and share the `parse → validate` prelude; they diverge
//! after that:
//!
//! - **preview**: parse → if parse errors → 422 → if 0 rows → 400 →
//!   compute fuzzy warnings against existing ingredient names → 200
//!   with `RecipeImportPreview`.
//! - **commit**: parse → if parse errors → 422 → if 0 rows → 400 →
//!   `import_recipes_from_rows` inside a tx → 200 with `RecipeImportResult`.
//!
//! The TS route (`src/routes/admin/recipes/new/+page.server.ts`) decides
//! which endpoint to hit based on `(mode === 'add' && !confirmed)`:
//! that's the "show the user fuzzy warnings before touching the DB"
//! branch. In replace-mode or on a confirmed re-submit it goes straight
//! to commit. The dispatcher-less route forwards multipart to one of
//! these two endpoints based on the same flag.
//!
//! Error-code parity with the TS route:
//! - unparseable workbook → 400 (ApiError::Validation)
//! - row-level validation failures → 422 with `parse_errors` payload
//! - empty row set → 400
//! - all other failures (DB, auth) → the standard 500/401 surface

use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use db::queries::{ingredients, recipe_imports};
use domain::{
    recipes::{RecipeImportPreview, RecipeImportResult},
    DomainError, ImportRowError, Role,
};
use excel::{
    fuzzy::{find_fuzzy_warnings, DEFAULT_MIN_LENGTH, DEFAULT_THRESHOLD},
    recipes::{parse_recipe_xlsx, ParseRecipeError},
};
use serde::Serialize;
use std::collections::HashSet;

use crate::{auth::AuthCtx, error::ApiError, state::AppState};

/// 5 MiB. Matches the TS route cap. Larger uploads 413 before we parse.
const MAX_UPLOAD_BYTES: usize = 5 * 1024 * 1024;

// ---------------------------------------------------------------
// Scope helper — recipe imports are admin-only.
// ---------------------------------------------------------------

fn require_admin(auth: &AuthCtx) -> Result<(), ApiError> {
    if auth.role == Role::Admin {
        Ok(())
    } else {
        Err(ApiError::Domain(DomainError::Forbidden))
    }
}

// ---------------------------------------------------------------
// 422 wire shape — mirrors the TS `fail(422, { parseErrors, ... })`
// branch so the dispatcher-side forwarder can bounce the body through.
// ---------------------------------------------------------------

/// Response body returned with HTTP 422 when the uploaded workbook had
/// row-level validation errors. The TS route surfaces `parseErrors`
/// (camelCase on the SvelteKit side); we keep snake_case here and let
/// the forwarder translate — same discipline as `ImportRowError` fields.
#[derive(Debug, Serialize)]
struct ParseErrorsBody {
    parse_errors: Vec<ImportRowError>,
}

/// Local error enum so the two handlers can share the parse+validate
/// prelude without each duplicating the match arms. Converts into an
/// `axum::Response` directly — bypasses `ApiError` because the 422
/// case needs a structured body, not the plain `{error, message}`
/// shape `ApiError` emits.
enum PrepareError {
    /// Hard failure before any row was parsed: bad xlsx, missing
    /// sheet, missing required columns. Maps to 400 + a plain-text
    /// message string via `ApiError::Validation`.
    Parse(String),
    /// Row-level validation errors. Maps to 422 with a structured body.
    RowErrors(Vec<ImportRowError>),
    /// Parse succeeded but produced zero data rows. 400 with a plain
    /// message, matching TS `fail(400, { importError: 'contains no data rows' })`.
    Empty,
    /// Multipart / upload-shape problem. 400 with a message.
    Upload(String),
}

impl IntoResponse for PrepareError {
    fn into_response(self) -> Response {
        match self {
            PrepareError::Parse(msg) => {
                ApiError::Domain(DomainError::Validation(msg)).into_response()
            }
            PrepareError::Upload(msg) => {
                ApiError::Domain(DomainError::Validation(msg)).into_response()
            }
            PrepareError::Empty => ApiError::Domain(DomainError::Validation(
                "The spreadsheet contains no data rows.".to_string(),
            ))
            .into_response(),
            PrepareError::RowErrors(errs) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ParseErrorsBody {
                    parse_errors: errs,
                }),
            )
                .into_response(),
        }
    }
}

/// Shared prelude: pull the file off the multipart body, parse it,
/// surface the three "no rows for you" branches. On success returns
/// the parsed row vec, ready for either preview or commit.
async fn prepare(multipart: Multipart) -> Result<Vec<domain::recipes::RecipeExcelRowParsed>, PrepareError> {
    let bytes = read_xlsx_upload(multipart).await?;
    let parsed = parse_recipe_xlsx(&bytes).map_err(|e| match e {
        // Hard parse failure → surface the user-visible message. TS
        // produces "Excel template missing columns: ..." verbatim; our
        // `ParseRecipeError::Display` matches that string.
        ParseRecipeError::MissingColumns(_)
        | ParseRecipeError::NoSheet
        | ParseRecipeError::Workbook(_) => PrepareError::Parse(e.to_string()),
    })?;

    if !parsed.errors.is_empty() {
        return Err(PrepareError::RowErrors(parsed.errors));
    }
    if parsed.rows.is_empty() {
        return Err(PrepareError::Empty);
    }
    Ok(parsed.rows)
}

// ---------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------

/// `POST /api/recipe-imports/preview` — parse + fuzzy-match, no writes.
pub async fn preview(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    multipart: Multipart,
) -> Result<Json<RecipeImportPreview>, Response> {
    require_admin(&auth).map_err(IntoResponse::into_response)?;
    let rows = prepare(multipart).await.map_err(IntoResponse::into_response)?;

    // Collect the input ingredient names (filter_map strips rows with no
    // ingredient and dedupes first-seen; TS does the same via
    // `[...new Set(rows.map(r => r.ingredient).filter(Boolean))]`).
    let mut seen: HashSet<String> = HashSet::new();
    let mut input_names: Vec<String> = Vec::new();
    for r in &rows {
        if let Some(name) = r.ingredient.as_deref() {
            if seen.insert(name.to_string()) {
                input_names.push(name.to_string());
            }
        }
    }

    // Existing names from the DB — ordered, deduped by `list_names`.
    let existing = ingredients::list_names(&state.pool)
        .await
        .map_err(|e| ApiError::Db(e).into_response())?;

    let warnings = find_fuzzy_warnings(
        &input_names,
        &existing,
        DEFAULT_THRESHOLD,
        DEFAULT_MIN_LENGTH,
    );

    Ok(Json(RecipeImportPreview {
        rows_parsed: rows.len() as i64,
        parse_errors: Vec::new(),
        fuzzy_warnings: warnings,
    }))
}

/// `POST /api/recipe-imports/commit` — parse + import in a transaction.
pub async fn commit(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    multipart: Multipart,
) -> Result<Json<RecipeImportResult>, Response> {
    require_admin(&auth).map_err(IntoResponse::into_response)?;
    let rows = prepare(multipart).await.map_err(IntoResponse::into_response)?;

    let created_by = auth.user_id.as_str();
    let result = recipe_imports::import_recipes_from_rows(&state.pool, &rows, created_by)
        .await
        .map_err(|e| ApiError::Db(e).into_response())?;

    Ok(Json(result))
}

// ---------------------------------------------------------------
// Multipart drain — same shape as `import.rs::read_xlsx_upload`,
// returning our local PrepareError for consistent error mapping.
// ---------------------------------------------------------------

async fn read_xlsx_upload(mut multipart: Multipart) -> Result<Vec<u8>, PrepareError> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| PrepareError::Upload(format!("multipart read failed: {e}")))?
    {
        if field.name() != Some("file") {
            continue;
        }

        let file_name = field.file_name().map(str::to_owned);
        let bytes = field
            .bytes()
            .await
            .map_err(|e| PrepareError::Upload(format!("file read failed: {e}")))?;

        if bytes.is_empty() {
            return Err(PrepareError::Upload("Uploaded file is empty".to_string()));
        }
        if bytes.len() > MAX_UPLOAD_BYTES {
            return Err(PrepareError::Upload(format!(
                "File exceeds {} MiB limit",
                MAX_UPLOAD_BYTES / (1024 * 1024)
            )));
        }

        let name = file_name.as_deref().unwrap_or("");
        if !name.to_ascii_lowercase().ends_with(".xlsx") {
            return Err(PrepareError::Upload(
                "Only .xlsx files are supported".to_string(),
            ));
        }

        return Ok(bytes.to_vec());
    }

    Err(PrepareError::Upload(
        "Missing `file` field in multipart body".to_string(),
    ))
}
