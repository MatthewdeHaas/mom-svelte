import { describe, it, expect, beforeAll, afterAll, beforeEach, afterEach } from 'vitest';
import {
  getOrders,
  getOrderById,
  getOrderCountsByStatus,
  createOrder,
  updateOrder,
  confirmOrder,
  fulfillOrder,
  cancelOrder,
  insertOrdersBatch,
} from '$lib/server/queries/orders';
import { createCompany } from '$lib/server/queries/companies';
import { createRecipe } from '$lib/server/queries/recipes';
import { createTestContext } from './helpers';
import type { InsertOrder } from '$lib/types';

const ctx = createTestContext();

beforeAll(ctx.setup);
afterAll(ctx.teardown);
beforeEach(ctx.begin);
afterEach(ctx.rollback);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

let counter = 0;
function uniqueName(prefix = 'Co') {
  return `${prefix}_${++counter}_${Date.now()}`;
}

const BASE_SERVING = new Date('2025-06-15');
const BASE_SHIPMENT = new Date('2025-06-10');

async function seedCompany() {
  return createCompany(ctx.client, { name: uniqueName('Company') });
}

async function seedRecipe() {
  return createRecipe(ctx.client, { name: uniqueName('Recipe') });
}

async function seedOrder(overrides: Partial<InsertOrder> = {}) {
  const company = await seedCompany();
  return createOrder(ctx.client, {
    company_id: company.id,
    order_source: 'admin_manual',
    amount: 10,
    serving_date: BASE_SERVING,
    shipment_date: BASE_SHIPMENT,
    ...overrides,
  });
}

// ---------------------------------------------------------------------------
// getOrders
// ---------------------------------------------------------------------------

describe('getOrders', () => {
  it('returns an empty array when no orders exist', async () => {
    expect(await getOrders(ctx.client)).toEqual([]);
  });

  it('returns all orders when no filters applied', async () => {
    const o1 = await seedOrder();
    const o2 = await seedOrder();

    const ids = (await getOrders(ctx.client)).map((o) => o.id);

    expect(ids).toContain(o1.id);
    expect(ids).toContain(o2.id);
  });

  it('includes company_name on each row', async () => {
    await seedOrder();
    const rows = await getOrders(ctx.client);

    expect(rows.every((r) => typeof r.company_name === 'string')).toBe(true);
  });

  it('filters by company_id', async () => {
    const c1 = await seedCompany();
    const c2 = await seedCompany();

    const o1 = await createOrder(ctx.client, {
      company_id: c1.id,
      order_source: 'admin_manual',
      amount: 5,
      serving_date: BASE_SERVING,
      shipment_date: BASE_SHIPMENT,
    });
    await createOrder(ctx.client, {
      company_id: c2.id,
      order_source: 'admin_manual',
      amount: 5,
      serving_date: BASE_SERVING,
      shipment_date: BASE_SHIPMENT,
    });

    const rows = await getOrders(ctx.client, { company_id: c1.id });
    const ids = rows.map((r) => r.id);

    expect(ids).toContain(o1.id);
    expect(ids).toHaveLength(1);
  });

  it('filters by status', async () => {
    const pending = await seedOrder();
    const confirmed = await seedOrder();
    await confirmOrder(ctx.client, confirmed.id);

    const rows = await getOrders(ctx.client, { status: 'pending' });
    const ids = rows.map((r) => r.id);

    expect(ids).toContain(pending.id);
    expect(ids).not.toContain(confirmed.id);
  });

  it('filters by recipe_id', async () => {
    const recipe = await seedRecipe();
    const company = await seedCompany();

    const withRecipe = await createOrder(ctx.client, {
      company_id: company.id,
      recipe_id: recipe.id,
      order_source: 'admin_manual',
      amount: 5,
      serving_date: BASE_SERVING,
      shipment_date: BASE_SHIPMENT,
    });
    const withoutRecipe = await seedOrder();

    const rows = await getOrders(ctx.client, { recipe_id: recipe.id });
    const ids = rows.map((r) => r.id);

    expect(ids).toContain(withRecipe.id);
    expect(ids).not.toContain(withoutRecipe.id);
  });

  it('filters by serving_date_from and serving_date_to', async () => {
    const company = await seedCompany();

    const early = await createOrder(ctx.client, {
      company_id: company.id,
      order_source: 'admin_manual',
      amount: 5,
      serving_date: new Date('2025-01-10'),
      shipment_date: new Date('2025-01-05'),
    });
    const late = await createOrder(ctx.client, {
      company_id: company.id,
      order_source: 'admin_manual',
      amount: 5,
      serving_date: new Date('2025-12-20'),
      shipment_date: new Date('2025-12-15'),
    });

    const rows = await getOrders(ctx.client, {
      serving_date_from: new Date('2025-06-01'),
      serving_date_to: new Date('2025-06-30'),
    });
    const ids = rows.map((r) => r.id);

    expect(ids).not.toContain(early.id);
    expect(ids).not.toContain(late.id);
  });

  it('filters by frozen', async () => {
    const company = await seedCompany();

    const frozen = await createOrder(ctx.client, {
      company_id: company.id,
      order_source: 'admin_manual',
      amount: 5,
      serving_date: BASE_SERVING,
      shipment_date: BASE_SHIPMENT,
      frozen: true,
    });
    const notFrozen = await createOrder(ctx.client, {
      company_id: company.id,
      order_source: 'admin_manual',
      amount: 5,
      serving_date: BASE_SERVING,
      shipment_date: BASE_SHIPMENT,
      frozen: false,
    });

    const rows = await getOrders(ctx.client, { frozen: true });
    const ids = rows.map((r) => r.id);

    expect(ids).toContain(frozen.id);
    expect(ids).not.toContain(notFrozen.id);
  });

  it('returns results ordered by serving_date DESC then company name ASC', async () => {
    const c1 = await createCompany(ctx.client, { name: 'AAA Company' });
    const c2 = await createCompany(ctx.client, { name: 'ZZZ Company' });

    await createOrder(ctx.client, {
      company_id: c2.id,
      order_source: 'admin_manual',
      amount: 5,
      serving_date: new Date('2025-06-15'),
      shipment_date: new Date('2025-06-10'),
    });
    await createOrder(ctx.client, {
      company_id: c1.id,
      order_source: 'admin_manual',
      amount: 5,
      serving_date: new Date('2025-06-15'),
      shipment_date: new Date('2025-06-10'),
    });

    const rows = await getOrders(ctx.client);
    const names = rows.map((r) => r.company_name);
    const aaaIdx = names.indexOf('AAA Company');
    const zzzIdx = names.indexOf('ZZZ Company');

    expect(aaaIdx).toBeLessThan(zzzIdx);
  });
});

// ---------------------------------------------------------------------------
// getOrderById
// ---------------------------------------------------------------------------

describe('getOrderById', () => {
  it('returns null for a non-existent id', async () => {
    expect(
      await getOrderById(ctx.client, '00000000-0000-0000-0000-000000000000')
    ).toBeNull();
  });

  it('returns the order with embedded company', async () => {
    const company = await seedCompany();
    const order = await createOrder(ctx.client, {
      company_id: company.id,
      order_source: 'admin_manual',
      amount: 5,
      serving_date: BASE_SERVING,
      shipment_date: BASE_SHIPMENT,
    });

    const found = await getOrderById(ctx.client, order.id);

    expect(found).not.toBeNull();
    expect(found!.id).toBe(order.id);
    expect(found!.company.id).toBe(company.id);
    expect(found!.company.name).toBe(company.name);
  });

  it('returns recipe as null when no recipe is set', async () => {
    const order = await seedOrder();
    const found = await getOrderById(ctx.client, order.id);

    expect(found!.recipe).toBeNull();
  });

  it('returns embedded recipe when one is set', async () => {
    const company = await seedCompany();
    const recipe = await seedRecipe();

    const order = await createOrder(ctx.client, {
      company_id: company.id,
      recipe_id: recipe.id,
      order_source: 'admin_manual',
      amount: 5,
      serving_date: BASE_SERVING,
      shipment_date: BASE_SHIPMENT,
    });

    const found = await getOrderById(ctx.client, order.id);

    expect(found!.recipe).not.toBeNull();
    expect(found!.recipe!.id).toBe(recipe.id);
    expect(found!.recipe!.name).toBe(recipe.name);
  });

  it('returns amount as a number', async () => {
    const order = await seedOrder({ amount: 42.5 });
    const found = await getOrderById(ctx.client, order.id);

    expect(typeof found!.amount).toBe('number');
    expect(found!.amount).toBeCloseTo(42.5);
  });
});

// ---------------------------------------------------------------------------
// getOrderCountsByStatus
// ---------------------------------------------------------------------------

describe('getOrderCountsByStatus', () => {
  it('returns zero counts for all statuses when no orders exist', async () => {
    const counts = await getOrderCountsByStatus(ctx.client);

    expect(counts.pending).toBe(0);
    expect(counts.confirmed).toBe(0);
    expect(counts.fulfilled).toBe(0);
    expect(counts.cancelled).toBe(0);
  });

  it('counts orders correctly by status', async () => {
    const o1 = await seedOrder();
    const o2 = await seedOrder();
    const o3 = await seedOrder();

    await confirmOrder(ctx.client, o2.id);
    await confirmOrder(ctx.client, o3.id);
    await fulfillOrder(ctx.client, o3.id);

    const counts = await getOrderCountsByStatus(ctx.client);

    expect(counts.pending).toBe(1);
    expect(counts.confirmed).toBe(1);
    expect(counts.fulfilled).toBe(1);
    expect(counts.cancelled).toBe(0);
  });

  it('filters counts by serving_date range', async () => {
    const company = await seedCompany();

    await createOrder(ctx.client, {
      company_id: company.id,
      order_source: 'admin_manual',
      amount: 5,
      serving_date: new Date('2025-01-10'),
      shipment_date: new Date('2025-01-05'),
    });
    await createOrder(ctx.client, {
      company_id: company.id,
      order_source: 'admin_manual',
      amount: 5,
      serving_date: new Date('2025-06-15'),
      shipment_date: new Date('2025-06-10'),
    });

    const counts = await getOrderCountsByStatus(ctx.client, {
      serving_date_from: new Date('2025-06-01'),
      serving_date_to: new Date('2025-06-30'),
    });

    expect(counts.pending).toBe(1);
  });
});

// ---------------------------------------------------------------------------
// createOrder
// ---------------------------------------------------------------------------

describe('createOrder', () => {
  it('creates an order with default status of pending', async () => {
    const order = await seedOrder();
    expect(order.status).toBe('pending');
  });

  it('returns the order with a string id', async () => {
    const order = await seedOrder();
    expect(typeof order.id).toBe('string');
  });

  it('defaults frozen, chilled, recurring to false', async () => {
    const order = await seedOrder();

    expect(order.frozen).toBe(false);
    expect(order.chilled).toBe(false);
    expect(order.recurring).toBe(false);
  });

  it('respects explicit status', async () => {
    const order = await seedOrder({ status: 'confirmed' });
    expect(order.status).toBe('confirmed');
  });

  it('throws when shipment_date is after serving_date (CHECK constraint)', async () => {
    const company = await seedCompany();

    await expect(
      createOrder(ctx.client, {
        company_id: company.id,
        order_source: 'admin_manual',
        amount: 5,
        serving_date: new Date('2025-06-10'),
        shipment_date: new Date('2025-06-15'), // after serving_date — invalid
      })
    ).rejects.toThrow();
  });

  it('allows shipment_date equal to serving_date', async () => {
    const company = await seedCompany();
    const sameDay = new Date('2025-06-15');

    const order = await createOrder(ctx.client, {
      company_id: company.id,
      order_source: 'admin_manual',
      amount: 5,
      serving_date: sameDay,
      shipment_date: sameDay,
    });

    expect(order.id).toBeDefined();
  });
});

// ---------------------------------------------------------------------------
// updateOrder
// ---------------------------------------------------------------------------

describe('updateOrder', () => {
  it('returns null for a non-existent id', async () => {
    expect(
      await updateOrder(ctx.client, '00000000-0000-0000-0000-000000000000', { amount: 5 })
    ).toBeNull();
  });

  it('updates amount only, leaving other fields unchanged', async () => {
    const order = await seedOrder({ frozen: true });

    const updated = await updateOrder(ctx.client, order.id, { amount: 99 });

    expect(updated!.frozen).toBe(true); // unchanged
    expect(parseFloat(updated!.amount as unknown as string)).toBeCloseTo(99);
  });

  it('updates multiple fields at once', async () => {
    const order = await seedOrder();

    const updated = await updateOrder(ctx.client, order.id, {
      frozen: true,
      chilled: true,
      client: 'Handle with care',
    });

    expect(updated!.frozen).toBe(true);
    expect(updated!.chilled).toBe(true);
    expect(updated!.client).toBe('Handle with care');
  });

  it('change is reflected in subsequent read', async () => {
    const order = await seedOrder();
    await updateOrder(ctx.client, order.id, { client: 'Updated note' });

    const found = await getOrderById(ctx.client, order.id);
    expect(found!.client).toBe('Updated note');
  });
});

// ---------------------------------------------------------------------------
// confirmOrder
// ---------------------------------------------------------------------------

describe('confirmOrder', () => {
  it('transitions a pending order to confirmed', async () => {
    const order = await seedOrder();
    const result = await confirmOrder(ctx.client, order.id);

    expect(result).not.toBeNull();
    expect(result!.status).toBe('confirmed');
  });

  it('returns null when order is not in pending state', async () => {
    const order = await seedOrder();
    await confirmOrder(ctx.client, order.id); // now confirmed

    const second = await confirmOrder(ctx.client, order.id);
    expect(second).toBeNull();
  });

  it('returns null for a fulfilled order', async () => {
    const order = await seedOrder();
    await confirmOrder(ctx.client, order.id);
    await fulfillOrder(ctx.client, order.id);

    expect(await confirmOrder(ctx.client, order.id)).toBeNull();
  });

  it('returns null for a cancelled order', async () => {
    const order = await seedOrder();
    await cancelOrder(ctx.client, order.id);

    expect(await confirmOrder(ctx.client, order.id)).toBeNull();
  });

  it('returns null for a non-existent id', async () => {
    expect(
      await confirmOrder(ctx.client, '00000000-0000-0000-0000-000000000000')
    ).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// fulfillOrder
// ---------------------------------------------------------------------------

describe('fulfillOrder', () => {
  it('transitions a confirmed order to fulfilled', async () => {
    const order = await seedOrder();
    await confirmOrder(ctx.client, order.id);

    const result = await fulfillOrder(ctx.client, order.id);

    expect(result).not.toBeNull();
    expect(result!.status).toBe('fulfilled');
  });

  it('returns null when order is still pending (must confirm first)', async () => {
    const order = await seedOrder();

    expect(await fulfillOrder(ctx.client, order.id)).toBeNull();
  });

  it('returns null when order is already fulfilled', async () => {
    const order = await seedOrder();
    await confirmOrder(ctx.client, order.id);
    await fulfillOrder(ctx.client, order.id);

    expect(await fulfillOrder(ctx.client, order.id)).toBeNull();
  });

  it('returns null for a cancelled order', async () => {
    const order = await seedOrder();
    await cancelOrder(ctx.client, order.id);

    expect(await fulfillOrder(ctx.client, order.id)).toBeNull();
  });

  it('returns null for a non-existent id', async () => {
    expect(
      await fulfillOrder(ctx.client, '00000000-0000-0000-0000-000000000000')
    ).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// cancelOrder
// ---------------------------------------------------------------------------

describe('cancelOrder', () => {
  it('cancels a pending order', async () => {
    const order = await seedOrder();
    const result = await cancelOrder(ctx.client, order.id);

    expect(result).not.toBeNull();
    expect(result!.status).toBe('cancelled');
  });

  it('cancels a confirmed order', async () => {
    const order = await seedOrder();
    await confirmOrder(ctx.client, order.id);

    const result = await cancelOrder(ctx.client, order.id);

    expect(result).not.toBeNull();
    expect(result!.status).toBe('cancelled');
  });

  it('returns null for an already cancelled order', async () => {
    const order = await seedOrder();
    await cancelOrder(ctx.client, order.id);

    expect(await cancelOrder(ctx.client, order.id)).toBeNull();
  });

  it('returns null for a fulfilled order', async () => {
    const order = await seedOrder();
    await confirmOrder(ctx.client, order.id);
    await fulfillOrder(ctx.client, order.id);

    expect(await cancelOrder(ctx.client, order.id)).toBeNull();
  });

  it('returns null for a non-existent id', async () => {
    expect(
      await cancelOrder(ctx.client, '00000000-0000-0000-0000-000000000000')
    ).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// insertOrdersBatch
// ---------------------------------------------------------------------------

describe('insertOrdersBatch', () => {
  it('returns an empty array when given no orders', async () => {
    expect(await insertOrdersBatch(ctx.client, [])).toEqual([]);
  });

  it('inserts multiple orders and returns them all', async () => {
    const company = await seedCompany();

    const orders = await insertOrdersBatch(ctx.client, [
      {
        company_id: company.id,
        order_source: 'excel_import',
        amount: 5,
        serving_date: BASE_SERVING,
        shipment_date: BASE_SHIPMENT,
      },
      {
        company_id: company.id,
        order_source: 'excel_import',
        amount: 10,
        serving_date: BASE_SERVING,
        shipment_date: BASE_SHIPMENT,
      },
    ]);

    expect(orders).toHaveLength(2);
    expect(orders.every((o) => typeof o.id === 'string')).toBe(true);
  });

  it('all inserted orders default to pending status', async () => {
    const company = await seedCompany();

    const orders = await insertOrdersBatch(ctx.client, [
      {
        company_id: company.id,
        order_source: 'excel_import',
        amount: 5,
        serving_date: BASE_SERVING,
        shipment_date: BASE_SHIPMENT,
      },
    ]);

    expect(orders[0].status).toBe('pending');
  });

  it('inserted orders are retrievable via getOrders', async () => {
    const company = await seedCompany();

    const [inserted] = await insertOrdersBatch(ctx.client, [
      {
        company_id: company.id,
        order_source: 'excel_import',
        amount: 5,
        serving_date: BASE_SERVING,
        shipment_date: BASE_SHIPMENT,
      },
    ]);

    const rows = await getOrders(ctx.client, { company_id: company.id });
    expect(rows.some((r) => r.id === inserted.id)).toBe(true);
  });

  it('throws and rolls back on a constraint violation (bad shipment_date)', async () => {
    const company = await seedCompany();

    await expect(
      insertOrdersBatch(ctx.client, [
        {
          company_id: company.id,
          order_source: 'excel_import',
          amount: 5,
          serving_date: new Date('2025-06-10'),
          shipment_date: new Date('2025-06-15'), // violates CHECK
        },
      ])
    ).rejects.toThrow();
  });
});
