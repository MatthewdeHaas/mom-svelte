//! Feature-flag introspection endpoints.
//!
//! `GET /api/flags`        — list all flags with their resolved value
//!                           for the caller's context.
//! `POST /api/flags/check` — single flag check (`{"name":"insights"}`).
//!
//! These are a sanity surface for the SvelteKit side and the first
//! real users of the HMAC middleware + flag resolver.

use axum::{extract::State, Extension, Json};
use flags::catalog;
use serde::{Deserialize, Serialize};

use crate::{auth::AuthCtx, error::ApiError, state::AppState};

#[derive(Serialize)]
pub struct FlagView {
    pub name: &'static str,
    pub description: &'static str,
    pub default: bool,
    pub enabled: bool,
}

pub async fn list(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
) -> Result<Json<Vec<FlagView>>, ApiError> {
    let ctx = auth.to_flag_context();
    let resolver = state.flags.as_ref();

    let mut out = Vec::new();
    for d in catalog() {
        // `resolve` returns None when no row exists; fall back to the
        // compile-time default so the API never pretends a flag is
        // missing — just reports its default state.
        let enabled = match flags::resolve_by_name(resolver, d.name, &ctx).await {
            Some(v) => v,
            None => d.default,
        };
        out.push(FlagView {
            name: d.name,
            description: d.description,
            default: d.default,
            enabled,
        });
    }
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct CheckReq {
    pub name: String,
}

#[derive(Serialize)]
pub struct CheckResp {
    pub name: String,
    pub enabled: bool,
}

pub async fn check(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
    Json(req): Json<CheckReq>,
) -> Result<Json<CheckResp>, ApiError> {
    let ctx = auth.to_flag_context();
    let resolver = state.flags.as_ref();

    // Unknown flag → false. We don't 404 so callers can safely probe
    // flags that haven't shipped yet without special-casing.
    let enabled = flags::resolve_by_name(resolver, &req.name, &ctx)
        .await
        .unwrap_or(false);

    Ok(Json(CheckResp {
        name: req.name,
        enabled,
    }))
}
