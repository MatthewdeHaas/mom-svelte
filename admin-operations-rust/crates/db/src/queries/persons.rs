//! Person + attendance queries. TS counterpart is
//! `src/lib/server/queries/persons.ts` (288 lines, 13 exports). Persons
//! are per-company master data (`person.company_id` FK) and own
//! allergen associations via `person_allergen`. Attendance is a
//! per-person-per-date boolean (`attendance` with
//! `UNIQUE(person_id, serving_date)`).
//!
//! No `::float8` casts, no three-state fields. Every static query is
//! compile-time `query_as!`-checked; `set_allergens` and
//! `upsert_attendance_batch` use `QueryBuilder` for multi-row inserts.
//!
//! Scoping is NOT enforced here — this module is a thin DB layer that
//! takes `company_id` as a parameter. The HTTP handler layer gates
//! admin-vs-client access and enforces `person.company_id == auth.company_id`
//! for client callers.
//!
//! Deactivate is a soft delete (set `active = false`) — attendance
//! history and allergen rows are preserved. The TS surface mirrors this.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgExecutor, PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

// =============================================================
// Public row types — mirror `src/lib/types.ts`.
// =============================================================

/// Base person row. Shape matches TS `Person` interface:
/// `{ id, company_id, name, active }`.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PersonRow {
    pub id: Uuid,
    pub company_id: Uuid,
    pub name: String,
    pub active: bool,
}

/// Allergen embedded under a `PersonWithAllergensRow` / nested
/// `AttendanceWithPersonRow`. Shape matches `ingredients::AllergenEmbed`
/// and the TS `Allergen` interface — kept as a separate local type per
/// the ingredients/companies precedent (avoid cross-module coupling even
/// when shapes coincide).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllergenEmbed {
    pub id: Uuid,
    pub name: String,
}

/// Person + nested allergen array. Matches TS `PersonWithAllergens`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonWithAllergensRow {
    pub id: Uuid,
    pub company_id: Uuid,
    pub name: String,
    pub active: bool,
    pub allergen: Vec<AllergenEmbed>,
}

/// Attendance row. Shape matches TS `Attendance`:
/// `{ id, person_id, serving_date, attending }`.
///
/// `serving_date` is a `DATE` on the pg side → `NaiveDate` here. The
/// TS type is `Date | string` which pg surfaces as a `Date`; both wire
/// paths (TS SQL and Rust) emit ISO `YYYY-MM-DD` in JSON, so the
/// decoder on the TS side sees the same shape either way.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AttendanceRow {
    pub id: Uuid,
    pub person_id: Uuid,
    pub serving_date: NaiveDate,
    pub attending: bool,
}

/// Attendance + embedded person (with allergens). Matches TS
/// `AttendanceWithPerson`. The embedded person's `company_id` always
/// equals the `company_id` filter passed to the query — mirrors the
/// TS layering at `persons.ts:239`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttendanceWithPersonRow {
    pub id: Uuid,
    pub person_id: Uuid,
    pub serving_date: NaiveDate,
    pub attending: bool,
    pub person: PersonWithAllergensRow,
}

// =============================================================
// Input types for mutating endpoints.
// =============================================================

/// Filters shared by every list endpoint. No `search` — the TS surface
/// doesn't expose one for persons.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListOpts {
    #[serde(default)]
    pub include_inactive: bool,
}

/// Input to `create`. Matches TS `InsertPerson`: `company_id` + `name`
/// required, `active` defaults to `true` server-side.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertPersonData {
    pub company_id: Uuid,
    pub name: String,
    #[serde(default)]
    pub active: Option<bool>,
}

/// Input to `update`. All fields optional. `None` → leave unchanged
/// (COALESCE). The TS surface uses `?? null` + COALESCE, which collapses
/// "clear" to "leave as-is" — we mirror that exactly.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdatePersonData {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
}

/// Input to `upsert_attendance` / `upsert_attendance_batch`. Matches TS
/// `UpsertAttendance`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertAttendanceData {
    pub person_id: Uuid,
    pub serving_date: NaiveDate,
    pub attending: bool,
}

// =============================================================
// Reads — flat shape.
// =============================================================

/// `getPersonsByCompany` — ordered by name ASC. Two static branches so
/// `query_as!` can type-check both.
pub async fn list_by_company(
    pool: &PgPool,
    company_id: Uuid,
    opts: &ListOpts,
) -> Result<Vec<PersonRow>, sqlx::Error> {
    if opts.include_inactive {
        sqlx::query_as!(
            PersonRow,
            r#"
            SELECT
                id         AS "id!: Uuid",
                company_id AS "company_id!: Uuid",
                name       AS "name!",
                active     AS "active!"
            FROM person
            WHERE company_id = $1
            ORDER BY name ASC
            "#,
            company_id,
        )
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as!(
            PersonRow,
            r#"
            SELECT
                id         AS "id!: Uuid",
                company_id AS "company_id!: Uuid",
                name       AS "name!",
                active     AS "active!"
            FROM person
            WHERE company_id = $1 AND active = TRUE
            ORDER BY name ASC
            "#,
            company_id,
        )
        .fetch_all(pool)
        .await
    }
}

/// `getPersonById` — returns `None` if the row doesn't exist OR is
/// inactive (unless `include_inactive`). Matches TS semantics at
/// `persons.ts:34-47`.
pub async fn get_by_id(
    pool: &PgPool,
    id: Uuid,
    include_inactive: bool,
) -> Result<Option<PersonRow>, sqlx::Error> {
    if include_inactive {
        sqlx::query_as!(
            PersonRow,
            r#"
            SELECT
                id         AS "id!: Uuid",
                company_id AS "company_id!: Uuid",
                name       AS "name!",
                active     AS "active!"
            FROM person
            WHERE id = $1
            "#,
            id,
        )
        .fetch_optional(pool)
        .await
    } else {
        sqlx::query_as!(
            PersonRow,
            r#"
            SELECT
                id         AS "id!: Uuid",
                company_id AS "company_id!: Uuid",
                name       AS "name!",
                active     AS "active!"
            FROM person
            WHERE id = $1 AND active = TRUE
            "#,
            id,
        )
        .fetch_optional(pool)
        .await
    }
}

/// `getPersonAllergens` — allergens attached to a person, ordered by
/// name ASC. Returns `[]` for a person with no allergens.
pub async fn get_allergens(
    pool: &PgPool,
    person_id: Uuid,
) -> Result<Vec<AllergenEmbed>, sqlx::Error> {
    sqlx::query_as!(
        AllergenEmbed,
        r#"
        SELECT a.id AS "id!: Uuid", a.name AS "name!"
        FROM allergen a
        JOIN person_allergen pa ON pa.allergen_id = a.id
        WHERE pa.person_id = $1
        ORDER BY a.name ASC
        "#,
        person_id,
    )
    .fetch_all(pool)
    .await
}

// =============================================================
// Reads — nested shape (with allergens).
// =============================================================

/// `getPersonWithAllergens` — single person with their allergens.
/// Layered call: fetch person first, then its allergens via a static
/// `query_as!`. Mirrors TS layering at `persons.ts:49-59`.
pub async fn get_with_allergens(
    pool: &PgPool,
    id: Uuid,
    include_inactive: bool,
) -> Result<Option<PersonWithAllergensRow>, sqlx::Error> {
    let Some(row) = get_by_id(pool, id, include_inactive).await? else {
        return Ok(None);
    };
    let allergen = get_allergens(pool, id).await?;
    Ok(Some(PersonWithAllergensRow {
        id: row.id,
        company_id: row.company_id,
        name: row.name,
        active: row.active,
        allergen,
    }))
}

/// `getPersonsWithAllergensByCompany` — flat list for a company, each
/// row with its nested allergen array. ARRAY_AGG + FILTER + manual zip,
/// same pattern as `ingredients::list_with_allergens`.
pub async fn list_with_allergens_by_company(
    pool: &PgPool,
    company_id: Uuid,
    opts: &ListOpts,
) -> Result<Vec<PersonWithAllergensRow>, sqlx::Error> {
    // Two branches so query! type-checks both. The ARRAY_AGG columns
    // are Nullable (FILTER + no matching rows → NULL) — we
    // `.unwrap_or_default()` to match TS's `?? []`.
    if opts.include_inactive {
        let rows = sqlx::query!(
            r#"
            SELECT
                p.id         AS "id!: Uuid",
                p.company_id AS "company_id!: Uuid",
                p.name       AS "name!",
                p.active     AS "active!",
                ARRAY_AGG(a.id   ORDER BY a.name) FILTER (WHERE a.id   IS NOT NULL) AS "allergen_ids: Vec<Uuid>",
                ARRAY_AGG(a.name ORDER BY a.name) FILTER (WHERE a.name IS NOT NULL) AS "allergen_names: Vec<String>"
            FROM person p
            LEFT JOIN person_allergen pa ON pa.person_id = p.id
            LEFT JOIN allergen a ON a.id = pa.allergen_id
            WHERE p.company_id = $1
            GROUP BY p.id
            ORDER BY p.name ASC
            "#,
            company_id,
        )
        .fetch_all(pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let ids = r.allergen_ids.unwrap_or_default();
                let names = r.allergen_names.unwrap_or_default();
                let allergen = ids
                    .into_iter()
                    .zip(names)
                    .map(|(id, name)| AllergenEmbed { id, name })
                    .collect();
                PersonWithAllergensRow {
                    id: r.id,
                    company_id: r.company_id,
                    name: r.name,
                    active: r.active,
                    allergen,
                }
            })
            .collect())
    } else {
        let rows = sqlx::query!(
            r#"
            SELECT
                p.id         AS "id!: Uuid",
                p.company_id AS "company_id!: Uuid",
                p.name       AS "name!",
                p.active     AS "active!",
                ARRAY_AGG(a.id   ORDER BY a.name) FILTER (WHERE a.id   IS NOT NULL) AS "allergen_ids: Vec<Uuid>",
                ARRAY_AGG(a.name ORDER BY a.name) FILTER (WHERE a.name IS NOT NULL) AS "allergen_names: Vec<String>"
            FROM person p
            LEFT JOIN person_allergen pa ON pa.person_id = p.id
            LEFT JOIN allergen a ON a.id = pa.allergen_id
            WHERE p.company_id = $1 AND p.active = TRUE
            GROUP BY p.id
            ORDER BY p.name ASC
            "#,
            company_id,
        )
        .fetch_all(pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let ids = r.allergen_ids.unwrap_or_default();
                let names = r.allergen_names.unwrap_or_default();
                let allergen = ids
                    .into_iter()
                    .zip(names)
                    .map(|(id, name)| AllergenEmbed { id, name })
                    .collect();
                PersonWithAllergensRow {
                    id: r.id,
                    company_id: r.company_id,
                    name: r.name,
                    active: r.active,
                    allergen,
                }
            })
            .collect())
    }
}

// =============================================================
// Writes — persons. No UNIQUE constraint on person(name) — a company
// may have two "John Smith" entries (siblings, father/son, whatever) —
// so no 23505 translation needed for persons. The 23503 case (company
// not found) IS still possible, mapped at the handler layer.
// =============================================================

/// `createPerson`. Trims name server-side (matches TS
/// `data.name.trim()` at `persons.ts:119`). Default: active = true.
pub async fn create(pool: &PgPool, data: InsertPersonData) -> Result<PersonRow, sqlx::Error> {
    let name = data.name.trim();
    let active = data.active.unwrap_or(true);
    sqlx::query_as!(
        PersonRow,
        r#"
        INSERT INTO person (company_id, name, active)
        VALUES ($1, $2, $3)
        RETURNING
            id         AS "id!: Uuid",
            company_id AS "company_id!: Uuid",
            name       AS "name!",
            active     AS "active!"
        "#,
        data.company_id,
        name,
        active,
    )
    .fetch_one(pool)
    .await
}

/// `updatePerson`. COALESCE on every field — `None` leaves the current
/// value in place. Trims `name` if present. Returns `None` if no row
/// matches `id`.
pub async fn update(
    pool: &PgPool,
    id: Uuid,
    data: UpdatePersonData,
) -> Result<Option<PersonRow>, sqlx::Error> {
    let name = data.name.as_deref().map(str::trim);
    sqlx::query_as!(
        PersonRow,
        r#"
        UPDATE person
        SET
            name   = COALESCE($1, name),
            active = COALESCE($2, active)
        WHERE id = $3
        RETURNING
            id         AS "id!: Uuid",
            company_id AS "company_id!: Uuid",
            name       AS "name!",
            active     AS "active!"
        "#,
        name,
        data.active,
        id,
    )
    .fetch_optional(pool)
    .await
}

/// `deactivatePerson`. Soft delete — attendance history + allergen
/// rows are preserved. Mirrors the TS one-liner at `persons.ts:142`.
pub async fn deactivate(pool: &PgPool, id: Uuid) -> Result<Option<PersonRow>, sqlx::Error> {
    update(
        pool,
        id,
        UpdatePersonData {
            active: Some(false),
            ..Default::default()
        },
    )
    .await
}

// =============================================================
// Allergen associations. Generic over any `PgExecutor` so a caller
// could share a transaction — matches the TS `Pool | PoolClient`
// surface. No in-repo caller currently passes a `PoolClient` to
// these three, but the shape is cheap to preserve.
// =============================================================

/// `addPersonAllergen` — idempotent insert into the join table.
pub async fn add_allergen<'c, E>(
    executor: E,
    person_id: Uuid,
    allergen_id: Uuid,
) -> Result<(), sqlx::Error>
where
    E: PgExecutor<'c>,
{
    sqlx::query!(
        r#"
        INSERT INTO person_allergen (person_id, allergen_id)
        VALUES ($1, $2)
        ON CONFLICT DO NOTHING
        "#,
        person_id,
        allergen_id,
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// `removePersonAllergen` — plain DELETE, no-op if the row isn't there.
pub async fn remove_allergen<'c, E>(
    executor: E,
    person_id: Uuid,
    allergen_id: Uuid,
) -> Result<(), sqlx::Error>
where
    E: PgExecutor<'c>,
{
    sqlx::query!(
        r#"
        DELETE FROM person_allergen
        WHERE person_id = $1 AND allergen_id = $2
        "#,
        person_id,
        allergen_id,
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// `setPersonAllergens` — atomic full-replace. DELETE + bulk INSERT in
/// one transaction. The TS surface accepts `Pool | PoolClient`; here
/// we always own the transaction from a pool. No in-repo caller passes
/// a `PoolClient` (grep-verified), so this is sufficient.
pub async fn set_allergens(
    pool: &PgPool,
    person_id: Uuid,
    allergen_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query!(
        r#"DELETE FROM person_allergen WHERE person_id = $1"#,
        person_id,
    )
    .execute(&mut *tx)
    .await?;

    if !allergen_ids.is_empty() {
        let mut qb: QueryBuilder<Postgres> =
            QueryBuilder::new("INSERT INTO person_allergen (person_id, allergen_id) ");
        qb.push_values(allergen_ids.iter(), |mut b, aid| {
            b.push_bind(person_id).push_bind(aid);
        });
        qb.build().execute(&mut *tx).await?;
    }

    tx.commit().await?;
    Ok(())
}

// =============================================================
// Attendance.
// =============================================================

/// `getAttendanceByDate` — attendance for a company on a specific
/// serving date, with embedded person (incl. allergens). ARRAY_AGG +
/// FILTER on the allergen join, manual zip for the nested shape.
///
/// Rows are ordered by person name ASC — matches TS semantics at
/// `persons.ts:228`.
pub async fn get_attendance_by_date(
    pool: &PgPool,
    company_id: Uuid,
    serving_date: NaiveDate,
) -> Result<Vec<AttendanceWithPersonRow>, sqlx::Error> {
    // Named row struct so the macro's row type isn't anonymous —
    // rustc 1.99.0's never-type fallback (issue #156047) otherwise
    // collapses the inferred type to `!` across the downstream chain.
    struct AttendanceAggRow {
        id: Uuid,
        person_id: Uuid,
        serving_date: NaiveDate,
        attending: bool,
        person_name: String,
        person_active: bool,
        allergen_ids: Option<Vec<Uuid>>,
        allergen_names: Option<Vec<String>>,
    }
    let rows: Vec<AttendanceAggRow> = sqlx::query_as!(
        AttendanceAggRow,
        r#"
        SELECT
            att.id            AS "id!: Uuid",
            att.person_id     AS "person_id!: Uuid",
            att.serving_date  AS "serving_date!: NaiveDate",
            att.attending     AS "attending!",
            p.name            AS "person_name!",
            p.active          AS "person_active!",
            ARRAY_AGG(a.id   ORDER BY a.name) FILTER (WHERE a.id   IS NOT NULL) AS "allergen_ids: Vec<Uuid>",
            ARRAY_AGG(a.name ORDER BY a.name) FILTER (WHERE a.name IS NOT NULL) AS "allergen_names: Vec<String>"
        FROM attendance att
        JOIN person p ON p.id = att.person_id
        LEFT JOIN person_allergen pa ON pa.person_id = p.id
        LEFT JOIN allergen a ON a.id = pa.allergen_id
        WHERE p.company_id    = $1
          AND att.serving_date = $2
        GROUP BY att.id, p.id
        ORDER BY p.name ASC
        "#,
        company_id,
        serving_date,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let ids = r.allergen_ids.unwrap_or_default();
            let names = r.allergen_names.unwrap_or_default();
            let allergen: Vec<AllergenEmbed> = ids
                .into_iter()
                .zip(names)
                .map(|(id, name)| AllergenEmbed { id, name })
                .collect();
            AttendanceWithPersonRow {
                id: r.id,
                person_id: r.person_id,
                serving_date: r.serving_date,
                attending: r.attending,
                person: PersonWithAllergensRow {
                    id: r.person_id,
                    company_id,
                    name: r.person_name,
                    active: r.person_active,
                    allergen,
                },
            }
        })
        .collect())
}

/// `upsertAttendance` — single-row upsert on the
/// `(person_id, serving_date)` unique constraint. Toggling attendance
/// just updates the existing row.
pub async fn upsert_attendance(
    pool: &PgPool,
    data: UpsertAttendanceData,
) -> Result<AttendanceRow, sqlx::Error> {
    sqlx::query_as!(
        AttendanceRow,
        r#"
        INSERT INTO attendance (person_id, serving_date, attending)
        VALUES ($1, $2, $3)
        ON CONFLICT (person_id, serving_date)
        DO UPDATE SET attending = EXCLUDED.attending
        RETURNING
            id           AS "id!: Uuid",
            person_id    AS "person_id!: Uuid",
            serving_date AS "serving_date!: NaiveDate",
            attending    AS "attending!"
        "#,
        data.person_id,
        data.serving_date,
        data.attending,
    )
    .fetch_one(pool)
    .await
}

/// `upsertAttendanceBatch` — bulk upsert. Builds a multi-row VALUES
/// list via `QueryBuilder::push_values` and relies on the UNIQUE
/// constraint for the ON CONFLICT clause. No-op on empty input to
/// match TS at `persons.ts:272`.
pub async fn upsert_attendance_batch(
    pool: &PgPool,
    records: &[UpsertAttendanceData],
) -> Result<(), sqlx::Error> {
    if records.is_empty() {
        return Ok(());
    }

    let mut qb: QueryBuilder<Postgres> =
        QueryBuilder::new("INSERT INTO attendance (person_id, serving_date, attending) ");
    qb.push_values(records.iter(), |mut b, r| {
        b.push_bind(r.person_id)
            .push_bind(r.serving_date)
            .push_bind(r.attending);
    });
    qb.push(" ON CONFLICT (person_id, serving_date) DO UPDATE SET attending = EXCLUDED.attending");
    qb.build().execute(pool).await?;
    Ok(())
}
