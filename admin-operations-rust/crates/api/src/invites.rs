//! `/api/invites/*` HTTP handlers.
//!
//! Thin layer: parse → authorise → delegate to `db::queries::invites`
//! → serialize. Admin-only across the board — the one unauthenticated
//! TS consumer (`src/routes/invite/[token]/+page.server.ts`) keeps
//! using the pg SQL path in this commit, consistent with every other
//! route in the Track 3 Step 3 series.
//!
//! Error mapping:
//! - UNIQUE-token violation (23505) → 409 Conflict. In practice tokens
//!   are freshly-minted `randomUUID()`s so a collision requires a
//!   universe-ending bug, but we map it defensively.
//! - FK violation on `company_id` (23503) → 400 Validation.
//!
//! `mark_used` and `revoke` emit the same SQL at the db layer — we
//! expose two handlers so the TS surface's two endpoints map 1:1
//! (dispatcher stays mechanical).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use db::queries::invites::{self, InsertInviteData, InviteRow};
use domain::{DomainError, Role};
use serde::Deserialize;
use uuid::Uuid;

use crate::{auth::AuthCtx, error::ApiError, state::AppState};

// ---------------------------------------------------------------
// Scope helper — invites are admin-only, reads and writes alike.
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

/// `GET /api/invites?company_id=…` — flat collection scoped by company.
/// Mirrors the orders/persons list precedent (filters as query params).
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub company_id: Uuid,
}

/// `GET /api/invites/by-token?token=…` — exact-match lookup.
#[derive(Debug, Deserialize)]
pub struct ByTokenQuery {
    pub token: String,
}

// ---------------------------------------------------------------
// Handlers — reads (admin only)
// ---------------------------------------------------------------

/// `GET /api/invites?company_id=…` — ordered `created_at DESC`.
pub async fn list(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<InviteRow>>, ApiError> {
    require_admin(&auth)?;
    let rows = invites::list_by_company(&state.pool, q.company_id).await?;
    Ok(Json(rows))
}

/// `GET /api/invites/by-token?token=…` — 404 on miss.
pub async fn get_by_token(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<ByTokenQuery>,
) -> Result<Json<InviteRow>, ApiError> {
    require_admin(&auth)?;
    let row = invites::get_by_token(&state.pool, &q.token)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

// ---------------------------------------------------------------
// Handlers — writes (admin only)
// ---------------------------------------------------------------

/// `POST /api/invites` — create. 201 on success; 409 on duplicate token.
pub async fn create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Json(body): Json<InsertInviteData>,
) -> Result<(StatusCode, Json<InviteRow>), ApiError> {
    require_admin(&auth)?;
    let row = invites::create(&state.pool, body).await.map_err(map_pg)?;
    Ok((StatusCode::CREATED, Json(row)))
}

/// `POST /api/invites/:id/mark-used` — stamp `used_at = now()`.
/// 204 on success, 404 when nothing matched.
pub async fn mark_used(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    require_admin(&auth)?;
    let touched = invites::mark_used(&state.pool, id).await?;
    if touched {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::Domain(DomainError::NotFound))
    }
}

/// `POST /api/invites/:id/revoke` — same SQL as `mark_used` on the TS
/// side. Kept as a separate route for 1:1 parity with the TS surface
/// (`revokeInvite`); the dispatcher calls this endpoint for `revoke`
/// and `mark-used` for `markInviteUsed`.
pub async fn revoke(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    require_admin(&auth)?;
    let touched = invites::mark_used(&state.pool, id).await?;
    if touched {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::Domain(DomainError::NotFound))
    }
}

// ---------------------------------------------------------------
// Error mapping — translate Postgres codes to HTTP semantics.
//
// 23505 = unique_violation         → 409 Conflict (duplicate token)
// 23503 = foreign_key_violation    → 400 Validation (bad company_id)
// ---------------------------------------------------------------

fn map_pg(e: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(ref db_err) = e {
        if let Some(code) = db_err.code() {
            match code.as_ref() {
                "23505" => {
                    return ApiError::Domain(DomainError::Conflict(
                        "An invite with this token already exists".to_string(),
                    ))
                }
                "23503" => {
                    return ApiError::Domain(DomainError::Validation(
                        db_err.message().to_string(),
                    ))
                }
                _ => {}
            }
        }
    }
    ApiError::Db(e)
}
