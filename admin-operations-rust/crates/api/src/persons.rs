//! `/api/persons/*` + `/api/attendance/*` HTTP handlers.
//!
//! Thin layer: parse → authorise → delegate to `db::queries::persons`
//! → serialize. Business logic (COALESCE updates, ARRAY_AGG + manual
//! zip for allergen embeds, batch upsert via QueryBuilder) lives in
//! the db crate — this module owns the HTTP contract and the admin-vs-
//! client scoping.
//!
//! Scoping rules — persons are per-company master data and surface in
//! BOTH admin and client routes (unlike ingredients which are admin-
//! only). The TS consumers are:
//!   - admin: `src/routes/admin/companies/[id]` (lists any company's roster)
//!   - client: `src/routes/client/roster`, `/client/roster/[id]`,
//!     `/client/attendance`, `/client/menu`
//!
//! The client routes always operate on the caller's own `company_id`.
//! Mirror that here:
//!   - `role == Admin` → unrestricted.
//!   - `role == Client` → may only read / write persons + attendance
//!     for its own `company_id`. Cross-company access returns 404
//!     ("lie about existence" rule — a client probing with a stolen id
//!     learns nothing); missing `company_id` on a client token is 403.
//!
//! Single-person endpoints (`GET/PATCH /:id`, deactivate, allergen
//! mutations, with-allergens, allergens) pre-fetch the person via
//! `ensure_client_can_touch_person` and compare `person.company_id`
//! against `auth.company_id` before delegating.
//!
//! Error mapping:
//! - FK violation (23503) / CHECK violation (23514) → 400 Validation.
//! - No 23505 translation — `person(name)` has no UNIQUE constraint
//!   (a company may have two "John Smith" entries, siblings etc.),
//!   and `attendance` upserts resolve the one unique it has
//!   (`(person_id, serving_date)`) via `ON CONFLICT DO UPDATE`.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::NaiveDate;
use db::queries::persons::{
    self, AllergenEmbed, AttendanceRow, AttendanceWithPersonRow, InsertPersonData, ListOpts,
    PersonRow, PersonWithAllergensRow, UpdatePersonData, UpsertAttendanceData,
};
use domain::{DomainError, Role};
use serde::Deserialize;
use uuid::Uuid;

use crate::{auth::AuthCtx, error::ApiError, state::AppState};

// ---------------------------------------------------------------
// Scope helpers.
//
// Three shapes:
//  - `can_read_company_persons`: for list / attendance endpoints that
//    take a `company_id` query param. Admin passes; client's token
//    `company_id` must match. 404 on mismatch (not 403) to match the
//    companies/orders "lie about existence" rule.
//  - `ensure_client_can_touch_person`: for `/:id` endpoints. Pre-fetch
//    the person (with `include_inactive=true` so touches on a
//    soft-deleted row behave the same for admin and client — the TS
//    surface treats inactive as "existing but filtered") and compare
//    its company_id against the client's. 404 on mismatch; returns
//    the fetched row so the handler can reuse it. Admin skips the
//    fetch (returns None) and lets the inner query handle 404.
//  - `require_admin`: unused here — every write persons supports is
//    also allowed for the owning client. Kept in mind if future
//    endpoints need it.
// ---------------------------------------------------------------

/// Admin → OK. Client → `auth.company_id` must equal `company_id`.
/// 404 (not 403) on mismatch to prevent cross-company id probing.
/// Missing `company_id` on a client token is 403 (malformed caller).
fn can_read_company_persons(auth: &AuthCtx, company_id: Uuid) -> Result<(), ApiError> {
    if auth.role == Role::Admin {
        return Ok(());
    }
    let cid = auth
        .company_id
        .ok_or(ApiError::Domain(DomainError::Forbidden))?;
    if cid == company_id {
        Ok(())
    } else {
        Err(ApiError::Domain(DomainError::NotFound))
    }
}

/// Pre-fetch a person for a `/:id` endpoint and gate on company scope.
///
/// Returns `Ok(Some(row))` when the caller is a client (so the handler
/// can skip a second fetch) and `Ok(None)` when the caller is admin
/// (the handler proceeds and the inner query handles not-found). On
/// client cross-company access returns 404.
///
/// We fetch with `include_inactive = true` because the TS surface has
/// callers that pass `includeInactive` through — the handler is free
/// to filter afterwards. Keeping the scope check uniform means a
/// client probing a known-inactive id from another company still gets
/// 404, same as an active one.
async fn ensure_client_can_touch_person(
    state: &AppState,
    auth: &AuthCtx,
    id: Uuid,
) -> Result<Option<PersonRow>, ApiError> {
    if auth.role == Role::Admin {
        return Ok(None);
    }
    let cid = auth
        .company_id
        .ok_or(ApiError::Domain(DomainError::Forbidden))?;
    let row = persons::get_by_id(&state.pool, id, true)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    if row.company_id != cid {
        return Err(ApiError::Domain(DomainError::NotFound));
    }
    Ok(Some(row))
}

// ---------------------------------------------------------------
// Query shapes
// ---------------------------------------------------------------

/// `GET /api/persons` / `/with-allergens`: scoped by `company_id`,
/// with optional `include_inactive`.
#[derive(Debug, Deserialize)]
pub struct ListByCompanyQuery {
    pub company_id: Uuid,
    #[serde(default)]
    pub include_inactive: bool,
}

/// `GET /api/persons/:id` / `/:id/with-allergens`: `?include_inactive=true`
/// to see soft-deleted rows.
#[derive(Debug, Default, Deserialize)]
pub struct GetByIdQuery {
    #[serde(default)]
    pub include_inactive: bool,
}

/// `GET /api/attendance?company_id=&serving_date=`.
#[derive(Debug, Deserialize)]
pub struct AttendanceQuery {
    pub company_id: Uuid,
    pub serving_date: NaiveDate,
}

/// `PUT /api/persons/:id/allergens` — full-replace body.
#[derive(Debug, Deserialize)]
pub struct SetAllergensBody {
    pub allergen_ids: Vec<Uuid>,
}

/// `POST /api/attendance/batch` — bulk upsert body.
#[derive(Debug, Deserialize)]
pub struct UpsertAttendanceBatchBody {
    pub records: Vec<UpsertAttendanceData>,
}

// ---------------------------------------------------------------
// Handlers — list reads
// ---------------------------------------------------------------

/// `GET /api/persons?company_id=&include_inactive=` — flat list.
pub async fn list(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<ListByCompanyQuery>,
) -> Result<Json<Vec<PersonRow>>, ApiError> {
    can_read_company_persons(&auth, q.company_id)?;
    let opts = ListOpts {
        include_inactive: q.include_inactive,
    };
    let rows = persons::list_by_company(&state.pool, q.company_id, &opts).await?;
    Ok(Json(rows))
}

/// `GET /api/persons/with-allergens?company_id=&include_inactive=` —
/// flat list with nested allergen arrays.
pub async fn list_with_allergens(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<ListByCompanyQuery>,
) -> Result<Json<Vec<PersonWithAllergensRow>>, ApiError> {
    can_read_company_persons(&auth, q.company_id)?;
    let opts = ListOpts {
        include_inactive: q.include_inactive,
    };
    let rows = persons::list_with_allergens_by_company(&state.pool, q.company_id, &opts).await?;
    Ok(Json(rows))
}

// ---------------------------------------------------------------
// Handlers — single-person reads
// ---------------------------------------------------------------

/// `GET /api/persons/:id` — flat row. 404 on miss, inactive-unless-
/// `include_inactive`, or client cross-company probe.
pub async fn get_by_id(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
    Query(q): Query<GetByIdQuery>,
) -> Result<Json<PersonRow>, ApiError> {
    let prefetched = ensure_client_can_touch_person(&state, &auth, id).await?;

    // Client scope satisfied — reuse the already-fetched row IF the
    // active-filter matches what the handler was asked for. Otherwise
    // (admin, or client asked for active-only on an inactive row)
    // re-run the inner query with the right filter so 404 semantics
    // match the TS surface.
    if let Some(row) = prefetched {
        if q.include_inactive || row.active {
            return Ok(Json(row));
        }
        return Err(ApiError::Domain(DomainError::NotFound));
    }

    let row = persons::get_by_id(&state.pool, id, q.include_inactive)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `GET /api/persons/:id/with-allergens` — nested shape, 404 on miss.
pub async fn get_with_allergens(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
    Query(q): Query<GetByIdQuery>,
) -> Result<Json<PersonWithAllergensRow>, ApiError> {
    ensure_client_can_touch_person(&state, &auth, id).await?;
    let row = persons::get_with_allergens(&state.pool, id, q.include_inactive)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `GET /api/persons/:id/allergens` — just the allergens for a person.
/// Returns `[]` if the person has none; 404 only if the person itself
/// is unreachable to the caller (cross-company probe).
pub async fn get_allergens(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<AllergenEmbed>>, ApiError> {
    ensure_client_can_touch_person(&state, &auth, id).await?;
    let rows = persons::get_allergens(&state.pool, id).await?;
    Ok(Json(rows))
}

// ---------------------------------------------------------------
// Handlers — person writes
// ---------------------------------------------------------------

/// `POST /api/persons` — create. 201 on success. Client callers can
/// only create persons under their own `company_id`; cross-company
/// `company_id` in the body is 404 (lie about existence of the target
/// company — same rule as reads).
pub async fn create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Json(body): Json<InsertPersonData>,
) -> Result<(StatusCode, Json<PersonRow>), ApiError> {
    can_read_company_persons(&auth, body.company_id)?;
    let row = persons::create(&state.pool, body).await.map_err(map_pg)?;
    Ok((StatusCode::CREATED, Json(row)))
}

/// `PATCH /api/persons/:id` — partial update. 404 on no match or
/// client cross-company probe.
pub async fn update(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdatePersonData>,
) -> Result<Json<PersonRow>, ApiError> {
    ensure_client_can_touch_person(&state, &auth, id).await?;
    let row = persons::update(&state.pool, id, body)
        .await
        .map_err(map_pg)?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `POST /api/persons/:id/deactivate` — soft delete.
pub async fn deactivate(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
) -> Result<Json<PersonRow>, ApiError> {
    ensure_client_can_touch_person(&state, &auth, id).await?;
    let row = persons::deactivate(&state.pool, id)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

// ---------------------------------------------------------------
// Handlers — allergen associations
// ---------------------------------------------------------------

/// `POST /api/persons/:id/allergens/:allergen_id` — attach one.
pub async fn add_allergen(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path((person_id, allergen_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    ensure_client_can_touch_person(&state, &auth, person_id).await?;
    persons::add_allergen(&state.pool, person_id, allergen_id)
        .await
        .map_err(map_pg)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/persons/:id/allergens/:allergen_id` — detach one.
pub async fn remove_allergen(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path((person_id, allergen_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    ensure_client_can_touch_person(&state, &auth, person_id).await?;
    persons::remove_allergen(&state.pool, person_id, allergen_id)
        .await
        .map_err(map_pg)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `PUT /api/persons/:id/allergens` — full-replace, atomic.
pub async fn set_allergens(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
    Json(body): Json<SetAllergensBody>,
) -> Result<StatusCode, ApiError> {
    ensure_client_can_touch_person(&state, &auth, id).await?;
    persons::set_allergens(&state.pool, id, &body.allergen_ids)
        .await
        .map_err(map_pg)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------
// Handlers — attendance
// ---------------------------------------------------------------

/// `GET /api/attendance?company_id=&serving_date=`.
pub async fn get_attendance_by_date(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<AttendanceQuery>,
) -> Result<Json<Vec<AttendanceWithPersonRow>>, ApiError> {
    can_read_company_persons(&auth, q.company_id)?;
    let rows = persons::get_attendance_by_date(&state.pool, q.company_id, q.serving_date).await?;
    Ok(Json(rows))
}

/// `POST /api/attendance` — single-row upsert. Client callers may
/// only upsert attendance for persons in their own company — enforced
/// by pre-fetching the person and comparing company_id.
pub async fn upsert_attendance(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Json(body): Json<UpsertAttendanceData>,
) -> Result<Json<AttendanceRow>, ApiError> {
    ensure_client_can_touch_person(&state, &auth, body.person_id).await?;
    let row = persons::upsert_attendance(&state.pool, body)
        .await
        .map_err(map_pg)?;
    Ok(Json(row))
}

/// `POST /api/attendance/batch` — bulk upsert. Every record's
/// `person_id` is checked against the caller's company scope before
/// the batch is submitted; a cross-company id anywhere in the batch
/// 404s the whole thing (no partial writes). Admin skips the check.
pub async fn upsert_attendance_batch(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Json(body): Json<UpsertAttendanceBatchBody>,
) -> Result<StatusCode, ApiError> {
    if auth.role != Role::Admin {
        let cid = auth
            .company_id
            .ok_or(ApiError::Domain(DomainError::Forbidden))?;
        // Dedup person_ids first so N identical rows cost one query.
        let mut unique: Vec<Uuid> = body.records.iter().map(|r| r.person_id).collect();
        unique.sort();
        unique.dedup();
        for pid in unique {
            let row = persons::get_by_id(&state.pool, pid, true)
                .await?
                .ok_or(ApiError::Domain(DomainError::NotFound))?;
            if row.company_id != cid {
                return Err(ApiError::Domain(DomainError::NotFound));
            }
        }
    }

    persons::upsert_attendance_batch(&state.pool, &body.records)
        .await
        .map_err(map_pg)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------
// Error mapping — translate Postgres codes to HTTP semantics.
//
// 23503 = foreign_key_violation    → 400 Validation
// 23514 = check_violation          → 400 Validation
//
// No 23505 branch — persons has no UNIQUE constraint on `name`, and
// attendance's `(person_id, serving_date)` UNIQUE is resolved by
// `ON CONFLICT DO UPDATE` inside the query layer, so a 23505 never
// surfaces here.
// ---------------------------------------------------------------

fn map_pg(e: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(ref db_err) = e {
        if let Some(code) = db_err.code() {
            if matches!(code.as_ref(), "23503" | "23514") {
                return ApiError::Domain(DomainError::Validation(db_err.message().to_string()));
            }
        }
    }
    ApiError::Db(e)
}
