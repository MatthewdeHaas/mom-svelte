//! `/api/import/preview` and `/api/import/commit`.
//!
//! Both endpoints share `plan_import` from the `excel` crate — the only
//! difference is what happens after planning: preview discards, commit
//! inserts in a transaction. That split is what makes the parity test
//! meaningful: if preview and commit disagree about `warnings`, the
//! bug is in the test setup, not the pipeline.
//!
//! Multipart contract mirrors `src/routes/admin/orders/import/+page.server.ts`:
//! - field name: `file`
//! - must have `.xlsx` extension (we trust the extension, not the
//!   content-type — a few clients send `application/octet-stream`)
//! - 5 MiB cap (the real-client weekly workbook is ~70 KiB; headroom
//!   for growth without opening a DoS vector)

use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    Extension, Json,
};
use db::queries::orders;
use domain::{ImportPreview, ImportResult};
use excel::import::{plan_import, ImportError};

use crate::{auth::AuthCtx, error::ApiError, state::AppState};

/// 5 MiB. Matches the TS route cap. Larger uploads 413 before we parse.
const MAX_UPLOAD_BYTES: usize = 5 * 1024 * 1024;

/// `POST /api/import/preview` — plan without inserting.
pub async fn preview(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    multipart: Multipart,
) -> Result<Json<ImportPreview>, ApiError> {
    let bytes = read_xlsx_upload(multipart).await?;
    let plan = run_plan(&bytes, &auth.user_id, &state).await?;

    Ok(Json(ImportPreview {
        would_insert: plan.orders.len() as i64,
        skipped: 0,
        errors: plan.errors,
        warnings: plan.warnings,
    }))
}

/// `POST /api/import/commit` — plan and (if no errors) insert in a tx.
pub async fn commit(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    multipart: Multipart,
) -> Result<Json<ImportResult>, ApiError> {
    let bytes = read_xlsx_upload(multipart).await?;
    let plan = run_plan(&bytes, &auth.user_id, &state).await?;

    // Mirror TS: errors → return 200 with inserted=0 so the SvelteKit
    // route can render the yellow strip without special-casing status.
    if !plan.errors.is_empty() {
        return Ok(Json(ImportResult {
            inserted: 0,
            skipped: 0,
            errors: plan.errors,
            warnings: plan.warnings,
        }));
    }

    let inserted = if plan.orders.is_empty() {
        0
    } else {
        let mut tx = state.pool.begin().await?;
        let n = orders::insert_batch(&mut tx, &plan.orders).await?;
        tx.commit().await?;
        n
    };

    Ok(Json(ImportResult {
        inserted,
        skipped: 0,
        errors: Vec::new(),
        warnings: plan.warnings,
    }))
}

/// Drain the `file` part off the multipart body with a hard 5 MiB cap.
/// Returns a validation `ApiError` on any shape problem — the handler
/// shouldn't have to care which specific failure mode tripped.
async fn read_xlsx_upload(mut multipart: Multipart) -> Result<Vec<u8>, ApiError> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| validation(format!("multipart read failed: {e}")))?
    {
        if field.name() != Some("file") {
            continue;
        }

        let file_name = field.file_name().map(str::to_owned);
        let bytes = field
            .bytes()
            .await
            .map_err(|e| validation(format!("file read failed: {e}")))?;

        if bytes.is_empty() {
            return Err(validation("Uploaded file is empty".to_string()));
        }
        if bytes.len() > MAX_UPLOAD_BYTES {
            return Err(ApiError::Domain(domain::DomainError::Validation(format!(
                "File exceeds {} MiB limit",
                MAX_UPLOAD_BYTES / (1024 * 1024)
            ))));
        }

        let name = file_name.as_deref().unwrap_or("");
        if !name.to_ascii_lowercase().ends_with(".xlsx") {
            return Err(validation("Only .xlsx files are supported".to_string()));
        }

        return Ok(bytes.to_vec());
    }

    Err(validation(
        "Missing `file` field in multipart body".to_string(),
    ))
}

/// Call `plan_import`, mapping its error surface onto the API error
/// shape. Parse failures are user-visible validation errors; DB
/// failures bubble up as 500s via the existing `From<sqlx::Error>`
/// mapping on `ApiError`.
async fn run_plan(
    bytes: &[u8],
    user_id: &str,
    state: &AppState,
) -> Result<excel::import::ImportPlan, ApiError> {
    plan_import(bytes, user_id, &state.pool)
        .await
        .map_err(|e| match e {
            ImportError::Parse(p) => validation(format!("Could not parse workbook: {p}")),
            ImportError::Db(db) => ApiError::Db(db),
        })
}

fn validation(msg: String) -> ApiError {
    ApiError::Domain(domain::DomainError::Validation(msg))
}

// Multipart error → 400 (not the default 500) when the body is simply
// malformed. Only hit when a client sends a non-multipart POST; axum
// otherwise rejects before we're invoked.
#[allow(dead_code)]
pub(crate) fn multipart_rejection_status() -> StatusCode {
    StatusCode::BAD_REQUEST
}
