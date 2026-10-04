import type { Pool, PoolClient } from 'pg';
import type {
  Order,
  InsertOrder,
  UpdateOrder,
  OrderSummary,
  OrderDetail,
  OrderStatus,
  OrderSource,
} from '$lib/types';
import type { Db } from '$lib/server/db';
import {
  type PaginationParams,
  type PaginatedResult,
  paginationSQL,
  buildPaginatedResult,
} from './pagination';

// -------------------------------------------------------------
// FILTERS
// -------------------------------------------------------------

export interface OrderFilters {
  company_id?: string;
  recipe_id?: string;
  status?: OrderStatus;
  order_source?: OrderSource;
  serving_date_from?: string | Date;
  serving_date_to?: string | Date;
  shipment_date?: string | Date;
  frozen?: boolean;
  chilled?: boolean;
}

// -------------------------------------------------------------
// READ
// -------------------------------------------------------------

export async function getOrders(
  db: Db,
  filters: OrderFilters = {}
): Promise<OrderSummary[]> {
  const conditions: string[] = [];
  const params: unknown[] = [];

	// For the where clause so we can parameterize it in TS
  if (filters.company_id) {
    params.push(filters.company_id);
    conditions.push(`o.company_id = $${params.length}`);
  }
  if (filters.recipe_id) {
    params.push(filters.recipe_id);
    conditions.push(`o.recipe_id = $${params.length}`);
  }
  if (filters.status) {
    params.push(filters.status);
    conditions.push(`o.status = $${params.length}`);
  }
  if (filters.order_source) {
    params.push(filters.order_source);
    conditions.push(`o.order_source = $${params.length}`);
  }
  if (filters.serving_date_from) {
    params.push(filters.serving_date_from);
    conditions.push(`o.serving_date >= $${params.length}::date`);
  }
  if (filters.serving_date_to) {
    params.push(filters.serving_date_to);
    conditions.push(`o.serving_date <= $${params.length}::date`);
  }
  if (filters.shipment_date) {
    params.push(filters.shipment_date);
    conditions.push(`o.shipment_date = $${params.length}::date`);
  }
  if (filters.frozen !== undefined) {
    params.push(filters.frozen);
    conditions.push(`o.frozen = $${params.length}`);
  }
  if (filters.chilled !== undefined) {
    params.push(filters.chilled);
    conditions.push(`o.chilled = $${params.length}`);
  }

  const where = conditions.length > 0 ? `WHERE ${conditions.join(' AND ')}` : '';

  const result = await db.query<OrderSummary>(
    `SELECT
       o.id,
       o.company_id,
       o.recipe_id,
       o.created_by,
       o.order_source,
       o.status,
       o.meal_category,
       o.amount,
       o.portion_size,
       o.day,
       o.serving_date,
       o.shipment_date,
       o.frozen,
       o.chilled,
       o.recurring,
       o.client,
       o.created_at,
       c.name     AS company_name,
       r.name     AS recipe_name
     FROM "order" o
     JOIN company c ON c.id = o.company_id
     LEFT JOIN recipe r ON r.id = o.recipe_id
     ${where}
     ORDER BY o.serving_date DESC, c.name ASC`,
    params
  );

  return result.rows;
}

export async function getOrdersPaginated(
  db: Db,
  filters: OrderFilters = {},
  pagination: PaginationParams
): Promise<PaginatedResult<OrderSummary>> {
  const conditions: string[] = [];
  const params: unknown[] = [];

  if (filters.company_id) {
    params.push(filters.company_id);
    conditions.push(`o.company_id = $${params.length}`);
  }
  if (filters.recipe_id) {
    params.push(filters.recipe_id);
    conditions.push(`o.recipe_id = $${params.length}`);
  }
  if (filters.status) {
    params.push(filters.status);
    conditions.push(`o.status = $${params.length}`);
  }
  if (filters.order_source) {
    params.push(filters.order_source);
    conditions.push(`o.order_source = $${params.length}`);
  }
  if (filters.serving_date_from) {
    params.push(filters.serving_date_from);
    conditions.push(`o.serving_date >= $${params.length}::date`);
  }
  if (filters.serving_date_to) {
    params.push(filters.serving_date_to);
    conditions.push(`o.serving_date <= $${params.length}::date`);
  }
  if (filters.shipment_date) {
    params.push(filters.shipment_date);
    conditions.push(`o.shipment_date = $${params.length}::date`);
  }
  if (filters.frozen !== undefined) {
    params.push(filters.frozen);
    conditions.push(`o.frozen = $${params.length}`);
  }
  if (filters.chilled !== undefined) {
    params.push(filters.chilled);
    conditions.push(`o.chilled = $${params.length}`);
  }

  const where = conditions.length > 0 ? `WHERE ${conditions.join(' AND ')}` : '';

  // Count query
  const countResult = await db.query<{ count: string }>(
    `SELECT COUNT(*) AS count
     FROM "order" o
     JOIN company c ON c.id = o.company_id
     ${where}`,
    params
  );
  const total = parseInt(countResult.rows[0].count, 10);

  // Data query with LIMIT/OFFSET
  const pag = paginationSQL(pagination, params.length + 1);
  const result = await db.query<OrderSummary>(
    `SELECT
       o.id,
       o.company_id,
       o.recipe_id,
       o.created_by,
       o.order_source,
       o.status,
       o.meal_category,
       o.amount,
       o.portion_size,
       o.day,
       o.serving_date,
       o.shipment_date,
       o.frozen,
       o.chilled,
       o.recurring,
       o.client,
       o.created_at,
       c.name     AS company_name,
       r.name     AS recipe_name
     FROM "order" o
     JOIN company c ON c.id = o.company_id
     LEFT JOIN recipe r ON r.id = o.recipe_id
     ${where}
     ORDER BY o.serving_date DESC, c.name ASC
     ${pag.sql}`,
    [...params, ...pag.values]
  );

  return buildPaginatedResult(result.rows, total, pagination);
}

export async function getOrderById(db: Db, id: string): Promise<OrderDetail | null> {
  const result = await db.query<
    Order & {
      company_name: string;
      company_contact_email: string | null;
      company_phone: string | null;
      company_shipping_address: string | null;
      company_active: boolean;
      company_created_at: Date;
      recipe_name: string | null;
      recipe_active: boolean | null;
      recipe_created_at: Date | null;
      recipe_price_per_meal: number | null;
      created_by_email: string | null;
    }
  >(
    `SELECT
       o.id,
       o.company_id,
       o.recipe_id,
       o.created_by,
       o.order_source,
       o.status,
       o.meal_category,
       o.amount,
       o.portion_size,
       o.day,
       o.serving_date,
       o.shipment_date,
       o.frozen,
       o.chilled,
       o.recurring,
       o.client,
       o.created_at,
       c.name             AS company_name,
       c.contact_email    AS company_contact_email,
       c.phone            AS company_phone,
       c.shipping_address AS company_shipping_address,
       c.active           AS company_active,
       c.created_at       AS company_created_at,
       r.name             AS recipe_name,
       r.active           AS recipe_active,
       r.created_at       AS recipe_created_at,
       r.price_per_meal   AS recipe_price_per_meal,
       u.email            AS created_by_email
     FROM "order" o
     JOIN company c ON c.id = o.company_id
     LEFT JOIN recipe r ON r.id = o.recipe_id
     LEFT JOIN "user" u ON u.id = o.created_by
     WHERE o.id = $1`,
    [id]
  );

  const row = result.rows[0];
  if (!row) return null;

  return {
    id: row.id,
    company_id: row.company_id,
    recipe_id: row.recipe_id,
    created_by: row.created_by,
    order_source: row.order_source,
    status: row.status,
    meal_category: row.meal_category,
    amount: parseFloat(row.amount as unknown as string),
    portion_size: row.portion_size != null ? parseFloat(row.portion_size as unknown as string) : null,
    day: row.day ?? null,
    serving_date: row.serving_date,
    shipment_date: row.shipment_date,
    frozen: row.frozen,
    chilled: row.chilled,
    recurring: row.recurring,
    client: row.client,
    created_at: row.created_at,
    created_by_email: row.created_by_email,
    company: {
      id: row.company_id,
      name: row.company_name,
      contact_email: row.company_contact_email,
      phone: row.company_phone,
      shipping_address: row.company_shipping_address,
      active: row.company_active,
      created_at: row.company_created_at,
    },
    recipe: row.recipe_id
      ? {
          id: row.recipe_id,
          name: row.recipe_name!,
          active: row.recipe_active!,
          created_at: row.recipe_created_at!,
          price_per_meal: row.recipe_price_per_meal ?? null,
        }
      : null,
  };
}

// Useful for the dashboard — orders going out on a specific shipment date.
export async function getOrdersByShipmentDate(
  db: Db,
  shipmentDate: Date
): Promise<OrderSummary[]> {
  return getOrders(db, { shipment_date: shipmentDate });
}

// Useful for the dashboard summary counts.
export async function getOrderCountsByStatus(
  db: Db,
  filters: Pick<OrderFilters, 'serving_date_from' | 'serving_date_to'> = {}
): Promise<Record<OrderStatus, number>> {
  const conditions: string[] = [];
  const params: unknown[] = [];

  if (filters.serving_date_from) {
    params.push(filters.serving_date_from);
    conditions.push(`serving_date >= $${params.length}::date`);
  }
  if (filters.serving_date_to) {
    params.push(filters.serving_date_to);
    conditions.push(`serving_date <= $${params.length}::date`);
  }

  const where = conditions.length > 0 ? `WHERE ${conditions.join(' AND ')}` : '';

  const result = await db.query<{ status: OrderStatus; count: string }>(
    `SELECT status, COUNT(*) AS count
     FROM "order"
     ${where}
     GROUP BY status`,
    params
  );

  const counts: Record<OrderStatus, number> = {
    pending: 0,
    confirmed: 0,
    fulfilled: 0,
    cancelled: 0,
  };

  for (const row of result.rows) {
    counts[row.status] = parseInt(row.count, 10);
  }

  return counts;
}

// -------------------------------------------------------------
// WRITE
// -------------------------------------------------------------

export async function createOrder(
  client: Pool | PoolClient,
  data: InsertOrder
): Promise<Order> {
  const result = await client.query<Order>(
    `INSERT INTO "order" (
       company_id, recipe_id, created_by, order_source, status,
       meal_category, amount, portion_size, day, serving_date, shipment_date,
       frozen, chilled, recurring, client
     )
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
     RETURNING
       id, company_id, recipe_id, created_by, order_source, status,
       meal_category, amount, portion_size, day, serving_date, shipment_date,
       frozen, chilled, recurring, client, created_at`,
    [
      data.company_id,
      data.recipe_id ?? null,
      data.created_by ?? null,
      data.order_source,
      data.status ?? 'pending',
      data.meal_category ?? null,
      data.amount,
      data.portion_size ?? null,
      data.day ?? null,
      data.serving_date,
      data.shipment_date,
      data.frozen ?? false,
      data.chilled ?? false,
      data.recurring ?? false,
      data.client ?? null,
    ]
  );
  return result.rows[0];
}

export async function updateOrder(
  db: Db,
  id: string,
  data: UpdateOrder
): Promise<Order | null> {
  const result = await db.query<Order>(
    `UPDATE "order"
     SET
       recipe_id     = COALESCE($1,  recipe_id),
       status        = COALESCE($2,  status),
       meal_category = COALESCE($3,  meal_category),
       amount        = COALESCE($4,  amount),
       serving_date  = COALESCE($5,  serving_date),
       shipment_date = COALESCE($6,  shipment_date),
       frozen        = COALESCE($7,  frozen),
       chilled       = COALESCE($8,  chilled),
       recurring     = COALESCE($9,  recurring),
       client        = COALESCE($10, client),
       portion_size  = COALESCE($11, portion_size),
       day           = COALESCE($12, day)
     WHERE id = $13
     RETURNING
       id, company_id, recipe_id, created_by, order_source, status,
       meal_category, amount, portion_size, day, serving_date, shipment_date,
       frozen, chilled, recurring, client, created_at`,
    [
      data.recipe_id ?? null,
      data.status ?? null,
      data.meal_category ?? null,
      data.amount ?? null,
      data.serving_date ?? null,
      data.shipment_date ?? null,
      data.frozen ?? null,
      data.chilled ?? null,
      data.recurring ?? null,
      data.client ?? null,
      data.portion_size ?? null,
      data.day ?? null,
      id,
    ]
  );
  return result.rows[0] ?? null;
}

// -------------------------------------------------------------
// STATUS TRANSITIONS
// Each transition is explicit rather than a generic setStatus —
// this makes invalid transitions visible at the call site and
// ensures business rules can be added per transition later.
// -------------------------------------------------------------

export async function confirmOrder(db: Db, id: string): Promise<Order | null> {
  const result = await db.query<Order>(
    `UPDATE "order"
     SET status = 'confirmed'
     WHERE id = $1 AND status = 'pending'
     RETURNING
       id, company_id, recipe_id, created_by, order_source, status,
       meal_category, amount, portion_size, day, serving_date, shipment_date,
       frozen, chilled, recurring, client, created_at`,
    [id]
  );
  // Returns null if the order wasn't in 'pending' state
  return result.rows[0] ?? null;
}

export async function fulfillOrder(db: Db, id: string): Promise<Order | null> {
  const result = await db.query<Order>(
    `UPDATE "order"
     SET status = 'fulfilled'
     WHERE id = $1 AND status = 'confirmed'
     RETURNING
       id, company_id, recipe_id, created_by, order_source, status,
       meal_category, amount, portion_size, day, serving_date, shipment_date,
       frozen, chilled, recurring, client, created_at`,
    [id]
  );
  return result.rows[0] ?? null;
}

export async function cancelOrder(db: Db, id: string): Promise<Order | null> {
  const result = await db.query<Order>(
    `UPDATE "order"
     SET status = 'cancelled'
     WHERE id = $1 AND status IN ('pending', 'confirmed')
     RETURNING
       id, company_id, recipe_id, created_by, order_source, status,
       meal_category, amount, portion_size, day, serving_date, shipment_date,
       frozen, chilled, recurring, client, created_at`,
    [id]
  );
  // Returns null if the order was already fulfilled or cancelled
  return result.rows[0] ?? null;
}

// -------------------------------------------------------------
// BULK STATUS TRANSITIONS
// Sets status unconditionally — allows moving to any state.
// -------------------------------------------------------------

export async function setOrdersStatusBatch(
  db: Db,
  ids: string[],
  status: OrderStatus
): Promise<Order[]> {
  if (ids.length === 0) return [];
  const result = await db.query<Order>(
    `UPDATE "order"
     SET status = $2
     WHERE id = ANY($1::uuid[])
     RETURNING
       id, company_id, recipe_id, created_by, order_source, status,
       meal_category, amount, portion_size, day, serving_date, shipment_date,
       frozen, chilled, recurring, client, created_at`,
    [ids, status]
  );
  return result.rows;
}

// -------------------------------------------------------------
// BULK INSERT
// Used by imports.ts — accepts a PoolClient so the caller controls
// the transaction boundary. Never call this outside a transaction.
// -------------------------------------------------------------

export async function insertOrdersBatch(
  client: PoolClient,
  orders: InsertOrder[]
): Promise<Order[]> {
  if (orders.length === 0) return [];

  const values = orders
    .map(
      (_, i) =>
        `($${i * 15 + 1}, $${i * 15 + 2}, $${i * 15 + 3}, $${i * 15 + 4}, $${i * 15 + 5},
          $${i * 15 + 6}, $${i * 15 + 7}, $${i * 15 + 8}, $${i * 15 + 9}, $${i * 15 + 10},
          $${i * 15 + 11}, $${i * 15 + 12}, $${i * 15 + 13}, $${i * 15 + 14}, $${i * 15 + 15})`
    )
    .join(', ');

  const params = orders.flatMap((o) => [
    o.company_id,
    o.recipe_id ?? null,
    o.created_by ?? null,
    o.order_source,
    o.status ?? 'pending',
    o.meal_category ?? null,
    o.amount,
    o.portion_size ?? null,
    o.day ?? null,
    o.serving_date,
    o.shipment_date,
    o.frozen ?? false,
    o.chilled ?? false,
    o.recurring ?? false,
    o.client ?? null,
  ]);

  const result = await client.query<Order>(
    `INSERT INTO "order" (
       company_id, recipe_id, created_by, order_source, status,
       meal_category, amount, portion_size, day, serving_date, shipment_date,
       frozen, chilled, recurring, client
     )
     VALUES ${values}
     RETURNING
       id, company_id, recipe_id, created_by, order_source, status,
       meal_category, amount, portion_size, day, serving_date, shipment_date,
       frozen, chilled, recurring, client, created_at`,
    params
  );

  return result.rows;
}
