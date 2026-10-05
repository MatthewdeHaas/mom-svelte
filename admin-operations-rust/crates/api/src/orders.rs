//! `/api/orders/*` HTTP handlers.
//!
//! Thin layer: parse → authorise → delegate to `db::queries::orders`
//! → serialize. All business logic (COALESCE, status transition
//! guards, filter stacking) lives in the db crate — this module only
//! owns the HTTP contract and the admin-vs-client scoping.
//!
//! Scoping rules, in one place:
//! - `role == Admin` → unrestricted. Filters flow through as-is.
//! - `role == Client` → every read/write is constrained to the
//!   caller's `company_id`. The server-side filter is injected into
//!   `OrderFilters.company_id` regardless of what the client sent,
//!   and mutations verify the targeted row belongs to the caller's
//!   company (either via a `company_id` SQL predicate or an explicit
//!   pre-check that returns 404 on mismatch).
//!
//! A client without a `company_id` on the auth header is treated as
//! unauthorised (403) — the SvelteKit login flow guarantees every
//! client user has a company; a missing one means something upstream
//! lied.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::NaiveDate;
use db::queries::{
    orders::{
        self, InsertOrderData, OrderDetailRow, OrderFilters, OrderRow, OrderStatusCounts,
        OrderSummaryRow, UpdateOrderData,
    },
    pagination::{parse_from_query, Paginated},
};
use domain::{DomainError, Role};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{auth::AuthCtx, error::ApiError, state::AppState};

// ---------------------------------------------------------------
// Validation helpers — reject bad enum values at the API boundary so
// handlers don't have to care about CHECK-constraint errors.
// ---------------------------------------------------------------

const VALID_STATUSES: &[&str] = &["pending", "confirmed", "fulfilled", "cancelled"];
const VALID_SOURCES: &[&str] = &["admin_manual", "excel_import", "client_portal"];

fn validate_status(s: &str) -> Result<(), ApiError> {
    if VALID_STATUSES.contains(&s) {
        Ok(())
    } else {
        Err(ApiError::Domain(DomainError::Validation(format!(
            "invalid status: {s}"
        ))))
    }
}

fn validate_source(s: &str) -> Result<(), ApiError> {
    if VALID_SOURCES.contains(&s) {
        Ok(())
    } else {
        Err(ApiError::Domain(DomainError::Validation(format!(
            "invalid order_source: {s}"
        ))))
    }
}

/// Apply client scoping to an `OrderFilters` the handler received off
/// the query string. For admins the filter is returned unchanged; for
/// clients the `company_id` is forced to the caller's company
/// (regardless of what the query string tried to set).
fn scope_filters(mut filters: OrderFilters, auth: &AuthCtx) -> Result<OrderFilters, ApiError> {
    if auth.role == Role::Client {
        let cid = auth
            .company_id
            .ok_or(ApiError::Domain(DomainError::Forbidden))?;
        filters.company_id = Some(cid);
    }
    Ok(filters)
}

/// Pre-check for single-row mutations: return 404 if the row doesn't
/// exist, 403 if the caller is a client scoped to a different company.
async fn ensure_client_can_touch(
    state: &AppState,
    auth: &AuthCtx,
    id: Uuid,
) -> Result<(), ApiError> {
    if auth.role != Role::Client {
        return Ok(());
    }
    let cid = auth
        .company_id
        .ok_or(ApiError::Domain(DomainError::Forbidden))?;
    match orders::get_company_id(&state.pool, id).await? {
        None => Err(ApiError::Domain(DomainError::NotFound)),
        Some(row_cid) if row_cid != cid => {
            // Lie about existence: a client probing with a stolen id
            // learns nothing beyond "not yours".
            Err(ApiError::Domain(DomainError::NotFound))
        }
        Some(_) => Ok(()),
    }
}

// ---------------------------------------------------------------
// Query/body shapes
// ---------------------------------------------------------------

/// `GET /api/orders/paginated` — `OrderFilters` fields plus
/// `page` / `per_page`. `#[serde(flatten)]` keeps the wire format
/// flat (all fields at the top level of the query string) so TS
/// doesn't need a nested encoder.
#[derive(Debug, Deserialize)]
pub struct ListPaginatedQuery {
    #[serde(flatten)]
    pub filters: OrderFilters,
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default, rename = "per_page")]
    pub per_page: Option<i64>,
}

/// `GET /api/orders/counts` — date-range filter only (counts are
/// unfiltered otherwise).
#[derive(Debug, Deserialize)]
pub struct CountsQuery {
    #[serde(default)]
    pub serving_date_from: Option<NaiveDate>,
    #[serde(default)]
    pub serving_date_to: Option<NaiveDate>,
}

/// `POST /api/orders/batch/status` — set the given ids to `status`.
#[derive(Debug, Deserialize)]
pub struct SetStatusBatchBody {
    pub ids: Vec<Uuid>,
    pub status: String,
}

/// Response wrapper for `GET /api/orders/counts`. Flat shape, matches
/// `Record<OrderStatus, number>` on the TS side.
#[derive(Debug, Serialize)]
pub struct CountsResponse {
    pub pending: i64,
    pub confirmed: i64,
    pub fulfilled: i64,
    pub cancelled: i64,
}

impl From<OrderStatusCounts> for CountsResponse {
    fn from(c: OrderStatusCounts) -> Self {
        Self {
            pending: c.pending,
            confirmed: c.confirmed,
            fulfilled: c.fulfilled,
            cancelled: c.cancelled,
        }
    }
}

// ---------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------

/// `GET /api/orders` — unpaginated list, filtered.
pub async fn list(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(filters): Query<OrderFilters>,
) -> Result<Json<Vec<OrderSummaryRow>>, ApiError> {
    let filters = scope_filters(filters, &auth)?;
    let rows = orders::list(&state.pool, &filters).await?;
    Ok(Json(rows))
}

/// `GET /api/orders/paginated` — paginated list.
pub async fn list_paginated(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(params): Query<ListPaginatedQuery>,
) -> Result<Json<Paginated<OrderSummaryRow>>, ApiError> {
    let filters = scope_filters(params.filters, &auth)?;
    // 50-row default per endpoint — same as TS.
    let pagination = parse_from_query(params.page, params.per_page, 50);
    let page = orders::list_paginated(&state.pool, &filters, pagination).await?;
    Ok(Json(page))
}

/// `GET /api/orders/:id` — detail. 404 on no match; clients get 404
/// even if the row exists but belongs to another company.
pub async fn get_by_id(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
) -> Result<Json<OrderDetailRow>, ApiError> {
    let row = orders::get_by_id(&state.pool, id)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;

    if auth.role == Role::Client {
        let cid = auth
            .company_id
            .ok_or(ApiError::Domain(DomainError::Forbidden))?;
        if row.company_id != cid {
            return Err(ApiError::Domain(DomainError::NotFound));
        }
    }

    Ok(Json(row))
}

/// `GET /api/orders/counts` — status counts over an optional date
/// range. Client-scoped by `company_id` — we can't just forward to
/// `counts_by_status` for clients because its SQL doesn't filter on
/// company. Fall through to the full `list` for clients and bucket
/// in-memory; cheap for client-scale datasets.
pub async fn counts(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(params): Query<CountsQuery>,
) -> Result<Json<CountsResponse>, ApiError> {
    if auth.role == Role::Admin {
        let c = orders::counts_by_status(
            &state.pool,
            params.serving_date_from,
            params.serving_date_to,
        )
        .await?;
        return Ok(Json(c.into()));
    }

    // Client path — bucket from the scoped list.
    let cid = auth
        .company_id
        .ok_or(ApiError::Domain(DomainError::Forbidden))?;
    let filters = OrderFilters {
        company_id: Some(cid),
        serving_date_from: params.serving_date_from,
        serving_date_to: params.serving_date_to,
        ..Default::default()
    };
    let rows = orders::list(&state.pool, &filters).await?;

    let mut out = OrderStatusCounts::default();
    for r in &rows {
        match r.status.as_str() {
            "pending" => out.pending += 1,
            "confirmed" => out.confirmed += 1,
            "fulfilled" => out.fulfilled += 1,
            "cancelled" => out.cancelled += 1,
            _ => {}
        }
    }
    Ok(Json(out.into()))
}

/// `POST /api/orders` — create. 201 on success. Enum validation at the
/// boundary; FK violations surface as 400 via the Postgres error
/// mapping.
pub async fn create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Json(body): Json<InsertOrderData>,
) -> Result<(StatusCode, Json<OrderRow>), ApiError> {
    validate_source(&body.order_source)?;
    if let Some(ref s) = body.status {
        validate_status(s)?;
    }

    // Client scope: force company_id to the caller's company — a
    // client trying to insert against a different company is a 403.
    let body = if auth.role == Role::Client {
        let cid = auth
            .company_id
            .ok_or(ApiError::Domain(DomainError::Forbidden))?;
        if body.company_id != cid {
            return Err(ApiError::Domain(DomainError::Forbidden));
        }
        body
    } else {
        body
    };

    let row = orders::create(&state.pool, body).await.map_err(map_fk)?;
    Ok((StatusCode::CREATED, Json(row)))
}

/// `PATCH /api/orders/:id` — partial update. 404 on no match.
pub async fn update(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateOrderData>,
) -> Result<Json<OrderRow>, ApiError> {
    if let Some(ref s) = body.status {
        validate_status(s)?;
    }
    ensure_client_can_touch(&state, &auth, id).await?;

    let row = orders::update(&state.pool, id, body)
        .await
        .map_err(map_fk)?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `POST /api/orders/:id/confirm` — status transition `pending →
/// confirmed`. 404 if the row doesn't exist OR wasn't pending.
pub async fn confirm(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
) -> Result<Json<OrderRow>, ApiError> {
    ensure_client_can_touch(&state, &auth, id).await?;
    let row = orders::confirm(&state.pool, id)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `POST /api/orders/:id/fulfill` — status transition `confirmed →
/// fulfilled`.
pub async fn fulfill(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
) -> Result<Json<OrderRow>, ApiError> {
    ensure_client_can_touch(&state, &auth, id).await?;
    let row = orders::fulfill(&state.pool, id)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `POST /api/orders/:id/cancel` — status transition
/// `pending|confirmed → cancelled`.
pub async fn cancel(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
) -> Result<Json<OrderRow>, ApiError> {
    ensure_client_can_touch(&state, &auth, id).await?;
    let row = orders::cancel(&state.pool, id)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `POST /api/orders/batch/status` — bulk unconditional status set.
/// Clients can only touch their own orders — any id in the batch
/// outside their company silently drops (matches the "lie about
/// existence" rule for single-row endpoints).
pub async fn set_status_batch(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Json(body): Json<SetStatusBatchBody>,
) -> Result<Json<Vec<OrderRow>>, ApiError> {
    validate_status(&body.status)?;

    let ids: Vec<Uuid> = if auth.role == Role::Client {
        // Filter the batch down to ids this client owns. Doing the
        // check per-id in a single query keeps the write path to one
        // UPDATE regardless of batch size.
        let cid = auth
            .company_id
            .ok_or(ApiError::Domain(DomainError::Forbidden))?;
        if body.ids.is_empty() {
            Vec::new()
        } else {
            sqlx::query_scalar!(
                r#"SELECT id AS "id!: Uuid" FROM "order" WHERE id = ANY($1::uuid[]) AND company_id = $2"#,
                &body.ids,
                cid,
            )
            .fetch_all(&state.pool)
            .await?
        }
    } else {
        body.ids
    };

    let rows = orders::set_status_batch(&state.pool, &ids, &body.status).await?;
    Ok(Json(rows))
}

// ---------------------------------------------------------------
// Error mapping — Postgres FK violations surface as 400, not 500.
// 23503 is foreign_key_violation; 23514 is check_violation. Both
// are user-correctable, so a 400 with the server message helps the
// caller fix their payload instead of filing a bug.
// ---------------------------------------------------------------

fn map_fk(e: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(ref db_err) = e {
        if let Some(code) = db_err.code() {
            if code == "23503" || code == "23514" {
                return ApiError::Domain(DomainError::Validation(db_err.message().to_string()));
            }
        }
    }
    ApiError::Db(e)
}
