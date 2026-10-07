//! `/api/exports/*` HTTP handlers.
//!
//! Three admin-only endpoints that delegate to the `excel` and `pdf`
//! export generators:
//!
//! - `GET /api/exports/production-order?from=&to=` → xlsx bytes
//! - `GET /api/exports/shopping-list?from=&to=`    → xlsx bytes
//! - `GET /api/exports/packing-slips?date=`        → JSON array of
//!   `{filename, company_name, pdf_base64}` entries, one per
//!   `(company, shipment_date)` group in the week containing `date`.
//!
//! Scoping — exports surface aggregate operational data across
//! companies, so client callers get a 403 unconditionally (same
//! pattern as `/api/insights/*`).
//!
//! Error mapping — both `ExportError` and `PdfError` are internal
//! build / DB failures; neither leaks structured validation state
//! worth translating. Both funnel through `ApiError::Internal` →
//! 500 with a short "export failed" body + tracing log.

use axum::{
    body::Body,
    extract::{Query, State},
    http::{header, HeaderValue, Response, StatusCode},
    Extension, Json,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::NaiveDate;
use domain::{DomainError, Role};
use excel::export::{production_order, shopping_list, ExportError};
use pdf::{packing_slip, PdfError};
use serde::{Deserialize, Serialize};

use crate::{auth::AuthCtx, error::ApiError, state::AppState};

// ---------------------------------------------------------------
// Scope helper — exports is admin-only.
// ---------------------------------------------------------------

fn require_admin(auth: &AuthCtx) -> Result<(), ApiError> {
    if auth.role == Role::Admin {
        Ok(())
    } else {
        Err(ApiError::Domain(DomainError::Forbidden))
    }
}

// ---------------------------------------------------------------
// Query shapes
// ---------------------------------------------------------------

/// Shared `from=&to=` window for production-order + shopping-list.
#[derive(Debug, Deserialize)]
pub struct ExportsRangeQuery {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

/// `GET /api/exports/packing-slips?date=YYYY-MM-DD`. Date is a
/// reference date; the generator expands it to the EST Mon-Sun week.
#[derive(Debug, Deserialize)]
pub struct PackingSlipsQuery {
    pub date: NaiveDate,
}

// ---------------------------------------------------------------
// Response shapes
// ---------------------------------------------------------------

/// One entry in the packing-slips JSON response. The TS route zips
/// these into a single download; keeping the API shape as an array of
/// pre-rendered PDFs means the route handler doesn't change.
#[derive(Debug, Serialize)]
pub struct PackingSlipJson {
    pub filename: String,
    pub company_name: String,
    pub pdf_base64: String,
}

// ---------------------------------------------------------------
// Error translation — the export generators have their own enums.
// Both get mapped to 500 with a generic message; details go to
// tracing. The oracle-rewrite sub-session will add determinism
// assertions on top of this layer, not change it.
// ---------------------------------------------------------------

fn map_export_err(e: ExportError) -> ApiError {
    tracing::error!(error = ?e, "excel export failed");
    ApiError::Internal(anyhow::anyhow!("export failed"))
}

fn map_pdf_err(e: PdfError) -> ApiError {
    tracing::error!(error = ?e, "pdf export failed");
    ApiError::Internal(anyhow::anyhow!("export failed"))
}

// ---------------------------------------------------------------
// Response builders for xlsx bytes
// ---------------------------------------------------------------

/// Wrap raw xlsx bytes in a response with the right Content-Type and
/// an attachment Content-Disposition. The TS side streams the bytes
/// to the browser verbatim; mirror that shape here.
///
/// Uses `Response::builder` so the dynamic filename lands in the
/// header as a plain owned `HeaderValue` — the `(headers, body)` tuple
/// `IntoResponse` sugar requires `&'static str` for header values and
/// would force a `Box::leak` to get a dynamic filename through.
///
/// `.expect` on `from_str` is justified: the filename is built from
/// `NaiveDate::to_string` plus a hard-coded prefix, so every byte is
/// ASCII; `HeaderValue::from_str` only fails on non-visible-ASCII.
fn xlsx_response(filename: &str, bytes: Vec<u8>) -> Response<Body> {
    let disposition = format!("attachment; filename=\"{filename}\"");
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        )
        .header(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_str(&disposition).expect("ascii disposition"),
        )
        .body(Body::from(bytes))
        .expect("build xlsx response")
}

// ---------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------

/// `GET /api/exports/production-order?from=&to=`.
pub async fn production_order(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<ExportsRangeQuery>,
) -> Result<Response<Body>, ApiError> {
    require_admin(&auth)?;
    let bytes = production_order::generate(&state.pool, q.from, q.to)
        .await
        .map_err(map_export_err)?;
    let filename = format!("production-order-{}-to-{}.xlsx", q.from, q.to);
    Ok(xlsx_response(&filename, bytes))
}

/// `GET /api/exports/shopping-list?from=&to=`.
pub async fn shopping_list(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<ExportsRangeQuery>,
) -> Result<Response<Body>, ApiError> {
    require_admin(&auth)?;
    let bytes = shopping_list::generate(&state.pool, q.from, q.to)
        .await
        .map_err(map_export_err)?;
    let filename = format!("shopping-list-{}-to-{}.xlsx", q.from, q.to);
    Ok(xlsx_response(&filename, bytes))
}

/// `GET /api/exports/packing-slips?date=`.
pub async fn packing_slips(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<PackingSlipsQuery>,
) -> Result<(StatusCode, Json<Vec<PackingSlipJson>>), ApiError> {
    require_admin(&auth)?;
    let slips = packing_slip::generate(&state.pool, q.date)
        .await
        .map_err(map_pdf_err)?;
    let out: Vec<PackingSlipJson> = slips
        .into_iter()
        .map(|s| PackingSlipJson {
            filename: s.filename,
            company_name: s.company_name,
            pdf_base64: STANDARD.encode(&s.pdf_bytes),
        })
        .collect();
    Ok((StatusCode::OK, Json(out)))
}
