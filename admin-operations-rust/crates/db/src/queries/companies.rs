//! Minimal company queries used by the Excel importer. The TS
//! equivalent lives in `src/lib/server/queries/companies.ts`.

use sqlx::PgPool;
use uuid::Uuid;

/// Enough of a company to resolve an import row — the importer only
/// needs the id for the FK and `active` to reject orders against a
/// deactivated company.
#[derive(Debug, Clone)]
pub struct CompanyRef {
    pub id: Uuid,
    pub active: bool,
}

/// Case-insensitive exact-name lookup. Mirrors
/// `getCompanyByName(db, name)` on the TS side — `LOWER(name) = LOWER($1)`
/// so leading/trailing whitespace in the sheet is the caller's
/// problem, but case variance isn't.
pub async fn get_by_name(pool: &PgPool, name: &str) -> Result<Option<CompanyRef>, sqlx::Error> {
    let row: Option<(Uuid, bool)> =
        sqlx::query_as("SELECT id, active FROM company WHERE LOWER(name) = LOWER($1)")
            .bind(name)
            .fetch_optional(pool)
            .await?;

    Ok(row.map(|(id, active)| CompanyRef { id, active }))
}
