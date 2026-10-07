//! `/api/users/*` HTTP handlers.
//!
//! Thin layer: parse → authorise → delegate to `db::queries::users`
//! → serialize. No TS `queries/users.ts` counterpart exists — the
//! pre-port state had inline SQL in a handful of SvelteKit route
//! handlers (`src/routes/admin/companies/[id]/+page.server.ts` and
//! `src/routes/invite/[token]/+page.server.ts`). Those routes cut
//! over directly to this surface.
//!
//! Scope note: Better Auth internals (session, account, password
//! hashing) stay in TS — `auth.$context.internalAdapter.*` and
//! `ctx.password.hash` are server-side Node APIs, not HTTP surfaces.
//! This module only exposes the data-access cuts: email lookup,
//! ownership-scoped get, and `company_id` assignment.
//!
//! Path params use `Path<String>` — Better Auth user ids are `TEXT`,
//! not UUIDs, so a `Path<Uuid>` extractor would 400 on every call.
//!
//! Authorisation:
//! - `get_by_email` and `get_by_id_in_company`: admin only.
//! - `assign_company`: admin unrestricted; a `Client` may self-stamp
//!   iff `auth.user_id == path id` AND they're currently unaffiliated
//!   (`auth.company_id.is_none()`). This matches the invite-acceptance
//!   flow, where a freshly-created client user tags their own row
//!   with the invite's `company_id`.

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use db::queries::users::{self, UserCompanyRow, UserIdRow, UserOwnedRow};
use domain::{DomainError, Role};
use serde::Deserialize;
use uuid::Uuid;

use crate::{auth::AuthCtx, error::ApiError, state::AppState};

// ---------------------------------------------------------------
// Scope helper — reads are admin-only.
// ---------------------------------------------------------------

fn require_admin(auth: &AuthCtx) -> Result<(), ApiError> {
    if auth.role == Role::Admin {
        Ok(())
    } else {
        Err(ApiError::Domain(DomainError::Forbidden))
    }
}

// ---------------------------------------------------------------
// Query / body shapes
// ---------------------------------------------------------------

/// `GET /api/users/by-email?email=…` — case-sensitive lookup (emails
/// are stored lowercased by Better Auth and by the admin create-user
/// action, so a plain `=` comparison is correct at the SQL layer).
#[derive(Debug, Deserialize)]
pub struct ByEmailQuery {
    pub email: String,
}

/// `GET /api/users/:id?company_id=…` — the `company_id` filter is the
/// ownership gate, not an optional convenience: the sole caller
/// (`resetUserPassword`) needs it to stop a `/companies/X` admin from
/// touching a `company_id = Y` user.
#[derive(Debug, Deserialize)]
pub struct GetByIdQuery {
    pub company_id: Uuid,
}

/// `POST /api/users/:id/assign-company` body.
#[derive(Debug, Deserialize)]
pub struct AssignCompanyBody {
    pub company_id: Uuid,
}

// ---------------------------------------------------------------
// Handlers — reads (admin only)
// ---------------------------------------------------------------

/// `GET /api/users/by-email?email=…` — 404 on miss. Used as a
/// pre-insert duplicate probe by `createUserWithPassword`.
pub async fn get_by_email(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<ByEmailQuery>,
) -> Result<Json<UserIdRow>, ApiError> {
    require_admin(&auth)?;
    let row = users::get_by_email(&state.pool, &q.email)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `GET /api/users/:id?company_id=…` — ownership-scoped lookup.
/// Returns 404 when the user doesn't exist OR doesn't belong to the
/// supplied company — the two are indistinguishable to the caller by
/// design (stops `/companies/X` admins from probing for ids owned by
/// `/companies/Y`).
pub async fn get_by_id_in_company(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(user_id): Path<String>,
    Query(q): Query<GetByIdQuery>,
) -> Result<Json<UserOwnedRow>, ApiError> {
    require_admin(&auth)?;
    let row = users::get_by_id_in_company(&state.pool, &user_id, q.company_id)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

// ---------------------------------------------------------------
// Handlers — writes
// ---------------------------------------------------------------

/// `POST /api/users/:id/assign-company` — stamp `company_id` on a
/// user row and echo `{id, company_id}` back.
///
/// Admin: unrestricted (used by `createUserWithPassword` after Better
/// Auth creates the row, since `company_id` is `input: false` in the
/// admin plugin config and can't flow in through the create call).
///
/// Client: self-stamp only, and only when currently unaffiliated —
/// this is the invite-acceptance path. Any other `Client` call is a
/// 403; in particular, a client who already has a `company_id` can't
/// move themselves to another company via this endpoint.
///
/// 404 when `:id` doesn't match any row.
pub async fn assign_company(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(user_id): Path<String>,
    Json(body): Json<AssignCompanyBody>,
) -> Result<Json<UserCompanyRow>, ApiError> {
    match auth.role {
        Role::Admin => {}
        Role::Client => {
            if auth.user_id != user_id || auth.company_id.is_some() {
                return Err(ApiError::Domain(DomainError::Forbidden));
            }
        }
    }
    let row = users::assign_company(&state.pool, &user_id, body.company_id)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}
