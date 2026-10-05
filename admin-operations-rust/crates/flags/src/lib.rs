//! Feature-flag registry.
//!
//! A flag is a compile-time ZST implementing `domain::Flag`. Lookups go
//! through a `FlagResolver` injected by the API crate — concretely, a
//! sqlx-backed resolver reading from `feature_flag` +
//! `feature_flag_company` (migration 009).
//!
//! Precedence: per-company override (if present) > global `enabled_global`
//! > `Flag::DEFAULT`. The resolver returns `Option<bool>` so callers can
//! distinguish an explicit per-company `false` from "not registered yet".

use async_trait::async_trait;
use domain::{Flag, FlagContext, FlagResolver};
use serde::Serialize;
use sqlx::PgPool;

// ---------------------------------------------------------------
// Concrete flags — ZSTs
//
// Add new flags here; they're discovered by name at runtime via the
// `catalog()` function below so `/admin/flags` can list them.
// ---------------------------------------------------------------

pub struct Financials;
impl Flag for Financials {
    const NAME: &'static str = "financials";
    const DESCRIPTION: &'static str =
        "Enable the financials section (invoices, recipe margins) for a company.";
}

pub struct Insights;
impl Flag for Insights {
    const NAME: &'static str = "insights";
    const DESCRIPTION: &'static str = "Enable the insights dashboard (KPIs, charts).";
}

pub struct Allergens;
impl Flag for Allergens {
    const NAME: &'static str = "allergens";
    const DESCRIPTION: &'static str =
        "Expose the allergens admin UI and allergen columns on orders.";
}

pub struct ClientSelfOrdering;
impl Flag for ClientSelfOrdering {
    const NAME: &'static str = "client_self_ordering";
    const DESCRIPTION: &'static str =
        "Let client users place their own orders through the portal.";
}

pub struct MagicLinkAuth;
impl Flag for MagicLinkAuth {
    const NAME: &'static str = "magic_link_auth";
    const DESCRIPTION: &'static str =
        "Show the magic-link login option alongside password login.";
}

// ---------------------------------------------------------------
// Catalog — iterable list for the admin UI
// ---------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct FlagDescriptor {
    pub name: &'static str,
    pub description: &'static str,
    pub default: bool,
}

/// All flags known at compile time. Order is stable — the admin UI
/// renders them in this order.
pub fn catalog() -> Vec<FlagDescriptor> {
    fn d<F: Flag>() -> FlagDescriptor {
        FlagDescriptor {
            name: F::NAME,
            description: F::DESCRIPTION,
            default: F::DEFAULT,
        }
    }
    vec![
        d::<Financials>(),
        d::<Insights>(),
        d::<Allergens>(),
        d::<ClientSelfOrdering>(),
        d::<MagicLinkAuth>(),
    ]
}

// ---------------------------------------------------------------
// Sqlx resolver
// ---------------------------------------------------------------

/// Resolver that reads `feature_flag` + `feature_flag_company`.
///
/// Not cached — a single `SELECT ... WHERE name = $1` per check is
/// cheap and avoids staleness. If profiling shows this hot, slot a
/// `moka` cache in front of it; the interface won't change.
pub struct PgFlagResolver {
    pool: PgPool,
}

impl PgFlagResolver {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl FlagResolver for PgFlagResolver {
    async fn resolve(&self, name: &str, ctx: &FlagContext) -> Option<bool> {
        // Try per-company override first.
        if let Some(company_id) = ctx.company_id {
            let row: Option<(bool,)> = sqlx::query_as(
                "SELECT enabled FROM feature_flag_company \
                 WHERE flag_name = $1 AND company_id = $2",
            )
            .bind(name)
            .bind(company_id)
            .fetch_optional(&self.pool)
            .await
            .unwrap_or_else(|e| {
                tracing::error!(error = ?e, flag = name, "feature_flag_company lookup failed");
                None
            });
            if let Some((enabled,)) = row {
                return Some(enabled);
            }
        }

        // Fall back to global.
        let row: Option<(bool,)> = sqlx::query_as(
            "SELECT enabled_global FROM feature_flag WHERE name = $1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .unwrap_or_else(|e| {
            tracing::error!(error = ?e, flag = name, "feature_flag lookup failed");
            None
        });
        row.map(|(enabled,)| enabled)
    }
}

// ---------------------------------------------------------------
// Typed check helper
// ---------------------------------------------------------------

/// Typed convenience: `is_enabled::<Financials>(&resolver, &ctx).await`.
/// Falls back to `F::DEFAULT` when the registry row is missing.
pub async fn is_enabled<F: Flag>(resolver: &dyn FlagResolver, ctx: &FlagContext) -> bool {
    resolver.resolve(F::NAME, ctx).await.unwrap_or(F::DEFAULT)
}

/// Untyped lookup used by the admin UI (iterating the catalog) and by
/// the generic `/api/flags/check` endpoint. Returns `None` when the
/// flag isn't registered — callers decide the fallback (catalog
/// callers fall back to the catalog `default`, probing callers to
/// `false`).
pub async fn resolve_by_name(
    resolver: &dyn FlagResolver,
    name: &str,
    ctx: &FlagContext,
) -> Option<bool> {
    resolver.resolve(name, ctx).await
}
