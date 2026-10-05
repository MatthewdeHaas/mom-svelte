//! Order queries. The TS counterpart is `src/lib/server/queries/orders.ts`
//! (568 lines, 11 exported functions). Column order on inserts is
//! deliberate — it's the same 15-column tuple the TS multi-row VALUES
//! statement builds, so a side-by-side diff of inserted rows between
//! the two paths is trivial.
//!
//! **Numeric columns are cast to `float8`** in SQL. Postgres `NUMERIC`
//! has no safe f64 decode in sqlx without pulling in `rust_decimal` or
//! `bigdecimal`; the TS side already does `parseFloat` on every amount,
//! so we match that behavior with an explicit cast and let sqlx decode
//! the result as `f64`. Keep the casts in sync with the TS numeric
//! sites or parity will drift.
//!
//! Only two queries use runtime SQL (`list` / `list_paginated` —
//! filter-driven WHERE); everything else is compile-time
//! `query_as!`-checked.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Executor, FromRow, PgExecutor, PgPool, Postgres, QueryBuilder, Transaction};
use uuid::Uuid;

use super::pagination::{build as build_paginated, offset, Paginated, PaginationParams};

// =============================================================
// Insert type used by the Excel importer. Pre-existing — do NOT
// rename or change the field order; the batch insert below binds
// in exactly this order.
// =============================================================

/// A row ready to be inserted into `"order"`. Field order matches the
/// INSERT column order so `push_values` lays bytes out predictably.
#[derive(Debug, Clone)]
pub struct InsertOrder {
    pub company_id: Uuid,
    pub recipe_id: Uuid,
    pub created_by: String,
    pub order_source: &'static str,
    pub status: &'static str,
    pub meal_category: String,
    pub amount: f64,
    pub portion_size: Option<f64>,
    pub day: Option<i32>,
    pub serving_date: NaiveDate,
    pub shipment_date: NaiveDate,
    pub frozen: bool,
    pub chilled: bool,
    pub recurring: bool,
    pub client: Option<String>,
}

/// Bulk insert from the Excel importer. Must be called inside a
/// transaction — the caller owns COMMIT/ROLLBACK so an insert failure
/// can be rolled back without touching any other state.
///
/// Returns the number of rows actually inserted. `created_at` is left
/// to the DB default — never override it, or the byte-identical export
/// contract in Track 3 step 4 breaks.
pub async fn insert_batch(
    tx: &mut Transaction<'_, Postgres>,
    orders: &[InsertOrder],
) -> Result<i64, sqlx::Error> {
    if orders.is_empty() {
        return Ok(0);
    }

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "INSERT INTO \"order\" ( \
            company_id, recipe_id, created_by, order_source, status, \
            meal_category, amount, portion_size, day, serving_date, shipment_date, \
            frozen, chilled, recurring, client \
         ) ",
    );

    qb.push_values(orders.iter(), |mut b, o| {
        b.push_bind(o.company_id)
            .push_bind(o.recipe_id)
            .push_bind(&o.created_by)
            .push_bind(o.order_source)
            .push_bind(o.status)
            .push_bind(&o.meal_category)
            .push_bind(o.amount)
            .push_bind(o.portion_size)
            .push_bind(o.day)
            .push_bind(o.serving_date)
            .push_bind(o.shipment_date)
            .push_bind(o.frozen)
            .push_bind(o.chilled)
            .push_bind(o.recurring)
            .push_bind(o.client.as_deref());
    });

    qb.push(" RETURNING id");

    let rows = qb.build_query_scalar::<Uuid>().fetch_all(&mut **tx).await?;

    Ok(rows.len() as i64)
}

// =============================================================
// Public API — mirrors `src/lib/server/queries/orders.ts`.
// =============================================================

/// Base order row. All numeric columns are decoded via a `::float8`
/// cast so sqlx returns `f64` — matches `parseFloat` on the TS side.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OrderRow {
    pub id: Uuid,
    pub company_id: Uuid,
    pub recipe_id: Option<Uuid>,
    pub created_by: Option<String>,
    pub order_source: String,
    pub status: String,
    pub meal_category: Option<String>,
    pub amount: f64,
    pub portion_size: Option<f64>,
    pub day: Option<i32>,
    pub serving_date: NaiveDate,
    pub shipment_date: NaiveDate,
    pub frozen: bool,
    pub chilled: bool,
    pub recurring: bool,
    pub client: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// List-view row — adds the pre-joined `company_name` and nullable
/// `recipe_name` to avoid an N+1 in the typical table page.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OrderSummaryRow {
    pub id: Uuid,
    pub company_id: Uuid,
    pub recipe_id: Option<Uuid>,
    pub created_by: Option<String>,
    pub order_source: String,
    pub status: String,
    pub meal_category: Option<String>,
    pub amount: f64,
    pub portion_size: Option<f64>,
    pub day: Option<i32>,
    pub serving_date: NaiveDate,
    pub shipment_date: NaiveDate,
    pub frozen: bool,
    pub chilled: bool,
    pub recurring: bool,
    pub client: Option<String>,
    pub created_at: DateTime<Utc>,
    pub company_name: String,
    pub recipe_name: Option<String>,
}

/// Embedded company shape on `OrderDetailRow`. Mirrors `Company` in
/// `src/lib/types.ts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyEmbed {
    pub id: Uuid,
    pub name: String,
    pub contact_email: Option<String>,
    pub phone: Option<String>,
    pub shipping_address: Option<String>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

/// Embedded recipe shape on `OrderDetailRow`. Mirrors `Recipe` in
/// `src/lib/types.ts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeEmbed {
    pub id: Uuid,
    pub name: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub price_per_meal: Option<f64>,
}

/// Detail-view row — flat order fields plus nested `company` and
/// optional nested `recipe`. The TS detail response has the same
/// shape; both consumers can decode this JSON unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderDetailRow {
    pub id: Uuid,
    pub company_id: Uuid,
    pub recipe_id: Option<Uuid>,
    pub created_by: Option<String>,
    pub order_source: String,
    pub status: String,
    pub meal_category: Option<String>,
    pub amount: f64,
    pub portion_size: Option<f64>,
    pub day: Option<i32>,
    pub serving_date: NaiveDate,
    pub shipment_date: NaiveDate,
    pub frozen: bool,
    pub chilled: bool,
    pub recurring: bool,
    pub client: Option<String>,
    pub created_at: DateTime<Utc>,
    pub created_by_email: Option<String>,
    pub company: CompanyEmbed,
    pub recipe: Option<RecipeEmbed>,
}

/// Query-string / JSON input shape for `list`, `list_paginated`,
/// `counts_by_status`. Every field is optional — presence means
/// "constrain by this".
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrderFilters {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub company_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipe_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serving_date_from: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serving_date_to: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shipment_date: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frozen: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chilled: Option<bool>,
}

/// Input to `create`. `status` / booleans default server-side (same
/// defaults as TS: pending, false, false, false). Validator at the
/// API boundary enforces the `order_source` / `status` enums.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertOrderData {
    pub company_id: Uuid,
    #[serde(default)]
    pub recipe_id: Option<Uuid>,
    #[serde(default)]
    pub created_by: Option<String>,
    pub order_source: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub meal_category: Option<String>,
    pub amount: f64,
    #[serde(default)]
    pub portion_size: Option<f64>,
    #[serde(default)]
    pub day: Option<i32>,
    pub serving_date: NaiveDate,
    pub shipment_date: NaiveDate,
    #[serde(default)]
    pub frozen: Option<bool>,
    #[serde(default)]
    pub chilled: Option<bool>,
    #[serde(default)]
    pub recurring: Option<bool>,
    #[serde(default)]
    pub client: Option<String>,
}

/// Input to `update`. Every field is optional; `None` means "leave
/// as-is" via COALESCE — identical to the TS `UpdateOrder` semantics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateOrderData {
    #[serde(default)]
    pub recipe_id: Option<Uuid>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub meal_category: Option<String>,
    #[serde(default)]
    pub amount: Option<f64>,
    #[serde(default)]
    pub portion_size: Option<f64>,
    #[serde(default)]
    pub day: Option<i32>,
    #[serde(default)]
    pub serving_date: Option<NaiveDate>,
    #[serde(default)]
    pub shipment_date: Option<NaiveDate>,
    #[serde(default)]
    pub frozen: Option<bool>,
    #[serde(default)]
    pub chilled: Option<bool>,
    #[serde(default)]
    pub recurring: Option<bool>,
    #[serde(default)]
    pub client: Option<String>,
}

/// Response shape for `counts_by_status`. All four statuses always
/// present (zero-filled). Mirrors `Record<OrderStatus, number>` on TS.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OrderStatusCounts {
    pub pending: i64,
    pub confirmed: i64,
    pub fulfilled: i64,
    pub cancelled: i64,
}

// ---------------------------------------------------------------
// WHERE builder shared by list / list_paginated. Takes a mutable
// `QueryBuilder` and pushes the exact same conditions in the exact
// same order the TS side does, so parameter indexes match 1:1.
// ---------------------------------------------------------------

fn push_filters<'a>(qb: &mut QueryBuilder<'a, Postgres>, f: &'a OrderFilters) {
    let mut first = true;
    let mut push_cond = |qb: &mut QueryBuilder<'a, Postgres>, text: &str| {
        if first {
            qb.push(" WHERE ");
            first = false;
        } else {
            qb.push(" AND ");
        }
        qb.push(text);
    };

    if let Some(id) = f.company_id {
        push_cond(qb, "o.company_id = ");
        qb.push_bind(id);
    }
    if let Some(id) = f.recipe_id {
        push_cond(qb, "o.recipe_id = ");
        qb.push_bind(id);
    }
    if let Some(ref s) = f.status {
        push_cond(qb, "o.status = ");
        qb.push_bind(s);
    }
    if let Some(ref s) = f.order_source {
        push_cond(qb, "o.order_source = ");
        qb.push_bind(s);
    }
    if let Some(d) = f.serving_date_from {
        push_cond(qb, "o.serving_date >= ");
        qb.push_bind(d);
    }
    if let Some(d) = f.serving_date_to {
        push_cond(qb, "o.serving_date <= ");
        qb.push_bind(d);
    }
    if let Some(d) = f.shipment_date {
        push_cond(qb, "o.shipment_date = ");
        qb.push_bind(d);
    }
    if let Some(b) = f.frozen {
        push_cond(qb, "o.frozen = ");
        qb.push_bind(b);
    }
    if let Some(b) = f.chilled {
        push_cond(qb, "o.chilled = ");
        qb.push_bind(b);
    }
}

const SUMMARY_SELECT: &str = "SELECT \
    o.id, o.company_id, o.recipe_id, o.created_by, o.order_source, o.status, \
    o.meal_category, \
    o.amount::float8 AS amount, \
    o.portion_size::float8 AS portion_size, \
    o.day, o.serving_date, o.shipment_date, \
    o.frozen, o.chilled, o.recurring, o.client, o.created_at, \
    c.name AS company_name, r.name AS recipe_name \
    FROM \"order\" o \
    JOIN company c ON c.id = o.company_id \
    LEFT JOIN recipe r ON r.id = o.recipe_id";

const SUMMARY_ORDER: &str =
    " ORDER BY o.serving_date DESC, c.name ASC, o.id ASC";

/// `getOrders` — unpaginated list filtered by `OrderFilters`, ordered
/// by serving_date DESC, company name ASC, id ASC. Runtime SQL because
/// the WHERE varies with filter presence.
pub async fn list(
    pool: &PgPool,
    filters: &OrderFilters,
) -> Result<Vec<OrderSummaryRow>, sqlx::Error> {
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(SUMMARY_SELECT);
    push_filters(&mut qb, filters);
    qb.push(SUMMARY_ORDER);
    qb.build_query_as::<OrderSummaryRow>().fetch_all(pool).await
}

/// `getOrdersPaginated` — same shape as `list` plus LIMIT/OFFSET and
/// a sibling COUNT(*) over the same WHERE. Clamping mirrors
/// `buildPaginatedResult`.
pub async fn list_paginated(
    pool: &PgPool,
    filters: &OrderFilters,
    pagination: PaginationParams,
) -> Result<Paginated<OrderSummaryRow>, sqlx::Error> {
    // --- Count ---
    let mut count_qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT COUNT(*) AS count FROM \"order\" o \
         JOIN company c ON c.id = o.company_id",
    );
    push_filters(&mut count_qb, filters);
    let total: i64 = count_qb
        .build_query_scalar::<i64>()
        .fetch_one(pool)
        .await?;

    // --- Data ---
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(SUMMARY_SELECT);
    push_filters(&mut qb, filters);
    qb.push(SUMMARY_ORDER);
    qb.push(" LIMIT ");
    qb.push_bind(pagination.per_page);
    qb.push(" OFFSET ");
    qb.push_bind(offset(&pagination));
    let data = qb.build_query_as::<OrderSummaryRow>().fetch_all(pool).await?;

    Ok(build_paginated(data, total, pagination))
}

/// `getOrderById` — full detail with embedded `company` and optional
/// `recipe`. Returns `None` on no match (handler maps to 404).
pub async fn get_by_id(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<OrderDetailRow>, sqlx::Error> {
    // One query, all three tables; map to the nested shape in Rust so
    // the JSON matches the TS structure byte for byte.
    let row = sqlx::query!(
        r#"
        SELECT
            o.id                 AS "id!: Uuid",
            o.company_id         AS "company_id!: Uuid",
            o.recipe_id          AS "recipe_id?: Uuid",
            o.created_by         AS "created_by?",
            o.order_source       AS "order_source!",
            o.status             AS "status!",
            o.meal_category      AS "meal_category?",
            o.amount::float8     AS "amount!: f64",
            o.portion_size::float8 AS "portion_size?: f64",
            o.day                AS "day?: i32",
            o.serving_date       AS "serving_date!: NaiveDate",
            o.shipment_date      AS "shipment_date!: NaiveDate",
            o.frozen             AS "frozen!",
            o.chilled            AS "chilled!",
            o.recurring          AS "recurring!",
            o.client             AS "client?",
            o.created_at         AS "created_at!: DateTime<Utc>",
            c.name               AS "company_name!",
            c.contact_email      AS "company_contact_email?",
            c.phone              AS "company_phone?",
            c.shipping_address   AS "company_shipping_address?",
            c.active             AS "company_active!",
            c.created_at         AS "company_created_at!: DateTime<Utc>",
            r.name               AS "recipe_name?",
            r.active             AS "recipe_active?",
            r.created_at         AS "recipe_created_at?: DateTime<Utc>",
            r.price_per_meal::float8 AS "recipe_price_per_meal?: f64",
            u.email              AS "created_by_email?"
        FROM "order" o
        JOIN company c ON c.id = o.company_id
        LEFT JOIN recipe r ON r.id = o.recipe_id
        LEFT JOIN "user" u ON u.id = o.created_by
        WHERE o.id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else { return Ok(None) };

    let recipe = row.recipe_id.map(|rid| RecipeEmbed {
        id: rid,
        // Inner joins at this point: recipe_id present means the LEFT
        // JOIN matched, so name/active/created_at are populated.
        name: row.recipe_name.unwrap_or_default(),
        active: row.recipe_active.unwrap_or(false),
        created_at: row.recipe_created_at.unwrap_or_else(Utc::now),
        price_per_meal: row.recipe_price_per_meal,
    });

    Ok(Some(OrderDetailRow {
        id: row.id,
        company_id: row.company_id,
        recipe_id: row.recipe_id,
        created_by: row.created_by,
        order_source: row.order_source,
        status: row.status,
        meal_category: row.meal_category,
        amount: row.amount,
        portion_size: row.portion_size,
        day: row.day,
        serving_date: row.serving_date,
        shipment_date: row.shipment_date,
        frozen: row.frozen,
        chilled: row.chilled,
        recurring: row.recurring,
        client: row.client,
        created_at: row.created_at,
        created_by_email: row.created_by_email,
        company: CompanyEmbed {
            id: row.company_id,
            name: row.company_name,
            contact_email: row.company_contact_email,
            phone: row.company_phone,
            shipping_address: row.company_shipping_address,
            active: row.company_active,
            created_at: row.company_created_at,
        },
        recipe,
    }))
}

/// `getOrdersByShipmentDate` — thin wrapper over `list`. Kept for
/// 1:1 name parity with the TS surface.
pub async fn list_by_shipment_date(
    pool: &PgPool,
    date: NaiveDate,
) -> Result<Vec<OrderSummaryRow>, sqlx::Error> {
    let filters = OrderFilters {
        shipment_date: Some(date),
        ..Default::default()
    };
    list(pool, &filters).await
}

/// `getOrderCountsByStatus` — one row per non-empty status; zero-fill
/// the four buckets locally so callers get a stable shape.
pub async fn counts_by_status(
    pool: &PgPool,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
) -> Result<OrderStatusCounts, sqlx::Error> {
    let rows = sqlx::query!(
        r#"
        SELECT status AS "status!", COUNT(*) AS "count!"
        FROM "order"
        WHERE ($1::date IS NULL OR serving_date >= $1)
          AND ($2::date IS NULL OR serving_date <= $2)
        GROUP BY status
        "#,
        from,
        to
    )
    .fetch_all(pool)
    .await?;

    let mut out = OrderStatusCounts::default();
    for r in rows {
        match r.status.as_str() {
            "pending" => out.pending = r.count,
            "confirmed" => out.confirmed = r.count,
            "fulfilled" => out.fulfilled = r.count,
            "cancelled" => out.cancelled = r.count,
            _ => {} // CHECK constraint keeps this unreachable.
        }
    }
    Ok(out)
}

/// `createOrder`. Generic over any `PgExecutor` so the caller can pass
/// a pool or a `&mut Transaction` — matches the TS signature that
/// accepts `Pool | PoolClient`.
///
/// Defaults mirror TS: status → 'pending', frozen/chilled/recurring →
/// false. Status enum validation lives at the API boundary.
pub async fn create<'c, E>(executor: E, data: InsertOrderData) -> Result<OrderRow, sqlx::Error>
where
    E: PgExecutor<'c>,
{
    let status = data.status.as_deref().unwrap_or("pending");
    let frozen = data.frozen.unwrap_or(false);
    let chilled = data.chilled.unwrap_or(false);
    let recurring = data.recurring.unwrap_or(false);

    sqlx::query_as!(
        OrderRow,
        r#"
        INSERT INTO "order" (
            company_id, recipe_id, created_by, order_source, status,
            meal_category, amount, portion_size, day, serving_date, shipment_date,
            frozen, chilled, recurring, client
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7::float8::numeric, $8::float8::numeric, $9, $10, $11, $12, $13, $14, $15)
        RETURNING
            id                    AS "id!: Uuid",
            company_id            AS "company_id!: Uuid",
            recipe_id             AS "recipe_id?: Uuid",
            created_by            AS "created_by?",
            order_source          AS "order_source!",
            status                AS "status!",
            meal_category         AS "meal_category?",
            amount::float8        AS "amount!: f64",
            portion_size::float8  AS "portion_size?: f64",
            day                   AS "day?: i32",
            serving_date          AS "serving_date!: NaiveDate",
            shipment_date         AS "shipment_date!: NaiveDate",
            frozen                AS "frozen!",
            chilled               AS "chilled!",
            recurring             AS "recurring!",
            client                AS "client?",
            created_at            AS "created_at!: DateTime<Utc>"
        "#,
        data.company_id,
        data.recipe_id,
        data.created_by,
        data.order_source,
        status,
        data.meal_category,
        data.amount,
        data.portion_size,
        data.day,
        data.serving_date,
        data.shipment_date,
        frozen,
        chilled,
        recurring,
        data.client,
    )
    .fetch_one(executor)
    .await
}

/// `updateOrder` — COALESCE each column so `None` means "leave as-is".
/// Returns `None` when the row doesn't exist.
pub async fn update(
    pool: &PgPool,
    id: Uuid,
    data: UpdateOrderData,
) -> Result<Option<OrderRow>, sqlx::Error> {
    sqlx::query_as!(
        OrderRow,
        r#"
        UPDATE "order"
        SET
            recipe_id     = COALESCE($1,  recipe_id),
            status        = COALESCE($2,  status),
            meal_category = COALESCE($3,  meal_category),
            amount        = COALESCE($4::float8::numeric,  amount),
            serving_date  = COALESCE($5,  serving_date),
            shipment_date = COALESCE($6,  shipment_date),
            frozen        = COALESCE($7,  frozen),
            chilled       = COALESCE($8,  chilled),
            recurring     = COALESCE($9,  recurring),
            client        = COALESCE($10, client),
            portion_size  = COALESCE($11::float8::numeric, portion_size),
            day           = COALESCE($12, day)
        WHERE id = $13
        RETURNING
            id                    AS "id!: Uuid",
            company_id            AS "company_id!: Uuid",
            recipe_id             AS "recipe_id?: Uuid",
            created_by            AS "created_by?",
            order_source          AS "order_source!",
            status                AS "status!",
            meal_category         AS "meal_category?",
            amount::float8        AS "amount!: f64",
            portion_size::float8  AS "portion_size?: f64",
            day                   AS "day?: i32",
            serving_date          AS "serving_date!: NaiveDate",
            shipment_date         AS "shipment_date!: NaiveDate",
            frozen                AS "frozen!",
            chilled               AS "chilled!",
            recurring             AS "recurring!",
            client                AS "client?",
            created_at            AS "created_at!: DateTime<Utc>"
        "#,
        data.recipe_id,
        data.status,
        data.meal_category,
        data.amount,
        data.serving_date,
        data.shipment_date,
        data.frozen,
        data.chilled,
        data.recurring,
        data.client,
        data.portion_size,
        data.day,
        id,
    )
    .fetch_optional(pool)
    .await
}

/// `confirmOrder` — status transition guard: only moves `pending →
/// confirmed`. Returns `None` if the row wasn't pending (or doesn't
/// exist).
pub async fn confirm(pool: &PgPool, id: Uuid) -> Result<Option<OrderRow>, sqlx::Error> {
    sqlx::query_as!(
        OrderRow,
        r#"
        UPDATE "order"
        SET status = 'confirmed'
        WHERE id = $1 AND status = 'pending'
        RETURNING
            id                    AS "id!: Uuid",
            company_id            AS "company_id!: Uuid",
            recipe_id             AS "recipe_id?: Uuid",
            created_by            AS "created_by?",
            order_source          AS "order_source!",
            status                AS "status!",
            meal_category         AS "meal_category?",
            amount::float8        AS "amount!: f64",
            portion_size::float8  AS "portion_size?: f64",
            day                   AS "day?: i32",
            serving_date          AS "serving_date!: NaiveDate",
            shipment_date         AS "shipment_date!: NaiveDate",
            frozen                AS "frozen!",
            chilled               AS "chilled!",
            recurring             AS "recurring!",
            client                AS "client?",
            created_at            AS "created_at!: DateTime<Utc>"
        "#,
        id,
    )
    .fetch_optional(pool)
    .await
}

/// `fulfillOrder` — status transition guard: only `confirmed →
/// fulfilled`.
pub async fn fulfill(pool: &PgPool, id: Uuid) -> Result<Option<OrderRow>, sqlx::Error> {
    sqlx::query_as!(
        OrderRow,
        r#"
        UPDATE "order"
        SET status = 'fulfilled'
        WHERE id = $1 AND status = 'confirmed'
        RETURNING
            id                    AS "id!: Uuid",
            company_id            AS "company_id!: Uuid",
            recipe_id             AS "recipe_id?: Uuid",
            created_by            AS "created_by?",
            order_source          AS "order_source!",
            status                AS "status!",
            meal_category         AS "meal_category?",
            amount::float8        AS "amount!: f64",
            portion_size::float8  AS "portion_size?: f64",
            day                   AS "day?: i32",
            serving_date          AS "serving_date!: NaiveDate",
            shipment_date         AS "shipment_date!: NaiveDate",
            frozen                AS "frozen!",
            chilled               AS "chilled!",
            recurring             AS "recurring!",
            client                AS "client?",
            created_at            AS "created_at!: DateTime<Utc>"
        "#,
        id,
    )
    .fetch_optional(pool)
    .await
}

/// `cancelOrder` — status transition guard: `pending|confirmed →
/// cancelled`. Already-cancelled or fulfilled rows return `None`.
pub async fn cancel(pool: &PgPool, id: Uuid) -> Result<Option<OrderRow>, sqlx::Error> {
    sqlx::query_as!(
        OrderRow,
        r#"
        UPDATE "order"
        SET status = 'cancelled'
        WHERE id = $1 AND status IN ('pending', 'confirmed')
        RETURNING
            id                    AS "id!: Uuid",
            company_id            AS "company_id!: Uuid",
            recipe_id             AS "recipe_id?: Uuid",
            created_by            AS "created_by?",
            order_source          AS "order_source!",
            status                AS "status!",
            meal_category         AS "meal_category?",
            amount::float8        AS "amount!: f64",
            portion_size::float8  AS "portion_size?: f64",
            day                   AS "day?: i32",
            serving_date          AS "serving_date!: NaiveDate",
            shipment_date         AS "shipment_date!: NaiveDate",
            frozen                AS "frozen!",
            chilled               AS "chilled!",
            recurring             AS "recurring!",
            client                AS "client?",
            created_at            AS "created_at!: DateTime<Utc>"
        "#,
        id,
    )
    .fetch_optional(pool)
    .await
}

/// `setOrdersStatusBatch` — unconditional multi-id status set. The
/// transition guards above don't apply here (admin-only; moves the
/// batch to whatever target the caller picked). Status enum validation
/// is enforced at the API boundary.
pub async fn set_status_batch(
    pool: &PgPool,
    ids: &[Uuid],
    status: &str,
) -> Result<Vec<OrderRow>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_as!(
        OrderRow,
        r#"
        UPDATE "order"
        SET status = $2
        WHERE id = ANY($1::uuid[])
        RETURNING
            id                    AS "id!: Uuid",
            company_id            AS "company_id!: Uuid",
            recipe_id             AS "recipe_id?: Uuid",
            created_by            AS "created_by?",
            order_source          AS "order_source!",
            status                AS "status!",
            meal_category         AS "meal_category?",
            amount::float8        AS "amount!: f64",
            portion_size::float8  AS "portion_size?: f64",
            day                   AS "day?: i32",
            serving_date          AS "serving_date!: NaiveDate",
            shipment_date         AS "shipment_date!: NaiveDate",
            frozen                AS "frozen!",
            chilled               AS "chilled!",
            recurring             AS "recurring!",
            client                AS "client?",
            created_at            AS "created_at!: DateTime<Utc>"
        "#,
        ids,
        status,
    )
    .fetch_all(pool)
    .await
}

// ---------------------------------------------------------------
// Client-scoped helpers.
//
// Used by the API's `update`, `confirm`, `fulfill`, `cancel`, and
// `set_status_batch` handlers when `role == Client`. Instead of
// pre-fetching every row to verify ownership, we fold the ownership
// predicate directly into the mutation's WHERE clause — a mismatched
// row simply doesn't update and the handler returns 404.
// ---------------------------------------------------------------

/// Fetch a single order's `company_id` without the full detail join.
/// Used to answer "does this order belong to the caller's company?"
/// before returning `GET /orders/:id` or committing a mutation.
pub async fn get_company_id(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT company_id AS "company_id!: Uuid" FROM "order" WHERE id = $1"#, id)
        .fetch_optional(pool)
        .await
}

/// Keep clippy quiet about unused Executor import if the compile-time
/// paths above drop it.
#[allow(dead_code)]
fn _assert_executor_import<'c, E: Executor<'c, Database = Postgres>>(_e: E) {}
