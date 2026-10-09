//! Excel export pipeline. Mirrors `src/lib/excel/export/*.ts` 1:1 —
//! same column headers, same column widths, same fill colours, same
//! sort tuples. Each generator is a single `pub async fn generate(pool,
//! from, to) -> Result<Vec<u8>, ExportError>` so the HTTP handler stays
//! trivial.
//!
//! Byte-identical determinism: each generator ends with a call to
//! [`crate::determinism::pin_xlsx_bytes`], which rewrites every ZIP
//! entry with the DOS epoch mtime and scrubs the "Unknown" strings
//! `rust_xlsxwriter` leaves in `docProps/core.xml`. Workbook-level
//! metadata (creator / created / modified) is zeroed out before save
//! so core.xml's remaining fields are already blank by the time the
//! scrub pass runs. Two runs on the same semantic data produce
//! byte-identical bytes; the `*_bytes_identical_across_runs` tests in
//! `crates/api/tests/exports.rs` guard the invariant.

pub mod production_order;
pub mod shopping_list;

/// Error surface for the two export generators. DB, workbook build, and
/// determinism post-processing failures funnel through one enum so the
/// HTTP handler can map all three to a single 500 response.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("xlsx build error: {0}")]
    Xlsx(#[from] rust_xlsxwriter::XlsxError),
    #[error("zip post-process error: {0}")]
    Zip(zip::result::ZipError),
    #[error("i/o error during zip post-process: {0}")]
    Io(std::io::Error),
    #[error("non-utf8 content in docProps/core.xml: {0}")]
    Utf8(std::string::FromUtf8Error),
}
