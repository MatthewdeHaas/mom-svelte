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

// ---------------------------------------------------------------
// Drift — committed-vs-live mirror for the /admin/flags page
//
// `GET /api/flags/drift` parses the embedded migration SQL to
// reconstruct the "committed" state (what `scripts/migrations/`
// says the globals and overrides should be after a fresh apply),
// then reads the live rows from the DB. The SvelteKit side renders
// both tables side-by-side and highlights divergent rows.
//
// Why parse the migrations in Rust rather than Node: the standing
// directive is "business logic in Rust, TS is a byte pump". The
// migrations are the source of truth for flags, so the parser that
// turns them into state belongs on the Rust side.
//
// Scope — we match the three statement shapes the TUI emits (and
// that `009_feature_flags.sql` through `024_*` actually use):
//   - `INSERT INTO feature_flag (name, description, enabled_global)
//      VALUES (...) ON CONFLICT (name) DO NOTHING` — catalog seed
//   - `UPDATE feature_flag SET enabled_global = <bool> WHERE
//      name = '<flag>'` — global flip
//   - `INSERT INTO feature_flag_company (flag_name, company_id,
//      enabled) VALUES ('<flag>', '<uuid>', <bool>) ON CONFLICT
//      DO UPDATE` — per-company upsert
//   - `DELETE FROM feature_flag_company WHERE flag_name = '<flag>'
//      AND company_id = '<uuid>'` — per-company clear
//
// Anything else (schema DDL, DROP COLUMN cleanups, comments) is
// ignored — the parser is intentionally shallow. If someone writes
// a weirder migration by hand the drift page will show the live
// row with no committed counterpart and the operator will know to
// fix the migration.
// ---------------------------------------------------------------

/// One committed global flag row — what the migrations say the
/// catalog row should be after a full apply.
#[derive(Debug, Serialize)]
pub struct CommittedFlag {
    pub name: String,
    pub enabled_global: bool,
}

/// One committed per-company override row.
#[derive(Debug, Serialize)]
pub struct CommittedOverride {
    pub flag_name: String,
    pub company_id: Uuid,
    pub enabled: bool,
}

/// Live view — same shape as `CommittedOverride` but sourced from
/// the DB, so the UI can zip the two by `(flag_name, company_id)`.
#[derive(Debug, Serialize)]
pub struct LiveFlag {
    pub name: String,
    pub enabled_global: bool,
}

#[derive(Debug, Serialize)]
pub struct LiveOverride {
    pub flag_name: String,
    pub company_id: Uuid,
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct FlagDrift {
    pub committed_flags: Vec<CommittedFlag>,
    pub committed_overrides: Vec<CommittedOverride>,
    pub live_flags: Vec<LiveFlag>,
    pub live_overrides: Vec<LiveOverride>,
}

/// Walk the embedded migrator and fold every flag-mutating
/// statement into the final committed state. Returns `(flags,
/// overrides)` sorted deterministically so the UI diff is stable.
fn parse_committed_state() -> (Vec<CommittedFlag>, Vec<CommittedOverride>) {
    // Re-embed the same migrations the `db` crate runs. The macro
    // reads from `CARGO_MANIFEST_DIR/../../migrations` at compile
    // time — same path, same files (synced by
    // `scripts/sync-rust-migrations.sh` + Dockerfile copy).
    static MIGRATOR: db::sqlx::migrate::Migrator =
        db::sqlx::migrate!("./../../migrations");

    use std::collections::HashMap;

    // `(flag_name) -> enabled_global`. HashMap because later
    // migrations overwrite earlier ones (an UPDATE after an
    // INSERT ... DO NOTHING is the normal flip pattern).
    let mut flags: HashMap<String, bool> = HashMap::new();
    // `(flag_name, company_id) -> enabled`. A later DELETE removes
    // the entry entirely (not sets it to false).
    let mut overrides: HashMap<(String, Uuid), bool> = HashMap::new();

    for migration in MIGRATOR.iter() {
        apply_sql(&migration.sql, &mut flags, &mut overrides);
    }

    let mut flag_vec: Vec<CommittedFlag> = flags
        .into_iter()
        .map(|(name, enabled_global)| CommittedFlag {
            name,
            enabled_global,
        })
        .collect();
    flag_vec.sort_by(|a, b| a.name.cmp(&b.name));

    let mut override_vec: Vec<CommittedOverride> = overrides
        .into_iter()
        .map(|((flag_name, company_id), enabled)| CommittedOverride {
            flag_name,
            company_id,
            enabled,
        })
        .collect();
    override_vec.sort_by(|a, b| {
        a.flag_name
            .cmp(&b.flag_name)
            .then_with(|| a.company_id.cmp(&b.company_id))
    });

    (flag_vec, override_vec)
}

/// Match the four shapes documented above and update the running
/// state. Pure function — called once per migration file.
fn apply_sql(
    sql: &str,
    flags: &mut std::collections::HashMap<String, bool>,
    overrides: &mut std::collections::HashMap<(String, Uuid), bool>,
) {
    // Walk statement-by-statement so one migration can contain a
    // mix of INSERT/UPDATE/DELETE without the regex over-matching.
    // SQL `--` comments are naive-stripped (no multi-line /* */
    // handling — the migration files don't use those for flag
    // statements).
    let cleaned: String = sql
        .lines()
        .map(|l| l.split("--").next().unwrap_or("").trim_end())
        .collect::<Vec<_>>()
        .join("\n");

    for stmt in cleaned.split(';') {
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }
        let lower = stmt.to_ascii_lowercase();

        if lower.starts_with("insert into feature_flag ")
            || lower.starts_with("insert into feature_flag(")
        {
            if let Some(row) = parse_insert_flag(stmt) {
                flags.entry(row.0).or_insert(row.1);
            }
        } else if lower.starts_with("update feature_flag ")
            && !lower.starts_with("update feature_flag_company ")
        {
            if let Some(row) = parse_update_flag(stmt) {
                flags.insert(row.0, row.1);
            }
        } else if lower.starts_with("insert into feature_flag_company") {
            if let Some(row) = parse_insert_override(stmt) {
                overrides.insert((row.0, row.1), row.2);
            }
        } else if lower.starts_with("delete from feature_flag_company") {
            if let Some(row) = parse_delete_override(stmt) {
                overrides.remove(&(row.0, row.1));
            }
        }
    }
}

/// `INSERT INTO feature_flag (name, ..., enabled_global, ...)
///  VALUES ('<name>', ..., <bool>, ...) ON CONFLICT (name) DO NOTHING`
///
/// Only the `name` and `enabled_global` columns matter for drift.
/// Returns `(name, enabled_global)`. Loose about column order: the
/// seed migration lists `(name, description, enabled_global)` but
/// a future one could reorder.
fn parse_insert_flag(stmt: &str) -> Option<(String, bool)> {
    let (cols, vals) = split_insert(stmt)?;
    let name_idx = cols.iter().position(|c| c.eq_ignore_ascii_case("name"))?;
    let enabled_idx = cols
        .iter()
        .position(|c| c.eq_ignore_ascii_case("enabled_global"))?;
    let name = unquote(vals.get(name_idx)?)?;
    let enabled = parse_bool(vals.get(enabled_idx)?)?;
    Some((name, enabled))
}

/// `UPDATE feature_flag SET enabled_global = <bool> WHERE name = '<flag>'`
fn parse_update_flag(stmt: &str) -> Option<(String, bool)> {
    let lower = stmt.to_ascii_lowercase();
    // Pull the enabled_global value.
    let set_pos = lower.find("set ")?;
    let after_set = &stmt[set_pos + 4..];
    let eq_pos = after_set.find('=')?;
    let col = after_set[..eq_pos].trim();
    if !col.eq_ignore_ascii_case("enabled_global") {
        return None;
    }
    let rest = &after_set[eq_pos + 1..];
    // Grab the value token up to the first WHERE / , / end-of-stmt.
    let value_end = rest
        .to_ascii_lowercase()
        .find(" where ")
        .or_else(|| rest.find(','))
        .unwrap_or(rest.len());
    let value = rest[..value_end].trim();
    let enabled = parse_bool(value)?;

    // Pull the flag name from the WHERE clause.
    let where_pos = lower.find(" where ")?;
    let where_clause = &stmt[where_pos + 7..];
    let name = extract_eq_literal(where_clause, "name")?;
    Some((name, enabled))
}

/// `INSERT INTO feature_flag_company (flag_name, company_id, enabled)
///  VALUES ('<flag>', '<uuid>', <bool>) ON CONFLICT ... DO UPDATE ...`
fn parse_insert_override(stmt: &str) -> Option<(String, Uuid, bool)> {
    let (cols, vals) = split_insert(stmt)?;
    let flag_idx = cols
        .iter()
        .position(|c| c.eq_ignore_ascii_case("flag_name"))?;
    let company_idx = cols
        .iter()
        .position(|c| c.eq_ignore_ascii_case("company_id"))?;
    let enabled_idx = cols.iter().position(|c| c.eq_ignore_ascii_case("enabled"))?;
    let flag_name = unquote(vals.get(flag_idx)?)?;
    let company_id = unquote(vals.get(company_idx)?)?.parse().ok()?;
    let enabled = parse_bool(vals.get(enabled_idx)?)?;
    Some((flag_name, company_id, enabled))
}

/// `DELETE FROM feature_flag_company WHERE flag_name = '<flag>'
///  AND company_id = '<uuid>'`
fn parse_delete_override(stmt: &str) -> Option<(String, Uuid)> {
    let lower = stmt.to_ascii_lowercase();
    let where_pos = lower.find(" where ")?;
    let where_clause = &stmt[where_pos + 7..];
    let flag_name = extract_eq_literal(where_clause, "flag_name")?;
    let company_id = extract_eq_literal(where_clause, "company_id")?.parse().ok()?;
    Some((flag_name, company_id))
}

/// Split an INSERT statement into `(columns, values)` lists. Values
/// are returned as raw tokens (still quoted). Returns None if the
/// statement doesn't have the expected `(a, b) VALUES (x, y)` shape.
fn split_insert(stmt: &str) -> Option<(Vec<String>, Vec<String>)> {
    let open_cols = stmt.find('(')?;
    let close_cols = stmt[open_cols..].find(')')? + open_cols;
    let cols_src = &stmt[open_cols + 1..close_cols];
    let cols: Vec<String> = cols_src
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    let rest = &stmt[close_cols + 1..];
    let lower_rest = rest.to_ascii_lowercase();
    let values_pos = lower_rest.find("values")?;
    let after_values = &rest[values_pos + 6..];
    let open_vals = after_values.find('(')?;
    // Values list may contain commas inside quotes — walk char by
    // char tracking quote state.
    let after_open = &after_values[open_vals + 1..];
    let mut vals: Vec<String> = Vec::new();
    let mut buf = String::new();
    let mut in_quote = false;
    for c in after_open.chars() {
        if c == '\'' {
            in_quote = !in_quote;
            buf.push(c);
        } else if c == ',' && !in_quote {
            vals.push(buf.trim().to_string());
            buf.clear();
        } else if c == ')' && !in_quote {
            vals.push(buf.trim().to_string());
            return Some((cols, vals));
        } else {
            buf.push(c);
        }
    }
    None
}

/// Strip surrounding single quotes and un-double embedded ones.
/// Returns None for non-string tokens.
fn unquote(s: &str) -> Option<String> {
    let s = s.trim();
    if s.len() < 2 || !s.starts_with('\'') || !s.ends_with('\'') {
        return None;
    }
    Some(s[1..s.len() - 1].replace("''", "'"))
}

/// Parse a Postgres bool literal: `true`, `false`, `TRUE`, `FALSE`.
fn parse_bool(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "'t'" | "'true'" => Some(true),
        "false" | "'f'" | "'false'" => Some(false),
        _ => None,
    }
}

/// Find `<col> = '<literal>'` inside a WHERE clause and return the
/// unquoted literal. Case-insensitive on the column name. Stops at
/// the first match — a WHERE with multiple `<col> =` entries would
/// need a smarter parser, but the migrations never do that.
fn extract_eq_literal(where_clause: &str, col: &str) -> Option<String> {
    let lower = where_clause.to_ascii_lowercase();
    let needle = format!("{} =", col.to_ascii_lowercase());
    let pos = lower.find(&needle).or_else(|| {
        // Also match `col=` with no spaces.
        let tight = format!("{}=", col.to_ascii_lowercase());
        lower.find(&tight)
    })?;
    // Advance past the `=` sign.
    let after_eq = where_clause[pos..].find('=')? + pos + 1;
    let rest = where_clause[after_eq..].trim_start();
    // Grab the quoted literal.
    if !rest.starts_with('\'') {
        return None;
    }
    let close = rest[1..].find('\'')? + 1;
    Some(rest[1..close].replace("''", "'"))
}

/// `GET /api/flags/drift` — same gate as `/flags/snapshot`.
pub async fn drift(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthCtx>,
) -> Result<Json<FlagDrift>, (StatusCode, &'static str)> {
    let gated_user = state
        .flags_snapshot_user_id
        .as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "drift gate not configured"))?;

    if auth.user_id.as_str() != gated_user.as_str() {
        tracing::warn!(
            caller_user_id = %auth.user_id,
            "flags_drift: user id does not match gate"
        );
        return Err((StatusCode::FORBIDDEN, "forbidden"));
    }

    let (committed_flags, committed_overrides) = parse_committed_state();

    let live_flags = sqlx::query_as!(
        LiveFlag,
        r#"
        SELECT
            name            AS "name!",
            enabled_global  AS "enabled_global!"
        FROM feature_flag
        ORDER BY name ASC
        "#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "flags_drift: feature_flag query failed");
        (StatusCode::INTERNAL_SERVER_ERROR, "internal")
    })?;

    let live_overrides = sqlx::query_as!(
        LiveOverride,
        r#"
        SELECT
            flag_name   AS "flag_name!",
            company_id  AS "company_id!: Uuid",
            enabled     AS "enabled!"
        FROM feature_flag_company
        ORDER BY flag_name ASC, company_id ASC
        "#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "flags_drift: feature_flag_company query failed");
        (StatusCode::INTERNAL_SERVER_ERROR, "internal")
    })?;

    Ok(Json(FlagDrift {
        committed_flags,
        committed_overrides,
        live_flags,
        live_overrides,
    }))
}
