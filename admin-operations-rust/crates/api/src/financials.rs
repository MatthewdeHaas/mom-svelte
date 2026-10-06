//! `/api/financials/*` HTTP handlers.
//!
//! Thin layer: parse → authorise → delegate to `db::queries::financials`
//! → serialize. Business logic (margin arithmetic, invoice aggregation)
//! lives in the db crate — this module owns the HTTP contract and the
//! admin-only scoping.
//!
//! Scoping rule — financials are admin-only across the board (same as
//! invites / allergens). The one consumer
//! (`src/routes/admin/financials/+page.server.ts`) is admin-gated at
//! the route level; this handler enforces it defensively.
//!
//! `get_invoice_data` returns 404 when the company id doesn't exist,
//! which the dispatcher translates back to the TS `null` return value.

use axum::{
    extract::{Query, State},
    Extension, Json,
};
use chrono::NaiveDate;
use db::queries::{
    financials::{self, InvoiceSummary, RecipeMarginRow},
    pagination::{parse_from_query, Paginated},
};
use domain::{DomainError, Role};
use serde::Deserialize;
use uuid::Uuid;

use crate::{auth::AuthCtx, error::ApiError, state::AppState};

// ---------------------------------------------------------------
// Scope helper — financials are admin-only, reads only.
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

/// `GET /api/financials/recipe-margins/paginated?page=&per_page=`.
#[derive(Debug, Deserialize)]
pub struct PaginatedQuery {
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default, rename = "per_page")]
    pub per_page: Option<i64>,
}

/// `GET /api/financials/invoice?company_id=&from=&to=`. Dates arrive as
/// `YYYY-MM-DD` strings; `NaiveDate` parses that format natively via
/// serde — matches the dispatcher's `filtersToQueryString` ISO-date
/// formatter.
#[derive(Debug, Deserialize)]
pub struct InvoiceQuery {
    pub company_id: Uuid,
    pub from: NaiveDate,
    pub to: NaiveDate,
}

// ---------------------------------------------------------------
// Handlers — admin only
// ---------------------------------------------------------------

/// `GET /api/financials/recipe-margins` — unpaginated flat list.
pub async fn recipe_margins(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
) -> Result<Json<Vec<RecipeMarginRow>>, ApiError> {
    require_admin(&auth)?;
    let rows = financials::get_recipes_with_margins(&state.pool).await?;
    Ok(Json(rows))
}

/// `GET /api/financials/recipe-margins/paginated` — paginated list,
/// 25-row default to match the TS route (`+page.server.ts:9`).
pub async fn recipe_margins_paginated(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<PaginatedQuery>,
) -> Result<Json<Paginated<RecipeMarginRow>>, ApiError> {
    require_admin(&auth)?;
    let pagination = parse_from_query(q.page, q.per_page, 25);
    let page = financials::get_recipes_with_margins_paginated(&state.pool, pagination).await?;
    Ok(Json(page))
}

/// `GET /api/financials/invoice?company_id=&from=&to=` — 404 when the
/// company id doesn't exist (dispatcher maps to `null`). An existing
/// company with no matching orders returns 200 with an empty line_items
/// array — matches the TS SQL path's shape at `financials.ts:176-182`.
pub async fn invoice(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<InvoiceQuery>,
) -> Result<Json<InvoiceSummary>, ApiError> {
    require_admin(&auth)?;
    let summary = financials::get_invoice_data(&state.pool, q.company_id, q.from, q.to)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(summary))
}
