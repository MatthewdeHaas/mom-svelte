//! Shared pagination for list queries. TS counterpart is
//! `src/lib/server/queries/pagination.ts`; keep the clamping rules
//! identical so both paths produce the same `totalPages` math.
//!
//! The TS side exposes `paginationSQL` which stitches `LIMIT $N
//! OFFSET $N+1` onto a hand-built query string. We don't need that
//! here — sqlx `QueryBuilder` lets each query call `.push_bind(limit)`
//! / `.push_bind(offset)` directly. So this module only ships the
//! types + clamp helpers + result builder.

use serde::{Deserialize, Serialize};

/// Mirrors TS `PaginationParams`. `page` is 1-based; `per_page` is
/// bounded [10, 100] by `parse_from_query`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_per_page", rename = "per_page")]
    pub per_page: i64,
}

fn default_page() -> i64 {
    1
}
fn default_per_page() -> i64 {
    50
}

impl Default for PaginationParams {
    fn default() -> Self {
        Self {
            page: 1,
            per_page: 50,
        }
    }
}

/// Wire shape returned by `*_paginated` queries. Field names match the
/// TS `PaginatedResult<T>` camelCase so the SvelteKit JSON decoder
/// doesn't need a translation layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Paginated<T> {
    pub data: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
    pub total_pages: i64,
}

/// Clamp page >= 1 and per_page ∈ [10, 100]. Exactly mirrors the TS
/// `parsePaginationParams` logic. `default_per_page` lets callers
/// override the 50-row default per endpoint.
pub fn parse_from_query(
    page: Option<i64>,
    per_page: Option<i64>,
    default_per_page: i64,
) -> PaginationParams {
    let page = page.unwrap_or(1).max(1);
    let per_page = per_page
        .unwrap_or(default_per_page)
        .clamp(10, 100);
    PaginationParams { page, per_page }
}

/// 0-based offset for a `LIMIT`/`OFFSET` clause.
pub fn offset(p: &PaginationParams) -> i64 {
    (p.page - 1) * p.per_page
}

/// Build the response. Clamps requested `page` down to `total_pages`
/// when the caller asked past the end — same policy as the TS helper.
pub fn build<T>(data: Vec<T>, total: i64, p: PaginationParams) -> Paginated<T> {
    let total_pages = ((total as f64) / (p.per_page as f64)).ceil().max(1.0) as i64;
    let page = p.page.min(total_pages);
    Paginated {
        data,
        total,
        page,
        per_page: p.per_page,
        total_pages,
    }
}
