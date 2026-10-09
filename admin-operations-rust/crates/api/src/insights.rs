//! `/api/insights/*` HTTP handlers.
//!
//! Thin layer: parse → authorise → delegate to `db::queries::insights`
//! → serialize. The two SQL rewrites (CTE+FILTER for KPI,
//! `SUM(SUM()) OVER ()` for top-recipes) live in the db crate; this
//! module only owns the HTTP contract.
//!
//! Scoping rule — insights is **admin-only across the board**. All 7
//! consumers (6 `/admin/insights/api/*` routes + the admin landing-page
//! KPI tile) are already admin-gated in SvelteKit; this layer enforces
//! it defensively via `require_admin` so a buggy route-threading
//! change can't accidentally hand client callers aggregate data about
//! other tenants.

use axum::{
    extract::{Query, State},
    Extension, Json,
};
use chrono::NaiveDate;
use db::queries::insights::{
    self, AttendanceByCompanyRow, AttendanceDayRateRow, CompanyBreakdownRow, IngredientDemandRow,
    KpiMetrics, SourceMixRow, TopRecipeRow, VolumeSeriesRow,
};
use domain::{DomainError, Role};
use serde::Deserialize;

use crate::{auth::AuthCtx, error::ApiError, state::AppState};

// ---------------------------------------------------------------
// Scope helper — insights is admin-only, reads only.
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

/// Minimal `?from=&to=` — matches six of the eight endpoints.
#[derive(Debug, Deserialize)]
pub struct RangeQuery {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

/// `GET /api/insights/kpi?from=&to=&prior_from=&prior_to=` — WoW
/// compare needs its own prior window (the caller controls what
/// "prior" means; we don't infer it from `from/to`).
#[derive(Debug, Deserialize)]
pub struct KpiQuery {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub prior_from: NaiveDate,
    pub prior_to: NaiveDate,
}

/// `GET /api/insights/volume?from=&to=&granularity=daily|weekly`.
/// Granularity defaults to `daily` to match the TS surface.
#[derive(Debug, Deserialize)]
pub struct VolumeQuery {
    pub from: NaiveDate,
    pub to: NaiveDate,
    #[serde(default = "default_granularity")]
    pub granularity: String,
}

fn default_granularity() -> String {
    "daily".to_string()
}

/// `GET /api/insights/top-recipes?from=&to=&limit=`. Limit defaults
/// to 10 to match the TS surface.
#[derive(Debug, Deserialize)]
pub struct TopRecipesQuery {
    pub from: NaiveDate,
    pub to: NaiveDate,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    10
}

// ---------------------------------------------------------------
// Handlers — admin only across the board
// ---------------------------------------------------------------

/// `GET /api/insights/kpi` — one CTE query, no post-SQL fan-out.
pub async fn kpi(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<KpiQuery>,
) -> Result<Json<KpiMetrics>, ApiError> {
    require_admin(&auth)?;
    let metrics =
        insights::get_kpi_metrics(&state.pool, q.from, q.to, q.prior_from, q.prior_to).await?;
    Ok(Json(metrics))
}

/// `GET /api/insights/volume` — daily or weekly series.
pub async fn volume(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<VolumeQuery>,
) -> Result<Json<Vec<VolumeSeriesRow>>, ApiError> {
    require_admin(&auth)?;
    let rows = insights::get_order_volume_series(&state.pool, q.from, q.to, &q.granularity).await?;
    Ok(Json(rows))
}

/// `GET /api/insights/top-recipes` — includes `percent_of_total` via
/// `SUM(SUM()) OVER ()`.
pub async fn top_recipes(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<TopRecipesQuery>,
) -> Result<Json<Vec<TopRecipeRow>>, ApiError> {
    require_admin(&auth)?;
    let rows = insights::get_top_recipes(&state.pool, q.from, q.to, q.limit).await?;
    Ok(Json(rows))
}

/// `GET /api/insights/companies` — per-company totals in window.
pub async fn companies(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<RangeQuery>,
) -> Result<Json<Vec<CompanyBreakdownRow>>, ApiError> {
    require_admin(&auth)?;
    let rows = insights::get_company_breakdown(&state.pool, q.from, q.to).await?;
    Ok(Json(rows))
}

/// `GET /api/insights/source-mix` — order_source rollup in window.
pub async fn source_mix(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<RangeQuery>,
) -> Result<Json<Vec<SourceMixRow>>, ApiError> {
    require_admin(&auth)?;
    let rows = insights::get_source_mix(&state.pool, q.from, q.to).await?;
    Ok(Json(rows))
}

/// `GET /api/insights/ingredients` — projected ingredient demand.
pub async fn ingredients(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<RangeQuery>,
) -> Result<Json<Vec<IngredientDemandRow>>, ApiError> {
    require_admin(&auth)?;
    let rows = insights::get_ingredient_demand(&state.pool, q.from, q.to).await?;
    Ok(Json(rows))
}

/// `GET /api/insights/attendance` — daily attendance rate.
pub async fn attendance(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<RangeQuery>,
) -> Result<Json<Vec<AttendanceDayRateRow>>, ApiError> {
    require_admin(&auth)?;
    let rows = insights::get_attendance_rate(&state.pool, q.from, q.to).await?;
    Ok(Json(rows))
}

/// `GET /api/insights/attendance-by-company` — company-rollup rate.
pub async fn attendance_by_company(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<RangeQuery>,
) -> Result<Json<Vec<AttendanceByCompanyRow>>, ApiError> {
    require_admin(&auth)?;
    let rows = insights::get_attendance_by_company(&state.pool, q.from, q.to).await?;
    Ok(Json(rows))
}
