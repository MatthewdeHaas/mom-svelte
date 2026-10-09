//! `/api/exports/*` HTTP handlers.
//!
//! Three admin-only endpoints that delegate to the `excel` and `pdf`
//! export generators:
//!
//! - `GET /api/exports/production-order?from=&to=` → xlsx bytes
//! - `GET /api/exports/shopping-list?from=&to=`    → xlsx bytes
//! - `GET /api/exports/packing-slips?date=`        → raw bytes: a single
//!   PDF when the week contains exactly one `(company, shipment_date)`
//!   group, a ZIP archive (`<slug>/<filename>` entries) when it
//!   contains more than one. 404 when it contains zero.
//!
//! All three return raw bytes with the right `Content-Type` and an
//! attachment `Content-Disposition`, so the SvelteKit routes are pure
//! byte pumps — no base64 decode, no in-TS ZIP assembly, no business
//! logic in the HTTP layer.
//!
//! Scoping — exports surface aggregate operational data across
//! companies, so client callers get a 403 unconditionally (same
//! pattern as `/api/insights/*`).
//!
//! Error mapping — both `ExportError` and `PdfError` are internal
//! build / DB failures; neither leaks structured validation state
//! worth translating. Both funnel through `ApiError::Internal` →
//! 500 with a short "export failed" body + tracing log.

use std::io::{Cursor, Write};

use axum::{
    body::Body,
    extract::{Query, State},
    http::{header, HeaderValue, Response, StatusCode},
    Extension,
};
use chrono::NaiveDate;
use domain::{DomainError, Role};
use excel::export::{production_order, shopping_list, ExportError};
use pdf::{packing_slip, week_bounds_est, PackingSlipPdf, PdfError};
use serde::Deserialize;
use zip::{write::FileOptions, CompressionMethod, DateTime, ZipWriter};

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
// Error translation — the export generators have their own enums.
// Both get mapped to 500 with a generic message; details go to
// tracing.
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
// Response builders
// ---------------------------------------------------------------

/// Build an `attachment; filename="<name>"` disposition header. Only
/// safe for ASCII filenames — asserts via `.expect` which is justified
/// because every filename produced here is built from `NaiveDate` or a
/// slugified company name, both of which are ASCII-only. See
/// `slugify_company` for the folder-name path.
fn attachment_disposition(filename: &str) -> HeaderValue {
    let disposition = format!("attachment; filename=\"{filename}\"");
    HeaderValue::from_str(&disposition).expect("ascii disposition")
}

/// Wrap raw bytes in a response with the given `Content-Type` and
/// attachment `Content-Disposition`. Mirrors the TS `excelResponse` /
/// `pdfResponse` / inline ZIP helpers so the SvelteKit route becomes
/// a stream-through.
///
/// Uses `Response::builder` so the dynamic filename lands in the
/// header as a plain owned `HeaderValue` — the `(headers, body)` tuple
/// `IntoResponse` sugar requires `&'static str` for header values and
/// would force a `Box::leak` to get a dynamic filename through.
fn bytes_response(content_type: &'static str, filename: &str, bytes: Vec<u8>) -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(
            header::CONTENT_DISPOSITION,
            attachment_disposition(filename),
        )
        .body(Body::from(bytes))
        .expect("build byte response")
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
    Ok(bytes_response(
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        &filename,
        bytes,
    ))
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
    Ok(bytes_response(
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        &filename,
        bytes,
    ))
}

/// `GET /api/exports/packing-slips?date=`.
///
/// Three cases, decided by the number of `(company, shipment_date)`
/// groups in the EST week containing `date`:
///
/// - `0` → `404 Not Found` with body `"No orders found for that week."`
///   (TS route emitted this today; moved into Rust so the SvelteKit
///   route doesn't need to inspect the response shape to decide).
/// - `1` → raw PDF bytes, `Content-Type: application/pdf`,
///   `Content-Disposition: attachment; filename="<slip filename>"`.
/// - `>1` → ZIP archive, `Content-Type: application/zip`,
///   `Content-Disposition: attachment; filename="packing-slips-week-of-<mon>.zip"`
///   where `<mon>` is the Monday of the EST week. Entry paths are
///   `<slug(company_name)>/<slip filename>` matching the TS layout.
pub async fn packing_slips(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<PackingSlipsQuery>,
) -> Result<Response<Body>, ApiError> {
    require_admin(&auth)?;
    let mut slips = packing_slip::generate(&state.pool, q.date)
        .await
        .map_err(map_pdf_err)?;

    if slips.is_empty() {
        return Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(Body::from("No orders found for that week."))
            .expect("build 404 response"));
    }

    if slips.len() == 1 {
        let slip = slips.remove(0);
        return Ok(bytes_response(
            "application/pdf",
            &slip.filename,
            slip.pdf_bytes,
        ));
    }

    let (mon, _) = week_bounds_est(q.date);
    let zip_filename = format!("packing-slips-week-of-{mon}.zip");
    let zip_bytes = build_packing_slips_zip(&slips).map_err(|e| {
        tracing::error!(error = ?e, "packing-slip zip assembly failed");
        ApiError::Internal(anyhow::anyhow!("export failed"))
    })?;
    Ok(bytes_response("application/zip", &zip_filename, zip_bytes))
}

/// Assemble a byte-identical ZIP of packing slips. Mirrors the TS
/// `scripts/verify-determinism.ts` + route-side JSZip assembly:
///
/// - Entry path: `<slug(company_name)>/<slip.filename>`.
/// - Compression: `Deflated`, level 6 (TS `JSZip.generateAsync` default
///   when `compressionOptions: { level: 6 }` is passed).
/// - Entry mtime: DOS epoch (1980-01-01 00:00:00). Without this,
///   `zip::DateTime::default()` would stamp wall-clock time and the
///   `*_bytes_identical_across_runs` test flaps.
fn build_packing_slips_zip(slips: &[PackingSlipPdf]) -> Result<Vec<u8>, zip::result::ZipError> {
    let mut buf: Vec<u8> = Vec::with_capacity(slips.iter().map(|s| s.pdf_bytes.len() + 256).sum());
    {
        let cursor = Cursor::new(&mut buf);
        let mut writer = ZipWriter::new(cursor);
        let options: FileOptions = FileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(6))
            .last_modified_time(DateTime::default());

        for slip in slips {
            let folder = slugify_company(&slip.company_name);
            let entry = if folder.is_empty() {
                slip.filename.clone()
            } else {
                format!("{folder}/{}", slip.filename)
            };
            writer.start_file(&entry, options)?;
            writer
                .write_all(&slip.pdf_bytes)
                .map_err(zip::result::ZipError::Io)?;
        }

        writer.finish()?;
    }
    Ok(buf)
}

/// Lowercase, non-alphanumeric → `-`, trim leading/trailing dashes.
/// Mirrors the TS `slugify` in the deleted
/// `src/routes/admin/exports/packing-slips/+server.ts` 1:1.
fn slugify_company(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_was_dash = true; // suppress leading dashes
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.extend(c.to_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            out.push('-');
            last_was_dash = true;
        }
    }
    // Trim trailing dash.
    if out.ends_with('-') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_matches_ts_contract() {
        assert_eq!(slugify_company("Acme Foods"), "acme-foods");
        assert_eq!(slugify_company("Hello, World!"), "hello-world");
        assert_eq!(slugify_company("  spaced  "), "spaced");
        assert_eq!(slugify_company("---edge---"), "edge");
        assert_eq!(slugify_company("MixedCASE 123"), "mixedcase-123");
        assert_eq!(slugify_company("!!!"), "");
    }
}
