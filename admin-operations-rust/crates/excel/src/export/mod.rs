//! Excel export pipeline. Mirrors `src/lib/excel/export/*.ts` 1:1 —
//! same column headers, same column widths, same fill colours, same
//! sort tuples. Each generator is a single `pub async fn generate(pool,
//! from, to) -> Result<Vec<u8>, ExportError>` so the HTTP handler stays
//! trivial.
//!
//! Byte-identical determinism (ZIP epoch stamp + docProps/core.xml
//! scrub) lands in a later sub-session once the oracle rewrite is in;
//! the current ports target column/data parity only.

pub mod production_order;
pub mod shopping_list;

/// Error surface for the two export generators. DB and workbook build
/// failures funnel through one enum so the HTTP handler can map both
/// to a single 500 response.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("xlsx build error: {0}")]
    Xlsx(#[from] rust_xlsxwriter::XlsxError),
}
