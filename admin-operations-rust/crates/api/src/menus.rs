//! `/api/menus/*` HTTP handlers.
//!
//! Thin layer: parse → authorise → delegate to `db::queries::menus`
//! → serialize. Business logic (COALESCE, DELETE+bulk-INSERT for
//! `set_all_slots`, menu+slots two-query layering) lives in the db
//! crate — this module owns the HTTP contract and the admin-vs-client
//! scoping.
//!
//! Scoping rules — menus are GLOBAL master data (no `menu.company_id`),
//! so unlike persons/orders there is no row-level company scoping. The
//! client-facing routes consume `getMenusWithRecipes` (for the menu
//! view and new-order picker) and `getMenuSlot` (indirectly, via order
//! submission) — those endpoints are reads and must stay open to clients.
//! Writes are admin-only, matching recipes:
//! - `role == Admin` → unrestricted.
//! - `role == Client` → **reads only**. Every mutation (POST / PATCH /
//!   PUT / DELETE / deactivate) returns 403.
//!
//! Error mapping:
//! - UNIQUE-name violation (23505) → 409 Conflict.
//! - FK violation (23503) / CHECK violation (23514 — e.g. the
//!   `end_date > start_date` constraint, or `cycle_week BETWEEN 1 AND 4`,
//!   or the `day_of_week` enum CHECK) → 400 Validation.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::NaiveDate;
use db::queries::menus::{
    self, InsertMenuData, InsertMenuRecipeData, ListOpts, MenuRecipeRow, MenuRecipeWithNameRow,
    MenuRow, MenuWithRecipesRow, UpdateMenuData,
};
use domain::{DomainError, Role};
use serde::Deserialize;
use uuid::Uuid;

use crate::{auth::AuthCtx, error::ApiError, state::AppState};

// ---------------------------------------------------------------
// Scope helper — menus are read-free, write-admin-only.
// ---------------------------------------------------------------

/// Reject writes from client callers with 403. Admin passes through.
/// Reads do not call this.
fn scope_write(auth: &AuthCtx) -> Result<(), ApiError> {
    if auth.role == Role::Client {
        Err(ApiError::Domain(DomainError::Forbidden))
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------
// Query shapes
// ---------------------------------------------------------------

/// `GET /api/menus` / `/with-recipes`: `includeInactive`.
#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub include_inactive: bool,
}

impl From<ListQuery> for ListOpts {
    fn from(q: ListQuery) -> Self {
        Self {
            include_inactive: q.include_inactive,
        }
    }
}

/// `GET /api/menus/:id` / `/:id/with-recipes` — `?include_inactive=true`.
#[derive(Debug, Default, Deserialize)]
pub struct GetByIdQuery {
    #[serde(default)]
    pub include_inactive: bool,
}

/// `GET /api/menus/active-for-date?date=YYYY-MM-DD`.
#[derive(Debug, Deserialize)]
pub struct ActiveForDateQuery {
    pub date: NaiveDate,
}

/// `GET /api/menus/:id/slots/by-coord?cycle_week=&day_of_week=&meal_category=`.
#[derive(Debug, Deserialize)]
pub struct SlotCoordQuery {
    pub cycle_week: i16,
    pub day_of_week: String,
    pub meal_category: String,
}

/// `PUT /api/menus/:id/slots` — full-replace body.
#[derive(Debug, Deserialize)]
pub struct SetAllSlotsBody {
    pub slots: Vec<InsertMenuRecipeData>,
}

/// `POST /api/menus/:id/slots/clear` — delete one slot by coordinate.
/// Body is the same shape as `SlotCoordQuery`; keeping it in-body
/// avoids URL-encoding `meal_category` strings with spaces.
#[derive(Debug, Deserialize)]
pub struct ClearSlotBody {
    pub cycle_week: i16,
    pub day_of_week: String,
    pub meal_category: String,
}

// ---------------------------------------------------------------
// Handlers — menu reads
// ---------------------------------------------------------------

/// `GET /api/menus` — flat list, ordered `start_date DESC`.
pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<MenuRow>>, ApiError> {
    let opts: ListOpts = q.into();
    let rows = menus::list(&state.pool, &opts).await?;
    Ok(Json(rows))
}

/// `GET /api/menus/with-recipes` — flat list, each with nested slots.
pub async fn list_with_recipes(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<MenuWithRecipesRow>>, ApiError> {
    let opts: ListOpts = q.into();
    let rows = menus::list_with_recipes(&state.pool, &opts).await?;
    Ok(Json(rows))
}

/// `GET /api/menus/active-for-date?date=YYYY-MM-DD`. 404 when no active
/// menu covers the date.
pub async fn active_for_date(
    State(state): State<AppState>,
    Query(q): Query<ActiveForDateQuery>,
) -> Result<Json<MenuRow>, ApiError> {
    let row = menus::get_active_for_date(&state.pool, q.date)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `GET /api/menus/:id` — flat row, 404 on miss (or inactive without
/// `include_inactive`).
pub async fn get_by_id(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<GetByIdQuery>,
) -> Result<Json<MenuRow>, ApiError> {
    let row = menus::get_by_id(&state.pool, id, q.include_inactive)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `GET /api/menus/:id/with-recipes` — nested shape, 404 on miss.
pub async fn get_with_recipes(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<GetByIdQuery>,
) -> Result<Json<MenuWithRecipesRow>, ApiError> {
    let row = menus::get_with_recipes(&state.pool, id, q.include_inactive)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `GET /api/menus/:id/recipes` — just the slot array for a menu.
/// Returns `[]` when the menu exists but has no slots; the handler
/// does not pre-check menu existence (the SQL path doesn't either —
/// TS `getMenuRecipes` returns `[]` for a bogus `menuId`).
pub async fn get_menu_recipes(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<MenuRecipeWithNameRow>>, ApiError> {
    let rows = menus::get_menu_recipes(&state.pool, id).await?;
    Ok(Json(rows))
}

/// `GET /api/menus/:id/slots/by-coord?…` — single slot by coordinate.
/// 404 on miss.
pub async fn get_slot(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<SlotCoordQuery>,
) -> Result<Json<MenuRecipeWithNameRow>, ApiError> {
    let row = menus::get_slot(&state.pool, id, q.cycle_week, &q.day_of_week, &q.meal_category)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

// ---------------------------------------------------------------
// Handlers — menu writes (admin only)
// ---------------------------------------------------------------

/// `POST /api/menus` — create. 201 on success; 409 on duplicate name;
/// 400 on CHECK violation (end_date <= start_date).
pub async fn create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Json(body): Json<InsertMenuData>,
) -> Result<(StatusCode, Json<MenuRow>), ApiError> {
    scope_write(&auth)?;
    let row = menus::create(&state.pool, body).await.map_err(map_pg)?;
    Ok((StatusCode::CREATED, Json(row)))
}

/// `PATCH /api/menus/:id` — partial update. 404 on no match, 409 on
/// duplicate name, 400 on CHECK violation.
pub async fn update(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateMenuData>,
) -> Result<Json<MenuRow>, ApiError> {
    scope_write(&auth)?;
    let row = menus::update(&state.pool, id, body)
        .await
        .map_err(map_pg)?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `POST /api/menus/:id/deactivate` — soft delete.
pub async fn deactivate(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
) -> Result<Json<MenuRow>, ApiError> {
    scope_write(&auth)?;
    let row = menus::deactivate(&state.pool, id)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

// ---------------------------------------------------------------
// Handlers — slot writes (admin only)
// ---------------------------------------------------------------

/// `POST /api/menus/:id/slots` — upsert one slot. 200 on success with
/// the inserted/updated row. `:id` is informational; the body carries
/// the authoritative `menu_id` (keeps symmetry with the TS surface).
pub async fn set_slot(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(_id): Path<Uuid>,
    Json(body): Json<InsertMenuRecipeData>,
) -> Result<Json<MenuRecipeRow>, ApiError> {
    scope_write(&auth)?;
    let row = menus::set_slot(&state.pool, body).await.map_err(map_pg)?;
    Ok(Json(row))
}

/// `POST /api/menus/:id/slots/clear` — delete one slot by coordinate.
/// 200 on success with `{ "cleared": bool }`. The TS surface returns a
/// boolean directly; wrapping it in an object keeps the JSON shape
/// consistent for the dispatcher.
pub async fn clear_slot(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
    Json(body): Json<ClearSlotBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    scope_write(&auth)?;
    let cleared = menus::clear_slot(
        &state.pool,
        id,
        body.cycle_week,
        &body.day_of_week,
        &body.meal_category,
    )
    .await?;
    Ok(Json(serde_json::json!({ "cleared": cleared })))
}

/// `PUT /api/menus/:id/slots` — full-replace, atomic. 204 on success.
pub async fn set_all_slots(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
    Json(body): Json<SetAllSlotsBody>,
) -> Result<StatusCode, ApiError> {
    scope_write(&auth)?;
    menus::set_all_slots(&state.pool, id, &body.slots)
        .await
        .map_err(map_pg)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------
// Error mapping — translate Postgres codes to HTTP semantics.
//
// 23505 = unique_violation         → 409 Conflict (duplicate menu name)
// 23503 = foreign_key_violation    → 400 Validation (bad menu/recipe id)
// 23514 = check_violation          → 400 Validation (date range,
//                                     cycle_week, day_of_week enum)
// ---------------------------------------------------------------

fn map_pg(e: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(ref db_err) = e {
        if let Some(code) = db_err.code() {
            match code.as_ref() {
                "23505" => {
                    return ApiError::Domain(DomainError::Conflict(
                        "A menu with this name already exists".to_string(),
                    ))
                }
                "23503" | "23514" => {
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
