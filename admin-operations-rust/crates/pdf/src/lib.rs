//! PDF generation (packing slips).
//!
//! Port of `src/lib/pdf/packing-slip.ts` → [`packing_slip::generate`].
//! The TS byte-identical contract is tested from the SvelteKit side today
//! (`scripts/verify-determinism.ts`); the sub-session after this one
//! rewrites the oracle to semantic extraction and reasserts the contract
//! against this generator.

pub mod determinism;
pub mod packing_slip;

pub use packing_slip::{week_bounds_est, PackingSlipPdf};

/// Error surface for packing-slip generation. DB and printpdf build
/// failures funnel through one enum so the HTTP handler can map both
/// to a single 500 response.
///
/// `printpdf::Error` doesn't implement `std::error::Error` in 0.7, so
/// stringify at the boundary rather than wrap it as `#[from]`.
#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("pdf build error: {0}")]
    Build(String),
}
