//! Excel import pipeline. Mirrors `src/lib/excel/parse.ts` +
//! `src/lib/server/queries/imports.ts` 1:1 — same warning taxonomy,
//! same error messages, same row-numbering (header row is row 1,
//! first data row is row 2). The real-client integration fixture is
//! the parity oracle; see `crates/api/tests/import_real_client.rs`.
//!
//! Public surface:
//! - [`import::plan_import`] — the orchestrator. Parses the workbook,
//!   runs the full pipeline, returns an `ImportPlan` ready for either
//!   preview (discard) or commit (insert).
//! - [`import::ImportPlan`] / [`import::ImportError`] — plan output
//!   and parse/DB error surface.
//!
//! Private-but-tested submodules keep the pipeline composable; the
//! unit tests in each module cover the pure branches, and the real-
//! client fixture test covers the integration path end-to-end.

pub mod boilerplate;
pub mod determinism;
pub mod export;
pub mod fuzzy;
pub mod headers;
pub mod import;
pub mod meals;
pub mod normalise;
pub mod parse;
pub mod recipes;
pub mod validate;
