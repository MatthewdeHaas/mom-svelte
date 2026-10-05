//! `/api/recipes/*` HTTP handlers.
//!
//! Thin layer: parse → authorise → delegate to `db::queries::recipes`
//! → serialize. Business logic (COALESCE, three-state price_per_meal,
//! aggregation CTEs) lives in the db crate — this module owns the
//! HTTP contract and the admin-vs-client scoping.
//!
//! Scoping rules — recipes are global master data, so unlike orders
//! there is no row-level company scoping:
//! - `role == Admin` → unrestricted.
//! - `role == Client` → **reads only**. Every mutation (POST / PATCH /
//!   PUT / DELETE / deactivate) returns 403.
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
    pagination::{parse_from_query, Paginated},
    recipes::{
        self, IngredientLine, InsertRecipeData, ListOpts, RecipeIngredientRow, RecipeRow,
        RecipeWithIngredientsRow, UpdateRecipeData,
    },
};
use domain::{DomainError, Role};
use serde::Deserialize;
use uuid::Uuid;

use crate::{auth::AuthCtx, error::ApiError, state::AppState};

// ---------------------------------------------------------------
// Scope helper — recipes are read-free, write-admin-only.
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

/// `GET /api/recipes` / `/with-ingredients`: `includeInactive` + `search`.
#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub include_inactive: bool,
    #[serde(default)]
    pub search: Option<String>,
}

impl From<ListQuery> for ListOpts {
    fn from(q: ListQuery) -> Self {
        Self {
            include_inactive: q.include_inactive,
            search: q.search,
        }
    }
}

/// `GET /api/recipes/paginated` — flatten ListQuery with page/per_page.
#[derive(Debug, Deserialize)]
pub struct ListPaginatedQuery {
    #[serde(flatten)]
    pub opts: ListQuery,
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default, rename = "per_page")]
    pub per_page: Option<i64>,
}

/// `GET /api/recipes/:id` — `?include_inactive=true` to see soft-deleted.
#[derive(Debug, Default, Deserialize)]
pub struct GetByIdQuery {
    #[serde(default)]
    pub include_inactive: bool,
}

/// `GET /api/recipes/by-name?name=X&include_inactive=...`.
#[derive(Debug, Deserialize)]
pub struct GetByNameQuery {
    pub name: String,
    #[serde(default)]
    pub include_inactive: bool,
}

/// `GET /api/recipes/by-ids?ids=a,b,c` — comma-separated UUIDs.
#[derive(Debug, Deserialize)]
pub struct GetByIdsQuery {
    pub ids: String,
}

/// `POST /api/recipes/:id/ingredients/:ingredient_id` — body carries
/// amount + unit (ingredient_id is in the path).
#[derive(Debug, Deserialize)]
pub struct AddIngredientBody {
    pub amount: f64,
    pub unit: String,
}

/// `PUT /api/recipes/:id/ingredients` — full-replace. Keep the JSON
/// field name `ingredient` so the TS dispatcher can pass the exact
/// shape the SQL path accepts.
#[derive(Debug, Deserialize)]
pub struct SetIngredientsBody {
    pub ingredient: Vec<IngredientLine>,
}

// ---------------------------------------------------------------
// Handlers — reads
// ---------------------------------------------------------------

/// `GET /api/recipes` — flat list, no nested ingredients.
pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<RecipeRow>>, ApiError> {
    let opts: ListOpts = q.into();
    let rows = recipes::list(&state.pool, &opts).await?;
    Ok(Json(rows))
}

/// `GET /api/recipes/with-ingredients` — nested ingredient arrays.
pub async fn list_with_ingredients(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<RecipeWithIngredientsRow>>, ApiError> {
    let opts: ListOpts = q.into();
    let rows = recipes::list_with_ingredients(&state.pool, &opts).await?;
    Ok(Json(rows))
}

/// `GET /api/recipes/paginated` — same shape as `with_ingredients`
/// plus pagination + optional search.
pub async fn list_with_ingredients_paginated(
    State(state): State<AppState>,
    Query(q): Query<ListPaginatedQuery>,
) -> Result<Json<Paginated<RecipeWithIngredientsRow>>, ApiError> {
    let opts: ListOpts = q.opts.into();
    let pagination = parse_from_query(q.page, q.per_page, 50);
    let page = recipes::list_with_ingredients_paginated(&state.pool, &opts, pagination).await?;
    Ok(Json(page))
}

/// `GET /api/recipes/by-ids?ids=a,b,c` — returns a flat Vec; TS wraps
/// back into a `Map<id, RecipeWithIngredients>`.
pub async fn get_with_ingredients_by_ids(
    State(state): State<AppState>,
    Query(q): Query<GetByIdsQuery>,
) -> Result<Json<Vec<RecipeWithIngredientsRow>>, ApiError> {
    let ids: Vec<Uuid> = q
        .ids
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| {
            Uuid::parse_str(s.trim())
                .map_err(|_| ApiError::Domain(DomainError::Validation(format!("invalid uuid: {s}"))))
        })
        .collect::<Result<_, _>>()?;
    let rows = recipes::get_with_ingredients_by_ids(&state.pool, &ids).await?;
    Ok(Json(rows))
}

/// `GET /api/recipes/by-name` — case-insensitive exact match.
pub async fn get_by_name(
    State(state): State<AppState>,
    Query(q): Query<GetByNameQuery>,
) -> Result<Json<RecipeRow>, ApiError> {
    let row = recipes::get_by_name(&state.pool, &q.name, q.include_inactive)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `GET /api/recipes/:id` — flat row. 404 on no match.
pub async fn get_by_id(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<GetByIdQuery>,
) -> Result<Json<RecipeRow>, ApiError> {
    let row = recipes::get_by_id(&state.pool, id, q.include_inactive)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `GET /api/recipes/:id/with-ingredients` — nested shape, 404 on miss.
pub async fn get_with_ingredients(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<GetByIdQuery>,
) -> Result<Json<RecipeWithIngredientsRow>, ApiError> {
    let row = recipes::get_with_ingredients(&state.pool, id, q.include_inactive)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `GET /api/recipes/:id/ingredients` — just the ingredient array.
pub async fn get_ingredients(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<RecipeIngredientRow>>, ApiError> {
    let rows = recipes::get_ingredients(&state.pool, id).await?;
    Ok(Json(rows))
}

/// `GET /api/recipes/:id/active` — boolean, matches TS
/// `isRecipeActive` (false when the row doesn't exist).
pub async fn is_active(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let active = recipes::is_active(&state.pool, id).await?;
    Ok(Json(serde_json::json!({ "active": active })))
}

// ---------------------------------------------------------------
// Handlers — writes (admin only)
// ---------------------------------------------------------------

/// `POST /api/recipes` — create. 201 on success; 409 on duplicate name.
pub async fn create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Json(body): Json<InsertRecipeData>,
) -> Result<(StatusCode, Json<RecipeRow>), ApiError> {
    scope_write(&auth)?;
    let row = recipes::create(&state.pool, body).await.map_err(map_pg)?;
    Ok((StatusCode::CREATED, Json(row)))
}

/// `PATCH /api/recipes/:id` — partial update. 404 on no match, 409 on
/// duplicate name.
pub async fn update(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateRecipeData>,
) -> Result<Json<RecipeRow>, ApiError> {
    scope_write(&auth)?;
    let row = recipes::update(&state.pool, id, body)
        .await
        .map_err(map_pg)?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `POST /api/recipes/:id/deactivate` — soft delete.
pub async fn deactivate(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
) -> Result<Json<RecipeRow>, ApiError> {
    scope_write(&auth)?;
    let row = recipes::deactivate(&state.pool, id)
        .await?
        .ok_or(ApiError::Domain(DomainError::NotFound))?;
    Ok(Json(row))
}

/// `POST /api/recipes/:id/ingredients/:ingredient_id` — add or upsert.
pub async fn add_ingredient(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path((recipe_id, ingredient_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<AddIngredientBody>,
) -> Result<StatusCode, ApiError> {
    scope_write(&auth)?;
    recipes::add_ingredient(&state.pool, recipe_id, ingredient_id, body.amount, &body.unit)
        .await
        .map_err(map_pg)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/recipes/:id/ingredients/:ingredient_id` — remove.
pub async fn remove_ingredient(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path((recipe_id, ingredient_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    scope_write(&auth)?;
    recipes::remove_ingredient(&state.pool, recipe_id, ingredient_id)
        .await
        .map_err(map_pg)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `PUT /api/recipes/:id/ingredients` — full-replace, atomic.
pub async fn set_ingredients(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
    Json(body): Json<SetIngredientsBody>,
) -> Result<StatusCode, ApiError> {
    scope_write(&auth)?;
    recipes::set_ingredients(&state.pool, id, &body.ingredient)
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
                        "A recipe with this name already exists".to_string(),
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
