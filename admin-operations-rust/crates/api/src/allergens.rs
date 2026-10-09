//! `/api/allergens/*` HTTP handlers.
//!
//! Thin layer: parse → authorise → delegate to `db::queries::allergens`
//! → serialize. Business logic (name trimming, case-insensitive lookup,
//! ref-count subqueries) lives in the db crate — this module owns the
//! HTTP contract and the admin-only scoping.
//!
//! Scoping rule — allergens are admin-only across the board. The one
//! client-facing route that reads them today
//! (`src/routes/client/roster/[id]/+page.server.ts`) stays on the SQL
//! path this commit — routes are untouched. Future work to thread
//! `caller` through that route is a follow-up with its own scope
//! decision. No `/api/allergens` client route exists today, so
//! "lie about existence" is not required.
//!
//! Error mapping:
//! - UNIQUE-name violation (23505) → 409 Conflict with the exact
//!   TS-side message (`+page.server.ts:29,47`).
//! - FK violation (23503) → 400 Validation. In practice the admin
//!   UI pre-checks `get_ref_count` before `delete`, so a 23503 only
//!   arises on a race; the join tables are ON DELETE CASCADE anyway.
//!   Mapped defensively.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use db::queries::allergens::{self, AllergenRow, InsertAllergenData, RefCount};
use domain::{DomainError, Role};
use serde::Deserialize;
use uuid::Uuid;

use crate::{auth::AuthCtx, error::ApiError, state::AppState};

// ---------------------------------------------------------------
// Scope helper — allergens are admin-only, reads and writes alike.
// ---------------------------------------------------------------

/// Reject anything but admin with 403.
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

/// `GET /api/allergens/by-name?name=…` — case-insensitive lookup.
#[derive(Debug, Deserialize)]
pub struct ByNameQuery {
    pub name: String,
}

// ---------------------------------------------------------------
// Handlers — reads (admin only)
// ---------------------------------------------------------------

/// `GET /api/allergens` — flat list ordered by name.
pub async fn list(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
) -> Result<Json<Vec<AllergenRow>>, ApiError> {
    require_admin(&auth)?;
    let rows = allergens::list(&state.pool).await?;
    Ok(Json(rows))
}

/// `GET /api/allergens/by-name?name=…` — case-insensitive. 404 on miss.
pub async fn get_by_name(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<ByNameQuery>,
) -> Result<Json<AllergenRow>, ApiError> {
    require_admin(&auth)?;
    let row = allergens::get_by_name(&state.pool, &q.name)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `GET /api/allergens/:id` — 404 on miss.
pub async fn get_by_id(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
) -> Result<Json<AllergenRow>, ApiError> {
    require_admin(&auth)?;
    let row = allergens::get_by_id(&state.pool, id)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `GET /api/allergens/:id/ref-count` — `{ ingredients, persons }`.
/// Always 200; counts are 0 when the allergen is unused. We don't
/// pre-check whether the allergen exists — missing id simply returns
/// `{0, 0}`, matching the TS semantics (two independent COUNT(*)
/// subqueries ignore the parent row's existence).
pub async fn get_ref_count(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
) -> Result<Json<RefCount>, ApiError> {
    require_admin(&auth)?;
    let counts = allergens::get_ref_count(&state.pool, id).await?;
    Ok(Json(counts))
}

// ---------------------------------------------------------------
// Handlers — writes (admin only)
// ---------------------------------------------------------------

/// `POST /api/allergens` — create. 201 on success; 409 on duplicate name.
pub async fn create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Json(body): Json<InsertAllergenData>,
) -> Result<(StatusCode, Json<AllergenRow>), ApiError> {
    require_admin(&auth)?;
    let row = allergens::create(&state.pool, body).await.map_err(map_pg)?;
    Ok((StatusCode::CREATED, Json(row)))
}

/// `PATCH /api/allergens/:id` — rename. 404 on no match, 409 on
/// duplicate name.
pub async fn update(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
    Json(body): Json<InsertAllergenData>,
) -> Result<Json<AllergenRow>, ApiError> {
    require_admin(&auth)?;
    let row = allergens::update(&state.pool, id, body)
        .await
        .map_err(map_pg)?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `DELETE /api/allergens/:id` — hard delete. 204 on success,
/// 404 when nothing matched (parity with TS `deleteAllergen`
/// returning `boolean` — the dispatcher converts 404 → `false`).
pub async fn delete(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    require_admin(&auth)?;
    let deleted = allergens::delete(&state.pool, id).await.map_err(map_pg)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::Domain(DomainError::NotFound))
    }
}

// ---------------------------------------------------------------
// Error mapping — translate Postgres codes to HTTP semantics.
//
// 23505 = unique_violation         → 409 Conflict (duplicate name)
// 23503 = foreign_key_violation    → 400 Validation (defensive —
//   join tables ON DELETE CASCADE so this is only reachable on
//   schema drift)
// ---------------------------------------------------------------

fn map_pg(e: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(ref db_err) = e {
        if let Some(code) = db_err.code() {
            match code.as_ref() {
                "23505" => {
                    return ApiError::Domain(DomainError::Conflict(
                        "An allergen with this name already exists".to_string(),
                    ))
                }
                "23503" => {
                    return ApiError::Domain(DomainError::Validation(db_err.message().to_string()))
                }
                _ => {}
            }
        }
    }
    ApiError::Db(e)
}
