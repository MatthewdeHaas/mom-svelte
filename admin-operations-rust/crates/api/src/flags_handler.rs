//! Feature-flag introspection endpoints.
//!
//! `GET  /api/flags`          — list all flags with their resolved value
//!                              for the caller's context.
//! `POST /api/flags/check`    — single flag check (`{"name":"insights"}`).
//! `GET  /api/flags/snapshot` — full catalog + raw rows + company list
//!                              in one shot, gated to a single operator
//!                              user id (TUI hydration — see
//!                              `docs/feature-flags.md`).
//!
//! These are a sanity surface for the SvelteKit side and the first
//! real users of the HMAC middleware + flag resolver.

use axum::{extract::State, http::StatusCode, Extension, Json};
use chrono::{DateTime, Utc};
use db::queries::companies;
use flags::{catalog, FlagDescriptor};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

// ---------------------------------------------------------------
// Snapshot — TUI hydration endpoint
//
// `GET /api/flags/snapshot` returns the full flag catalog + raw DB
// rows (`feature_flag` globals, `feature_flag_company` overrides) +
// the company list in one shot, so the local flags-TUI can render a
// flag × company grid without further round-trips.
//
// Gated two ways:
// 1. HMAC-signed internal auth (standard middleware).
// 2. `auth.user_id` must equal `state.flags_snapshot_user_id`.
//
// Behavior when the state slot is `None` (operator never set
// `FLAGS_SNAPSHOT_USER_ID` in the environment): 503 Service
// Unavailable. Fail-closed is a deliberate choice — the endpoint
// should not become reachable by accident.
//
// Read-only by construction. The only path to mutate flags is a
// migration file applied via the normal Railway redeploy chain.
// ---------------------------------------------------------------

/// A single `feature_flag` row (global default + metadata). Serialized
/// as owned strings so the JSON structure is independent of the
/// handler's static catalog.
#[derive(Debug, Serialize)]
pub struct FlagRow {
    pub name: String,
    pub description: String,
    pub enabled_global: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A single `feature_flag_company` row (per-company override).
#[derive(Debug, Serialize)]
pub struct OverrideRow {
    pub flag_name: String,
    pub company_id: Uuid,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A trimmed `company` row — just what the TUI needs to label a column.
/// Full `CompanyRow` carries contact info the TUI never shows.
#[derive(Debug, Serialize)]
pub struct SnapshotCompanyRow {
    pub id: Uuid,
    pub name: String,
    pub active: bool,
}

/// Everything the TUI hydrates on startup: compile-time catalog + live
/// DB state (globals + overrides) + the company list for column labels.
#[derive(Debug, Serialize)]
pub struct FlagSnapshot {
    pub catalog: Vec<FlagDescriptor>,
    pub flags: Vec<FlagRow>,
    pub overrides: Vec<OverrideRow>,
    pub companies: Vec<SnapshotCompanyRow>,
}

/// `GET /api/flags/snapshot` — see module-level doc for the gate.
pub async fn snapshot(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
) -> Result<Json<FlagSnapshot>, (StatusCode, &'static str)> {
    // Gate 1: env var must be set on the server.
    let gated_user = state
        .flags_snapshot_user_id
        .as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "snapshot gate not configured"))?;

    // Gate 2: caller's user id must match. Audit-log the attempt
    // either way — a 403 here is interesting even when it's benign.
    if auth.user_id.as_str() != gated_user.as_str() {
        tracing::warn!(
            caller_user_id = %auth.user_id,
            "flags_snapshot: user id does not match gate"
        );
        return Err((StatusCode::FORBIDDEN, "forbidden"));
    }

    // Success path audit — one line per snapshot pull, correlatable
    // with Railway log retention.
    tracing::info!(
        caller_user_id = %auth.user_id,
        "flags_snapshot: pulled"
    );

    let flags = sqlx::query_as!(
        FlagRow,
        r#"
        SELECT
            name            AS "name!",
            description     AS "description!",
            enabled_global  AS "enabled_global!",
            created_at      AS "created_at!: DateTime<Utc>",
            updated_at      AS "updated_at!: DateTime<Utc>"
        FROM feature_flag
        ORDER BY name ASC
        "#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "flags_snapshot: feature_flag query failed");
        (StatusCode::INTERNAL_SERVER_ERROR, "internal")
    })?;

    let overrides = sqlx::query_as!(
        OverrideRow,
        r#"
        SELECT
            flag_name   AS "flag_name!",
            company_id  AS "company_id!: Uuid",
            enabled     AS "enabled!",
            created_at  AS "created_at!: DateTime<Utc>",
            updated_at  AS "updated_at!: DateTime<Utc>"
        FROM feature_flag_company
        ORDER BY flag_name ASC, company_id ASC
        "#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "flags_snapshot: feature_flag_company query failed");
        (StatusCode::INTERNAL_SERVER_ERROR, "internal")
    })?;

    // Pull the company list via the existing `companies::list` so column
    // labels match what the admin UI shows (same ordering, same
    // active-filter default). Include inactive — the TUI should let
    // ops clean up overrides on a deactivated company.
    let company_rows = companies::list(
        &state.pool,
        &db::queries::companies::ListOpts {
            include_inactive: true,
            search: None,
        },
    )
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "flags_snapshot: companies::list failed");
        (StatusCode::INTERNAL_SERVER_ERROR, "internal")
    })?;

    let companies: Vec<SnapshotCompanyRow> = company_rows
        .into_iter()
        .map(|c| SnapshotCompanyRow {
            id: c.id,
            name: c.name,
            active: c.active,
        })
        .collect();

    Ok(Json(FlagSnapshot {
        catalog: catalog(),
        flags,
        overrides,
        companies,
    }))
}
