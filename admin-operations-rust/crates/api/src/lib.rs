//! Library surface for the admin-operations-rust API.
//!
//! Everything that integration tests need to spin up an in-process
//! server (router wiring, `AppState`, auth types) lives here.
//! `main.rs` is the thin binary that loads config, opens the pool,
//! runs migrations, and hands off to [`router`].
//!
//! Tests under `crates/api/tests/` depend on this crate — they build
//! their own `AppState` against an isolated `#[sqlx::test]` pool, then
//! call [`router`] to get a `tower`-compatible service they can bind
//! to an ephemeral `TcpListener`.

pub mod auth;
pub mod companies;
pub mod error;
pub mod flags_handler;
pub mod health;
pub mod import;
pub mod orders;
pub mod recipes;
pub mod state;

use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};

pub use state::AppState;

/// Router wiring. Public endpoints go on the base router; everything
/// under `/api` sits behind the HMAC auth middleware. Exposed so
/// integration tests can mount the exact same router the binary
/// serves in production — no bespoke test-only routes.
pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/flags", get(flags_handler::list))
        .route("/flags/check", post(flags_handler::check))
        .route("/import/preview", post(import::preview))
        .route("/import/commit", post(import::commit))
        // Orders surface — mirrors `src/lib/server/queries/orders.ts`.
        // Order matters: `/batch/status` must be registered before
        // `/:id` or axum tries to parse "batch" as a UUID.
        .route("/orders", get(orders::list).post(orders::create))
        .route("/orders/paginated", get(orders::list_paginated))
        .route("/orders/counts", get(orders::counts))
        .route("/orders/batch/status", post(orders::set_status_batch))
        .route(
            "/orders/:id",
            get(orders::get_by_id).patch(orders::update),
        )
        .route("/orders/:id/confirm", post(orders::confirm))
        .route("/orders/:id/fulfill", post(orders::fulfill))
        .route("/orders/:id/cancel", post(orders::cancel))
        // Recipes surface — mirrors `src/lib/server/queries/recipes.ts`.
        // Order matters: literal paths (`/paginated`, `/with-ingredients`,
        // `/by-name`, `/by-ids`) register before `/:id` or axum tries to
        // parse "by-name" as a UUID.
        .route("/recipes", get(recipes::list).post(recipes::create))
        .route(
            "/recipes/paginated",
            get(recipes::list_with_ingredients_paginated),
        )
        .route(
            "/recipes/with-ingredients",
            get(recipes::list_with_ingredients),
        )
        .route("/recipes/by-name", get(recipes::get_by_name))
        .route("/recipes/by-ids", get(recipes::get_with_ingredients_by_ids))
        .route(
            "/recipes/:id",
            get(recipes::get_by_id).patch(recipes::update),
        )
        .route(
            "/recipes/:id/with-ingredients",
            get(recipes::get_with_ingredients),
        )
        .route(
            "/recipes/:id/ingredients",
            get(recipes::get_ingredients).put(recipes::set_ingredients),
        )
        .route(
            "/recipes/:id/ingredients/:ingredient_id",
            post(recipes::add_ingredient).delete(recipes::remove_ingredient),
        )
        .route("/recipes/:id/deactivate", post(recipes::deactivate))
        .route("/recipes/:id/active", get(recipes::is_active))
        // Companies surface — mirrors `src/lib/server/queries/companies.ts`.
        // Order matters: literal paths (`/paginated`, `/by-name`) register
        // before `/:id` or axum tries to parse "by-name" as a UUID.
        .route("/companies", get(companies::list).post(companies::create))
        .route("/companies/paginated", get(companies::list_paginated))
        .route("/companies/by-name", get(companies::get_by_name))
        .route(
            "/companies/:id",
            get(companies::get_by_id).patch(companies::update),
        )
        .route(
            "/companies/:id/users",
            get(companies::get_users_by_company),
        )
        .route("/companies/:id/deactivate", post(companies::deactivate))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::verify_internal_auth,
        ));

    Router::new()
        .route("/healthz", get(health::healthz))
        .nest("/api", api)
        // Request-id in → traced → request-id out.
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(TraceLayer::new_for_http())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .with_state(state)
}
