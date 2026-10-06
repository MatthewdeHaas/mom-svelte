//! Pure domain types shared across the Rust workspace.
//!
//! No I/O, no sqlx, no axum — only data definitions and the `Flag`
//! trait that the feature-flag registry builds on. This keeps the
//! downstream crates independent of DB and HTTP machinery.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// Excel-import wire types. Kept in the pure-types crate so the `excel`
// (business logic) and `api` (HTTP) crates can both name them without a
// shared heavy dependency.
pub mod import;
pub use import::{ImportPreview, ImportResult, ImportRowError, ImportWarning};

// Recipe-import wire types. Pure data; split from `excel::recipes` so
// `db::queries::recipe_imports` can name `RecipeExcelRowParsed` without
// requiring `db → excel` (that edge already runs the other way for the
// orders path and reversing it would introduce a cycle).
pub mod recipes;
pub use recipes::{
    FuzzyMatch, ParseRecipesResult, RecipeExcelRowParsed, RecipeImportPreview, RecipeImportResult,
};

// ---------------------------------------------------------------
// Roles
// ---------------------------------------------------------------

/// Mirrors the `role` column on `"user"` (migration 002).
///
/// SvelteKit sends this as a plain lowercase string in the
/// `X-Internal-Role` header. Any unrecognised value is treated as
/// `Client` — the least-privileged role — defensively.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Client,
}

impl Role {
    /// Case-insensitive parse. Unknown or empty → `Client`.
    pub fn from_header(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "admin" => Role::Admin,
            // "client" or anything else — default to least privilege.
            _ => Role::Client,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Client => "client",
        }
    }
}

// ---------------------------------------------------------------
// Feature-flag plumbing
// ---------------------------------------------------------------

/// Context passed to every flag check.
///
/// `company_id` is `None` for admin users whose scope is global, and
/// for the handful of flags evaluated outside a request (e.g. during
/// a cron job). `user_id` is a Better Auth string id (not a UUID) so
/// we store it as `String`; see migration 001.
#[derive(Debug, Clone)]
pub struct FlagContext {
    pub user_id: String,
    pub role: Role,
    pub company_id: Option<Uuid>,
}

/// A compile-time flag definition.
///
/// Implementors are zero-sized types in the `flags` crate. The
/// `is_enabled` default assumes the crate also wires in a `Resolver`
/// — kept as a trait so the lookup can be swapped in tests.
pub trait Flag {
    const NAME: &'static str;
    const DESCRIPTION: &'static str;
    const DEFAULT: bool = false;
}

/// Resolver abstraction so the `api` crate can inject a sqlx-backed
/// implementation and tests can inject a `HashMap` fake.
///
/// Returning `Option<bool>` lets the caller distinguish "flag not
/// present in the registry" (fall back to `Flag::DEFAULT`) from
/// "flag present and explicitly `false`".
#[async_trait::async_trait]
pub trait FlagResolver: Send + Sync {
    async fn resolve(&self, name: &str, ctx: &FlagContext) -> Option<bool>;
}

// re-export so downstream crates don't need to depend on `async_trait`
// just to name the trait.
pub use async_trait;

// ---------------------------------------------------------------
// Errors
// ---------------------------------------------------------------

/// Top-level error type used by handlers. HTTP crates convert this
/// into an `IntoResponse`; business-logic crates return it directly.
#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    #[error("not found")]
    NotFound,

    #[error("forbidden")]
    Forbidden,

    #[error("validation: {0}")]
    Validation(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("internal: {0}")]
    Internal(String),
}
