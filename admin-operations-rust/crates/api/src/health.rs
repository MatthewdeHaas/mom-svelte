//! `/healthz` — the single endpoint Railway and uptime checks hit.
//!
//! Returns 200 when we can round-trip a query against Postgres,
//! 503 otherwise. Timeout is tight (500ms) because a healthy pool
//! answers in single-digit milliseconds; anything slower is a signal
//! we should fail fast and let the platform cycle us.

use std::time::Duration;

use axum::{extract::State, http::StatusCode, Json};
use serde_json::json;

use crate::state::AppState;

pub async fn healthz(State(state): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    match db::ping(&state.pool, Duration::from_millis(500)).await {
        Ok(()) => (StatusCode::OK, Json(json!({ "status": "ok" }))),
        Err(e) => {
            tracing::warn!(error = %e, "healthz: db ping failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "degraded", "reason": "db" })),
            )
        }
    }
}
