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

// -------------------------------------------------------------
// FILTERS
// -------------------------------------------------------------

export interface OrderFilters {
  company_id?: string;
  recipe_id?: string;
  status?: OrderStatus;
  order_source?: OrderSource;
  serving_date_from?: Date;
  serving_date_to?: Date;
  shipment_date?: Date;
  frozen?: boolean;
  chilled?: boolean;
}

// -------------------------------------------------------------
// READ
// -------------------------------------------------------------

export async function getOrders(
  db: Pool,
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
    conditions.push(`o.serving_date >= $${params.length}`);
  }
  if (filters.serving_date_to) {
    params.push(filters.serving_date_to);
    conditions.push(`o.serving_date <= $${params.length}`);
  }
  if (filters.shipment_date) {
    params.push(filters.shipment_date);
    conditions.push(`o.shipment_date = $${params.length}`);
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
       o.serving_date,
       o.shipment_date,
       o.frozen,
       o.chilled,
       o.recurring,
       o.client_note,
       o.created_at,
       c.name     AS company_name,
       r.name     AS recipe_name
     FROM orders o
     JOIN companies c ON c.id = o.company_id
     LEFT JOIN recipes r ON r.id = o.recipe_id
     ${where}
     ORDER BY o.serving_date DESC, c.name ASC`,
    params
  );

  return result.rows;
}

export async function getOrderById(db: Pool, id: string): Promise<OrderDetail | null> {
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
       o.serving_date,
       o.shipment_date,
       o.frozen,
       o.chilled,
       o.recurring,
       o.client_note,
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
       u.email            AS created_by_email
     FROM orders o
     JOIN companies c ON c.id = o.company_id
     LEFT JOIN recipes r ON r.id = o.recipe_id
     LEFT JOIN users u ON u.id = o.created_by
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
    serving_date: row.serving_date,
    shipment_date: row.shipment_date,
    frozen: row.frozen,
    chilled: row.chilled,
    recurring: row.recurring,
    client_note: row.client_note,
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
        }
      : null,
  };
}

// Useful for the dashboard — orders going out on a specific shipment date.
export async function getOrdersByShipmentDate(
  db: Pool,
  shipmentDate: Date
): Promise<OrderSummary[]> {
  return getOrders(db, { shipment_date: shipmentDate });
}

// Useful for the dashboard summary counts.
export async function getOrderCountsByStatus(
  db: Pool,
  filters: Pick<OrderFilters, 'serving_date_from' | 'serving_date_to'> = {}
): Promise<Record<OrderStatus, number>> {
  const conditions: string[] = [];
  const params: unknown[] = [];

  if (filters.serving_date_from) {
    params.push(filters.serving_date_from);
    conditions.push(`serving_date >= $${params.length}`);
  }
  if (filters.serving_date_to) {
    params.push(filters.serving_date_to);
    conditions.push(`serving_date <= $${params.length}`);
  }

  const where = conditions.length > 0 ? `WHERE ${conditions.join(' AND ')}` : '';

  const result = await db.query<{ status: OrderStatus; count: string }>(
    `SELECT status, COUNT(*) AS count
     FROM orders
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
    `INSERT INTO orders (
       company_id, recipe_id, created_by, order_source, status,
       meal_category, amount, serving_date, shipment_date,
       frozen, chilled, recurring, client_note
     )
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
     RETURNING
       id, company_id, recipe_id, created_by, order_source, status,
       meal_category, amount, serving_date, shipment_date,
       frozen, chilled, recurring, client_note, created_at`,
    [
      data.company_id,
      data.recipe_id ?? null,
      data.created_by ?? null,
      data.order_source,
      data.status ?? 'pending',
      data.meal_category ?? null,
      data.amount,
      data.serving_date,
      data.shipment_date,
      data.frozen ?? false,
      data.chilled ?? false,
      data.recurring ?? false,
      data.client_note ?? null,
    ]
  );
  return result.rows[0];
}

export async function updateOrder(
  db: Pool,
  id: string,
  data: UpdateOrder
): Promise<Order | null> {
  const result = await db.query<Order>(
    `UPDATE orders
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
       client_note   = COALESCE($10, client_note)
     WHERE id = $11
     RETURNING
       id, company_id, recipe_id, created_by, order_source, status,
       meal_category, amount, serving_date, shipment_date,
       frozen, chilled, recurring, client_note, created_at`,
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
      data.client_note ?? null,
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

export async function confirmOrder(db: Pool, id: string): Promise<Order | null> {
  const result = await db.query<Order>(
    `UPDATE orders
     SET status = 'confirmed'
     WHERE id = $1 AND status = 'pending'
     RETURNING
       id, company_id, recipe_id, created_by, order_source, status,
       meal_category, amount, serving_date, shipment_date,
       frozen, chilled, recurring, client_note, created_at`,
    [id]
  );
  // Returns null if the order wasn't in 'pending' state
  return result.rows[0] ?? null;
}

export async function fulfillOrder(db: Pool, id: string): Promise<Order | null> {
  const result = await db.query<Order>(
    `UPDATE orders
     SET status = 'fulfilled'
     WHERE id = $1 AND status = 'confirmed'
     RETURNING
       id, company_id, recipe_id, created_by, order_source, status,
       meal_category, amount, serving_date, shipment_date,
       frozen, chilled, recurring, client_note, created_at`,
    [id]
  );
  return result.rows[0] ?? null;
}

export async function cancelOrder(db: Pool, id: string): Promise<Order | null> {
  const result = await db.query<Order>(
    `UPDATE orders
     SET status = 'cancelled'
     WHERE id = $1 AND status IN ('pending', 'confirmed')
     RETURNING
       id, company_id, recipe_id, created_by, order_source, status,
       meal_category, amount, serving_date, shipment_date,
       frozen, chilled, recurring, client_note, created_at`,
    [id]
  );
  // Returns null if the order was already fulfilled or cancelled
  return result.rows[0] ?? null;
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
        `($${i * 13 + 1}, $${i * 13 + 2}, $${i * 13 + 3}, $${i * 13 + 4}, $${i * 13 + 5},
          $${i * 13 + 6}, $${i * 13 + 7}, $${i * 13 + 8}, $${i * 13 + 9}, $${i * 13 + 10},
          $${i * 13 + 11}, $${i * 13 + 12}, $${i * 13 + 13})`
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
    o.serving_date,
    o.shipment_date,
    o.frozen ?? false,
    o.chilled ?? false,
    o.recurring ?? false,
    o.client_note ?? null,
  ]);

  const result = await client.query<Order>(
    `INSERT INTO orders (
       company_id, recipe_id, created_by, order_source, status,
       meal_category, amount, serving_date, shipment_date,
       frozen, chilled, recurring, client_note
     )
     VALUES ${values}
     RETURNING
       id, company_id, recipe_id, created_by, order_source, status,
       meal_category, amount, serving_date, shipment_date,
       frozen, chilled, recurring, client_note, created_at`,
    params
  );

  return result.rows;
}
