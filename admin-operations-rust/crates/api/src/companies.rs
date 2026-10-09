//! `/api/companies/*` HTTP handlers.
//!
//! Thin layer: parse → authorise → delegate to `db::queries::companies`
//! → serialize. Business logic (COALESCE updates, case-insensitive
//! name lookup, pagination) lives in the db crate — this module owns
//! the HTTP contract and the admin-vs-client scoping.
//!
//! Scoping rules — companies are global master data, but unlike
//! recipes clients may NOT enumerate the whole set (would leak the
//! customer list to any client with a stolen token):
//! - `role == Admin` → unrestricted.
//! - `role == Client` → **can only read their own company**. Every
//!   mutation returns 403; list / by-name / users-by-company return
//!   403; by-id returns 404 for anything that isn't the caller's own
//!   company (same "lie about existence" rule as orders — a client
//!   probing with a stolen id learns nothing).
//!
//! Error mapping:
//! - UNIQUE-name violation (23505) → 409 Conflict.
//! - FK violation (23503) / CHECK violation (23514) → 400 Validation.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use db::queries::{
    companies::{
        self, CompanyRow, InsertCompanyData, ListOpts, UpdateCompanyData, UserWithCompanyRow,
    },
    pagination::{parse_from_query, Paginated},
};
use domain::{DomainError, Role};
use serde::Deserialize;
use uuid::Uuid;

use crate::{auth::AuthCtx, error::ApiError, state::AppState};

// ---------------------------------------------------------------
// Scope helpers.
//
// Companies have a stricter scoping model than recipes or orders:
// clients can see their own row only. Every list/by-name endpoint
// is admin-only; by-id gates on `auth.company_id`.
// ---------------------------------------------------------------

/// Reject anything but admin with 403. Used by list / by-name /
/// users-by-company and every mutation.
fn require_admin(auth: &AuthCtx) -> Result<(), ApiError> {
    if auth.role == Role::Admin {
        Ok(())
    } else {
        Err(ApiError::Domain(DomainError::Forbidden))
    }
}

/// For `GET /api/companies/:id`: admin passes through, client is
/// restricted to its own `company_id`. Returning `NotFound` (not
/// `Forbidden`) on mismatch matches the orders handler and prevents
/// a stolen-id probe from distinguishing "exists but not yours" from
/// "doesn't exist".
fn can_read_company(auth: &AuthCtx, id: Uuid) -> Result<(), ApiError> {
    if auth.role == Role::Admin {
        return Ok(());
    }
    let cid = auth
        .company_id
        .ok_or(ApiError::Domain(DomainError::Forbidden))?;
    if cid == id {
        Ok(())
    } else {
        Err(ApiError::Domain(DomainError::NotFound))
    }
}

// ---------------------------------------------------------------
// Query shapes
// ---------------------------------------------------------------

/// `GET /api/companies`: `include_inactive` only.
#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    #[serde(default, deserialize_with = "crate::query::bool_from_str")]
    pub include_inactive: bool,
}

impl From<ListQuery> for ListOpts {
    fn from(q: ListQuery) -> Self {
        Self {
            include_inactive: q.include_inactive,
            search: None,
        }
    }
}

/// `GET /api/companies/paginated` — adds `search` + pagination.
#[derive(Debug, Deserialize)]
pub struct ListPaginatedQuery {
    #[serde(default, deserialize_with = "crate::query::bool_from_str")]
    pub include_inactive: bool,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default, rename = "per_page")]
    pub per_page: Option<i64>,
}

/// `GET /api/companies/by-name?name=...`.
#[derive(Debug, Deserialize)]
pub struct GetByNameQuery {
    pub name: String,
}

// ---------------------------------------------------------------
// Handlers — reads
// ---------------------------------------------------------------

/// `GET /api/companies` — admin-only flat list.
pub async fn list(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<CompanyRow>>, ApiError> {
    require_admin(&auth)?;
    let opts: ListOpts = q.into();
    let rows = companies::list(&state.pool, &opts).await?;
    Ok(Json(rows))
}

/// `GET /api/companies/paginated` — admin-only paginated list with
/// optional case-insensitive name search.
pub async fn list_paginated(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<ListPaginatedQuery>,
) -> Result<Json<Paginated<CompanyRow>>, ApiError> {
    require_admin(&auth)?;
    let opts = ListOpts {
        include_inactive: q.include_inactive,
        search: q.search,
    };
    let pagination = parse_from_query(q.page, q.per_page, 50);
    let page = companies::list_paginated(&state.pool, &opts, pagination).await?;
    Ok(Json(page))
}

/// `GET /api/companies/:id` — detail. Admin: any row (including
/// inactive — matches TS `getCompanyById` semantics). Client: own
/// company only, with 404 on mismatch.
pub async fn get_by_id(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
) -> Result<Json<CompanyRow>, ApiError> {
    can_read_company(&auth, id)?;
    let row = companies::get_by_id(&state.pool, id)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `GET /api/companies/by-name?name=...` — admin-only case-insensitive
/// exact-name lookup. 404 on no match.
pub async fn get_by_name(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<GetByNameQuery>,
) -> Result<Json<CompanyRow>, ApiError> {
    require_admin(&auth)?;
    let row = companies::get_by_name_full(&state.pool, &q.name)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `GET /api/companies/:id/users` — list users belonging to a company,
/// company embedded. Admin: any company. Client: own company only
/// (404 for others — same "lie about existence" rule).
pub async fn get_users_by_company(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<UserWithCompanyRow>>, ApiError> {
    can_read_company(&auth, id)?;
    let rows = companies::get_users_by_company(&state.pool, id).await?;
    Ok(Json(rows))
}

// ---------------------------------------------------------------
// Handlers — writes (admin only)
// ---------------------------------------------------------------

/// `POST /api/companies` — create. 201 on success; 409 on duplicate name.
pub async fn create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Json(body): Json<InsertCompanyData>,
) -> Result<(StatusCode, Json<CompanyRow>), ApiError> {
    require_admin(&auth)?;
    let row = companies::create(&state.pool, body).await.map_err(map_pg)?;
    Ok((StatusCode::CREATED, Json(row)))
}

/// `PATCH /api/companies/:id` — partial update. 404 on no match,
/// 409 on duplicate name.
pub async fn update(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateCompanyData>,
) -> Result<Json<CompanyRow>, ApiError> {
    require_admin(&auth)?;
    let row = companies::update(&state.pool, id, body)
        .await
        .map_err(map_pg)?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `POST /api/companies/:id/deactivate` — soft delete.
pub async fn deactivate(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
) -> Result<Json<CompanyRow>, ApiError> {
    require_admin(&auth)?;
    let row = companies::deactivate(&state.pool, id)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

// ---------------------------------------------------------------
// Error mapping — translate Postgres codes to HTTP semantics.
//
// 23505 = unique_violation         → 409 Conflict (duplicate name)
// 23503 = foreign_key_violation    → 400 Validation
// 23514 = check_violation          → 400 Validation
// ---------------------------------------------------------------

fn map_pg(e: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(ref db_err) = e {
        if let Some(code) = db_err.code() {
            match code.as_ref() {
                "23505" => {
                    return ApiError::Domain(DomainError::Conflict(
                        "A company with this name already exists".to_string(),
                    ))
                }
                "23503" | "23514" => {
                    return ApiError::Domain(DomainError::Validation(db_err.message().to_string()))
                }
                _ => {}
            }
        }
    }
    ApiError::Db(e)
}
