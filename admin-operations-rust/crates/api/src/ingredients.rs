//! `/api/ingredients/*` HTTP handlers.
//!
//! Thin layer: parse → authorise → delegate to `db::queries::ingredients`
//! → serialize. Business logic (COALESCE updates, three-state cost_per_unit,
//! ARRAY_AGG + manual zip for allergens) lives in the db crate — this
//! module owns the HTTP contract and the admin-only scoping.
//!
//! Scoping rule — ingredients are admin-only across the board. Unlike
//! companies (which "lie about existence" for cross-company reads),
//! every endpoint here returns 403 for a client caller: no client-facing
//! route in the SvelteKit app reads ingredients (verified during
//! planning). No "lie about existence" is required because no legitimate
//! client access exists to differentiate from.
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
    ingredients::{
        self, IngredientRow, IngredientWithAllergensRow, InsertIngredientData, ListOpts,
        UpdateIngredientData,
    },
    pagination::{parse_from_query, Paginated},
};
use domain::{DomainError, Role};
use serde::Deserialize;
use uuid::Uuid;

use crate::{auth::AuthCtx, error::ApiError, state::AppState};

// ---------------------------------------------------------------
// Scope helper — ingredients are admin-only, reads and writes alike.
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

/// `GET /api/ingredients` / `/with-ingredients` / `/with-allergens`:
/// `include_inactive` only.
#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
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

/// `GET /api/ingredients/paginated` — adds `search` + pagination.
#[derive(Debug, Deserialize)]
pub struct ListPaginatedQuery {
    #[serde(default)]
    pub include_inactive: bool,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default, rename = "per_page")]
    pub per_page: Option<i64>,
}

/// `GET /api/ingredients/:id` — `?include_inactive=true` to see soft-deleted.
#[derive(Debug, Default, Deserialize)]
pub struct GetByIdQuery {
    #[serde(default)]
    pub include_inactive: bool,
}

/// `PUT /api/ingredients/:id/allergens` — full-replace body.
#[derive(Debug, Deserialize)]
pub struct SetAllergensBody {
    pub allergen_ids: Vec<Uuid>,
}

// ---------------------------------------------------------------
// Handlers — reads (admin only)
// ---------------------------------------------------------------

/// `GET /api/ingredients` — flat list, no nested allergens.
pub async fn list(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<IngredientRow>>, ApiError> {
    require_admin(&auth)?;
    let opts: ListOpts = q.into();
    let rows = ingredients::list(&state.pool, &opts).await?;
    Ok(Json(rows))
}

/// `GET /api/ingredients/names` — just the names, for importer fuzzy
/// matching. No `include_inactive` — TS surface doesn't accept one
/// either, and the only caller is the importer which wants everything.
pub async fn list_names(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
) -> Result<Json<Vec<String>>, ApiError> {
    require_admin(&auth)?;
    let names = ingredients::list_names(&state.pool).await?;
    Ok(Json(names))
}

/// `GET /api/ingredients/with-allergens` — flat list, each with nested
/// allergen array.
pub async fn list_with_allergens(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<IngredientWithAllergensRow>>, ApiError> {
    require_admin(&auth)?;
    let opts: ListOpts = q.into();
    let rows = ingredients::list_with_allergens(&state.pool, &opts).await?;
    Ok(Json(rows))
}

/// `GET /api/ingredients/paginated` — paginated with optional search.
pub async fn list_with_allergens_paginated(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Query(q): Query<ListPaginatedQuery>,
) -> Result<Json<Paginated<IngredientWithAllergensRow>>, ApiError> {
    require_admin(&auth)?;
    let opts = ListOpts {
        include_inactive: q.include_inactive,
        search: q.search,
    };
    let pagination = parse_from_query(q.page, q.per_page, 50);
    let page =
        ingredients::list_with_allergens_paginated(&state.pool, &opts, pagination).await?;
    Ok(Json(page))
}

/// `GET /api/ingredients/:id` — flat row. 404 on miss or inactive
/// (unless `include_inactive`).
pub async fn get_by_id(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
    Query(q): Query<GetByIdQuery>,
) -> Result<Json<IngredientRow>, ApiError> {
    require_admin(&auth)?;
    let row = ingredients::get_by_id(&state.pool, id, q.include_inactive)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `GET /api/ingredients/:id/with-allergens` — nested shape, 404 on miss.
pub async fn get_with_allergens(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
    Query(q): Query<GetByIdQuery>,
) -> Result<Json<IngredientWithAllergensRow>, ApiError> {
    require_admin(&auth)?;
    let row = ingredients::get_with_allergens(&state.pool, id, q.include_inactive)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

// ---------------------------------------------------------------
// Handlers — writes (admin only)
// ---------------------------------------------------------------

/// `POST /api/ingredients` — create. 201 on success; 409 on duplicate name.
pub async fn create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Json(body): Json<InsertIngredientData>,
) -> Result<(StatusCode, Json<IngredientRow>), ApiError> {
    require_admin(&auth)?;
    let row = ingredients::create(&state.pool, body).await.map_err(map_pg)?;
    Ok((StatusCode::CREATED, Json(row)))
}

/// `PATCH /api/ingredients/:id` — partial update. 404 on no match,
/// 409 on duplicate name.
pub async fn update(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateIngredientData>,
) -> Result<Json<IngredientRow>, ApiError> {
    require_admin(&auth)?;
    let row = ingredients::update(&state.pool, id, body)
        .await
        .map_err(map_pg)?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `POST /api/ingredients/:id/deactivate` — soft delete.
pub async fn deactivate(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
) -> Result<Json<IngredientRow>, ApiError> {
    require_admin(&auth)?;
    let row = ingredients::deactivate(&state.pool, id)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `POST /api/ingredients/:id/allergens/:allergen_id` — attach one.
pub async fn add_allergen(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path((ingredient_id, allergen_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    require_admin(&auth)?;
    ingredients::add_allergen(&state.pool, ingredient_id, allergen_id)
        .await
        .map_err(map_pg)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/ingredients/:id/allergens/:allergen_id` — detach one.
pub async fn remove_allergen(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path((ingredient_id, allergen_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    require_admin(&auth)?;
    ingredients::remove_allergen(&state.pool, ingredient_id, allergen_id)
        .await
        .map_err(map_pg)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `PUT /api/ingredients/:id/allergens` — full-replace, atomic.
pub async fn set_allergens(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
    Json(body): Json<SetAllergensBody>,
) -> Result<StatusCode, ApiError> {
    require_admin(&auth)?;
    ingredients::set_allergens(&state.pool, id, &body.allergen_ids)
        .await
        .map_err(map_pg)?;
    Ok(StatusCode::NO_CONTENT)
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
                        "An ingredient with this name already exists".to_string(),
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
