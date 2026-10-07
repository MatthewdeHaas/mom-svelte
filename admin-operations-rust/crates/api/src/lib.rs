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

pub mod allergens;
pub mod auth;
pub mod companies;
pub mod error;
pub mod exports;
pub mod financials;
pub mod flags_handler;
pub mod health;
pub mod import;
pub mod ingredients;
pub mod insights;
pub mod invites;
pub mod menus;
pub mod orders;
pub mod persons;
pub mod recipe_imports;
pub mod recipes;
pub mod state;
pub mod users;

use axum::{
    middleware,
    routing::{get, post, put},
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
        // Recipe-imports surface — admin-only, multipart .xlsx upload.
        // Separate preview/commit split mirrors `/import/*` (orders).
        .route("/recipe-imports/preview", post(recipe_imports::preview))
        .route("/recipe-imports/commit", post(recipe_imports::commit))
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
        // Ingredients surface — mirrors `src/lib/server/queries/ingredients.ts`.
        // Order matters: literal paths (`/names`, `/paginated`,
        // `/with-allergens`) register before `/:id` or axum tries to
        // parse "names" as a UUID.
        .route(
            "/ingredients",
            get(ingredients::list).post(ingredients::create),
        )
        .route("/ingredients/names", get(ingredients::list_names))
        .route(
            "/ingredients/paginated",
            get(ingredients::list_with_allergens_paginated),
        )
        .route(
            "/ingredients/with-allergens",
            get(ingredients::list_with_allergens),
        )
        .route(
            "/ingredients/:id",
            get(ingredients::get_by_id).patch(ingredients::update),
        )
        .route(
            "/ingredients/:id/with-allergens",
            get(ingredients::get_with_allergens),
        )
        .route(
            "/ingredients/:id/deactivate",
            post(ingredients::deactivate),
        )
        .route(
            "/ingredients/:id/allergens",
            put(ingredients::set_allergens),
        )
        .route(
            "/ingredients/:id/allergens/:allergen_id",
            post(ingredients::add_allergen).delete(ingredients::remove_allergen),
        )
        // Persons + attendance surface — mirrors
        // `src/lib/server/queries/persons.ts`. Order matters: literal
        // `/with-allergens` registers before `/:id` or axum tries to
        // parse "with-allergens" as a UUID. The `/attendance/batch`
        // route must register before `/attendance` (same prefix). The
        // nested `/:id/allergens/:allergen_id` route is unambiguous
        // (both segments are UUIDs; literal `/allergens` separates them).
        .route("/persons", get(persons::list).post(persons::create))
        .route(
            "/persons/with-allergens",
            get(persons::list_with_allergens),
        )
        .route(
            "/persons/:id",
            get(persons::get_by_id).patch(persons::update),
        )
        .route(
            "/persons/:id/with-allergens",
            get(persons::get_with_allergens),
        )
        .route(
            "/persons/:id/allergens",
            get(persons::get_allergens).put(persons::set_allergens),
        )
        .route(
            "/persons/:id/allergens/:allergen_id",
            post(persons::add_allergen).delete(persons::remove_allergen),
        )
        .route("/persons/:id/deactivate", post(persons::deactivate))
        .route(
            "/attendance/batch",
            post(persons::upsert_attendance_batch),
        )
        .route(
            "/attendance",
            get(persons::get_attendance_by_date).post(persons::upsert_attendance),
        )
        // Allergens surface — mirrors `src/lib/server/queries/allergens.ts`.
        // Order matters: literal `/by-name` registers before `/:id` or axum
        // tries to parse "by-name" as a UUID. The `/:id/ref-count` route
        // is unambiguous (`:id` is a UUID segment, `ref-count` is literal).
        .route(
            "/allergens",
            get(allergens::list).post(allergens::create),
        )
        .route("/allergens/by-name", get(allergens::get_by_name))
        .route(
            "/allergens/:id",
            get(allergens::get_by_id)
                .patch(allergens::update)
                .delete(allergens::delete),
        )
        .route("/allergens/:id/ref-count", get(allergens::get_ref_count))
        // Invites surface — mirrors `src/lib/server/queries/invites.ts`.
        // Admin-only across the board (the unauthenticated
        // `/invite/[token]` SvelteKit route keeps the SQL path).
        // Order matters: `/by-token` registers before `/:id/*` so axum
        // doesn't try to parse "by-token" as a UUID.
        .route("/invites", get(invites::list).post(invites::create))
        .route("/invites/by-token", get(invites::get_by_token))
        .route("/invites/:id/mark-used", post(invites::mark_used))
        .route("/invites/:id/revoke", post(invites::revoke))
        // Financials surface — mirrors `src/lib/server/queries/financials.ts`.
        // Admin-only; three endpoints matching the three TS exports.
        // Order matters: `/recipe-margins/paginated` registers before
        // `/recipe-margins` would collide (same literal prefix) — axum
        // dispatches on full path, so both work, but the explicit
        // registration order keeps the surface easy to scan.
        .route(
            "/financials/recipe-margins",
            get(financials::recipe_margins),
        )
        .route(
            "/financials/recipe-margins/paginated",
            get(financials::recipe_margins_paginated),
        )
        .route("/financials/invoice", get(financials::invoice))
        // Exports surface — admin-only. Three endpoints that delegate to
        // the `excel::export` and `pdf::packing_slip` generators. The
        // xlsx handlers return raw bytes with an attachment disposition
        // header; the packing-slips handler returns a JSON array of
        // base64-encoded PDFs so the SvelteKit route can zip them.
        .route(
            "/exports/production-order",
            get(exports::production_order),
        )
        .route("/exports/shopping-list", get(exports::shopping_list))
        .route("/exports/packing-slips", get(exports::packing_slips))
        // Menus surface — mirrors `src/lib/server/queries/menus.ts`.
        // Order matters: literal paths (`/with-recipes`, `/active-for-date`)
        // register before `/:id` or axum tries to parse "with-recipes" as a
        // UUID. The `/:id/slots/clear` sub-route is distinct from
        // `/:id/slots` by HTTP shape (POST with body vs PUT); axum
        // dispatches on method, so order among these doesn't matter.
        .route("/menus", get(menus::list).post(menus::create))
        .route("/menus/with-recipes", get(menus::list_with_recipes))
        .route("/menus/active-for-date", get(menus::active_for_date))
        .route(
            "/menus/:id",
            get(menus::get_by_id).patch(menus::update),
        )
        .route(
            "/menus/:id/with-recipes",
            get(menus::get_with_recipes),
        )
        .route("/menus/:id/recipes", get(menus::get_menu_recipes))
        .route("/menus/:id/deactivate", post(menus::deactivate))
        .route(
            "/menus/:id/slots",
            post(menus::set_slot).put(menus::set_all_slots),
        )
        .route("/menus/:id/slots/clear", post(menus::clear_slot))
        .route(
            "/menus/:id/slots/by-coord",
            get(menus::get_slot),
        )
        // Insights surface — mirrors `src/lib/server/queries/insights.ts`.
        // Admin-only across the board; all 8 endpoints enforce
        // `require_admin` defensively (the /admin/insights SvelteKit
        // routes are already admin-gated upstream).
        .route("/insights/kpi", get(insights::kpi))
        .route("/insights/volume", get(insights::volume))
        .route("/insights/top-recipes", get(insights::top_recipes))
        .route("/insights/companies", get(insights::companies))
        .route("/insights/source-mix", get(insights::source_mix))
        .route("/insights/ingredients", get(insights::ingredients))
        .route("/insights/attendance", get(insights::attendance))
        .route(
            "/insights/attendance-by-company",
            get(insights::attendance_by_company),
        )
        // Users surface — no TS `queries/users.ts` counterpart; the
        // pre-port state had inline SQL in SvelteKit route handlers
        // (`admin/companies/[id]` and `invite/[token]`). Admin-gated
        // reads; `assign-company` allows a client to self-stamp when
        // unaffiliated (invite-acceptance flow — the handler enforces
        // this, not the router).
        //
        // Order matters: literal `/by-email` registers before `/:id`
        // or axum tries to parse "by-email" as the path param. The
        // `/:id/assign-company` sub-route is unambiguous (distinct
        // method + literal suffix).
        .route("/users/by-email", get(users::get_by_email))
        .route("/users/:id", get(users::get_by_id_in_company))
        .route("/users/:id/assign-company", post(users::assign_company))
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
