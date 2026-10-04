import { describe, it, expect } from 'vitest';
import { ExcelRowSchema, importOrdersFromRows } from '$lib/server/queries/imports';

// ---------------------------------------------------------------------------
// ExcelRowSchema
// ---------------------------------------------------------------------------

describe('ExcelRowSchema', () => {
  const valid = {
    company: 'Acme Corp',
    recipe: 'Pasta Primavera',
    serving_date: '2025-06-15',
    shipment_date: '2025-06-10',
    meal_category: 'Lunch',
    amount: 10,
    portion_size: 100,
    day: 2,
    storage: 'f',
    client: null,
  };

  it('accepts a fully valid row', () => {
    expect(ExcelRowSchema.safeParse(valid).success).toBe(true);
  });

  it('accepts client as undefined and coerces to null', () => {
    const { client, ...withoutClient } = valid;
    const result = ExcelRowSchema.safeParse(withoutClient);

    expect(result.success).toBe(true);
    if (result.success) expect(result.data.client).toBeNull();
  });

  it('accepts portion_size as null/undefined', () => {
    const result = ExcelRowSchema.safeParse({ ...valid, portion_size: null });
    expect(result.success).toBe(true);
    if (result.success) expect(result.data.portion_size).toBeNull();
  });

  it('accepts day as null/undefined', () => {
    const result = ExcelRowSchema.safeParse({ ...valid, day: null });
    expect(result.success).toBe(true);
    if (result.success) expect(result.data.day).toBeNull();
  });

  it('rejects day outside 1-7 range', () => {
    expect(ExcelRowSchema.safeParse({ ...valid, day: 0 }).success).toBe(false);
    expect(ExcelRowSchema.safeParse({ ...valid, day: 8 }).success).toBe(false);
  });

  it('accepts storage as null (ambient)', () => {
    const result = ExcelRowSchema.safeParse({ ...valid, storage: null });
    expect(result.success).toBe(true);
    if (result.success) expect(result.data.storage).toBeNull();
  });

  it('accepts both valid storage values', () => {
    expect(ExcelRowSchema.safeParse({ ...valid, storage: 'f' }).success).toBe(true);
    expect(ExcelRowSchema.safeParse({ ...valid, storage: 'c' }).success).toBe(true);
  });

  it('rejects an invalid storage value', () => {
    const result = ExcelRowSchema.safeParse({ ...valid, storage: 'ambient' });
    expect(result.success).toBe(false);
  });

  it('accepts serving_date equal to shipment_date', () => {
    const result = ExcelRowSchema.safeParse({
      ...valid,
      serving_date: '2025-06-15',
      shipment_date: '2025-06-15',
    });
    expect(result.success).toBe(true);
  });

  it('rejects shipment_date after serving_date', () => {
    const result = ExcelRowSchema.safeParse({
      ...valid,
      serving_date: '2025-06-10',
      shipment_date: '2025-06-15',
    });

    expect(result.success).toBe(false);
    if (!result.success) {
      const fields = result.error.issues.map((i) => i.path[0]);
      expect(fields).toContain('shipment_date');
    }
  });

  it('rejects a non-ISO serving_date', () => {
    const result = ExcelRowSchema.safeParse({ ...valid, serving_date: '15/06/2025' });
    expect(result.success).toBe(false);
  });

  it('rejects a non-ISO shipment_date', () => {
    const result = ExcelRowSchema.safeParse({ ...valid, shipment_date: 'June 10 2025' });
    expect(result.success).toBe(false);
  });

  it('rejects an invalid meal_category', () => {
    const result = ExcelRowSchema.safeParse({ ...valid, meal_category: 'Brunch' });
    expect(result.success).toBe(false);
  });

  it('accepts all valid meal_category values', () => {
    for (const meal_category of ['Breakfast', 'Lunch', 'Snack', 'Snack R', 'Soup', 'Dinner']) {
      expect(ExcelRowSchema.safeParse({ ...valid, meal_category }).success).toBe(true);
    }
  });

  it('rejects amount of zero', () => {
    const result = ExcelRowSchema.safeParse({ ...valid, amount: 0 });
    expect(result.success).toBe(false);
  });

  it('rejects a negative amount', () => {
    const result = ExcelRowSchema.safeParse({ ...valid, amount: -5 });
    expect(result.success).toBe(false);
  });

  it('rejects a non-numeric amount', () => {
    const result = ExcelRowSchema.safeParse({ ...valid, amount: 'ten' });
    expect(result.success).toBe(false);
  });

  it('rejects a missing company', () => {
    const { company, ...rest } = valid;
    expect(ExcelRowSchema.safeParse(rest).success).toBe(false);
  });

  it('rejects an empty company string', () => {
    expect(ExcelRowSchema.safeParse({ ...valid, company: '' }).success).toBe(false);
  });

  it('rejects a missing recipe', () => {
    const { recipe, ...rest } = valid;
    expect(ExcelRowSchema.safeParse(rest).success).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// importOrdersFromRows — orchestration logic (mocked DB)
// ---------------------------------------------------------------------------

// Minimal Pool mock — we only need query() and connect() to behave correctly.
// The mock never hits a real DB; we control what company/recipe lookups return.
function makeMockDb({
  company = { id: 'company-uuid', name: 'Acme Corp', active: true },
  recipe = { id: 'recipe-uuid', name: 'Pasta Primavera', active: true },
}: {
  company?: object | null;
  recipe?: object | null;
} = {}) {
  return {
    query: async (sql: string, params?: unknown[]) => {
      // getCompanyByName
      if (sql.includes('FROM company') && sql.includes('LOWER')) {
        return { rows: company ? [company] : [] };
      }
      // getRecipeByName
      if (sql.includes('FROM recipe') && sql.includes('LOWER')) {
        return { rows: recipe ? [recipe] : [] };
      }
      return { rows: [] };
    },
    connect: async () => ({
      query: async (sql: string) => {
        // insertOrdersBatch — table renamed to "order" in migration 004
        if (sql.includes('INSERT INTO "order"')) {
          return {
            rows: [{ id: 'order-uuid', status: 'pending' }],
          };
        }
        return { rows: [] };
      },
      release: () => {},
    }),
  };
}

const validRow = {
  company: 'Acme Corp',
  recipe: 'Pasta Primavera',
  serving_date: '2025-06-15',
  shipment_date: '2025-06-10',
  meal_category: 'Lunch',
  amount: 10,
  portion_size: 100,
  day: 2,
  storage: 'f',
  client: null,
};

describe('importOrdersFromRows — empty input', () => {
  it('returns zero counts with no errors for an empty array', async () => {
    const result = await importOrdersFromRows(makeMockDb() as any, [], 'user-1');

    expect(result.inserted).toBe(0);
    expect(result.errors).toHaveLength(0);
  });
});

describe('importOrdersFromRows — Zod validation phase', () => {
  it('collects errors from ALL rows before aborting, not just the first', async () => {
    const result = await importOrdersFromRows(makeMockDb() as any, [
      { ...validRow, amount: -1 },      // row 2 — bad amount
      { ...validRow, meal_category: 'Brunch' }, // row 3 — bad category
    ], 'user-1');

    expect(result.inserted).toBe(0);
    expect(result.errors.length).toBeGreaterThanOrEqual(2);

    const rowNumbers = result.errors.map((e) => e.row);
    expect(rowNumbers).toContain(2);
    expect(rowNumbers).toContain(3);
  });

  it('aborts with no inserts when any row fails validation', async () => {
    const result = await importOrdersFromRows(makeMockDb() as any, [
      validRow,
      { ...validRow, amount: -1 },
    ], 'user-1');

    expect(result.inserted).toBe(0);
    expect(result.errors.length).toBeGreaterThan(0);
  });

  it('reports errors with the correct 1-based row number (offset by header row)', async () => {
    const result = await importOrdersFromRows(makeMockDb() as any, [
      { ...validRow, amount: -1 },
    ], 'user-1');

    // First data row is row 2 (row 1 is the header)
    expect(result.errors[0].row).toBe(2);
  });

  it('reports the correct field name on a validation error', async () => {
    const result = await importOrdersFromRows(makeMockDb() as any, [
      { ...validRow, meal_category: 'Brunch' },
    ], 'user-1');

    expect(result.errors[0].field).toBe('meal_category');
  });
});

describe('importOrdersFromRows — reference resolution phase', () => {
  it('returns an error when the company is not found', async () => {
    const result = await importOrdersFromRows(
      makeMockDb({ company: null }) as any,
      [validRow],
      'user-1'
    );

    expect(result.inserted).toBe(0);
    expect(result.errors.some((e) => e.field === 'company')).toBe(true);
  });

  it('returns an error when the company is inactive', async () => {
    const result = await importOrdersFromRows(
      makeMockDb({ company: { id: 'c-id', name: 'Acme Corp', active: false } }) as any,
      [validRow],
      'user-1'
    );

    expect(result.inserted).toBe(0);
    expect(result.errors.some((e) => e.field === 'company')).toBe(true);
  });

  it('returns an error when the recipe is not found', async () => {
    const result = await importOrdersFromRows(
      makeMockDb({ recipe: null }) as any,
      [validRow],
      'user-1'
    );

    expect(result.inserted).toBe(0);
    expect(result.errors.some((e) => e.field === 'recipe')).toBe(true);
  });

  it('collects errors from all rows in the resolution phase too', async () => {
    const result = await importOrdersFromRows(
      makeMockDb({ company: null }) as any,
      [validRow, validRow],
      'user-1'
    );

    const rowNumbers = result.errors.map((e) => e.row);
    expect(rowNumbers).toContain(2);
    expect(rowNumbers).toContain(3);
  });
});

describe('importOrdersFromRows — header normalisation', () => {
  it('maps friendly display headers to schema field names', async () => {
    const rowWithDisplayHeaders = {
      'Customer':       'Acme Corp',
      'Recipe':         'Pasta Primavera',
      'Serving Date':   '2025-06-15',
      'Shipment Date':  '2025-06-10',
      'Meal':           'Lunch',
      'Order Amount':   10,
      'Portion Size':   100,
      'Day':            2,
      'Storage':        'f',
      'Client':         null,
    };

    const result = await importOrdersFromRows(
      makeMockDb() as any,
      [rowWithDisplayHeaders],
      'user-1',
      Object.keys(rowWithDisplayHeaders)
    );

    // If header normalisation works, Zod validation passes and we get an insert
    expect(result.errors).toHaveLength(0);
    expect(result.inserted).toBe(1);
  });

  it('ignores the Item column (duplicate of Recipe)', async () => {
    const rowWithItem = {
      'Customer':       'Acme Corp',
      'Recipe':         'Pasta Primavera',
      'Item':           'Pasta Primavera',  // should be ignored
      'Serving Date':   '2025-06-15',
      'Shipment Date':  '2025-06-10',
      'Meal':           'Lunch',
      'Order Amount':   10,
      'Storage':        'c',
      'Client':         null,
    };

    const result = await importOrdersFromRows(
      makeMockDb() as any,
      [rowWithItem],
      'user-1',
      Object.keys(rowWithItem)
    );

    expect(result.errors).toHaveLength(0);
    expect(result.inserted).toBe(1);
  });

  it('maps storage f/c to frozen/chilled booleans in the insert', async () => {
    let capturedParams: any[] = [];

    const db = {
      query: async (sql: string) => {
        if (sql.includes('FROM company')) return { rows: [{ id: 'c', name: 'Acme', active: true }] };
        if (sql.includes('FROM recipe')) return { rows: [{ id: 'r', name: 'Pasta', active: true }] };
        return { rows: [] };
      },
      connect: async () => ({
        query: async (sql: string, params?: unknown[]) => {
          if (sql.includes('INSERT INTO "order"')) {
            capturedParams = params as any[];
          }
          return { rows: [{ id: 'order-uuid' }] };
        },
        release: () => {},
      }),
    };

    await importOrdersFromRows(db as any, [{ ...validRow, storage: 'f' }], 'user-1');

    // frozen is at index 11, chilled is 12 in the 15-param insertOrdersBatch layout
    expect(capturedParams[11]).toBe(true);  // frozen
    expect(capturedParams[12]).toBe(false); // chilled
  });

  it('maps storage c to frozen: false, chilled: true', async () => {
    let capturedParams: any[] = [];

    const db = {
      query: async (sql: string) => {
        if (sql.includes('FROM company')) return { rows: [{ id: 'c', name: 'Acme', active: true }] };
        if (sql.includes('FROM recipe')) return { rows: [{ id: 'r', name: 'Pasta', active: true }] };
        return { rows: [] };
      },
      connect: async () => ({
        query: async (sql: string, params?: unknown[]) => {
          if (sql.includes('INSERT INTO "order"')) capturedParams = params as any[];
          return { rows: [{ id: 'order-uuid' }] };
        },
        release: () => {},
      }),
    };

    await importOrdersFromRows(db as any, [{ ...validRow, storage: 'c' }], 'user-1');

    expect(capturedParams[11]).toBe(false);  // frozen
    expect(capturedParams[12]).toBe(true);   // chilled
  });
});
