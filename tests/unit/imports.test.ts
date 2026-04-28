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
    storage: 'frozen',
    notes: null,
  };

  it('accepts a fully valid row', () => {
    expect(ExcelRowSchema.safeParse(valid).success).toBe(true);
  });

  it('accepts notes as undefined and coerces to null', () => {
    const { notes, ...withoutNotes } = valid;
    const result = ExcelRowSchema.safeParse(withoutNotes);

    expect(result.success).toBe(true);
    if (result.success) expect(result.data.notes).toBeNull();
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
    for (const meal_category of ['Breakfast', 'Lunch', 'Snack', 'Soup', 'Dinner']) {
      expect(ExcelRowSchema.safeParse({ ...valid, meal_category }).success).toBe(true);
    }
  });

  it('rejects an invalid storage value', () => {
    const result = ExcelRowSchema.safeParse({ ...valid, storage: 'ambient' });
    expect(result.success).toBe(false);
  });

  it('accepts both valid storage values', () => {
    expect(ExcelRowSchema.safeParse({ ...valid, storage: 'frozen' }).success).toBe(true);
    expect(ExcelRowSchema.safeParse({ ...valid, storage: 'chilled' }).success).toBe(true);
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
      if (sql.includes('FROM companies') && sql.includes('LOWER')) {
        return { rows: company ? [company] : [] };
      }
      // getRecipeByName
      if (sql.includes('FROM recipes') && sql.includes('LOWER')) {
        return { rows: recipe ? [recipe] : [] };
      }
      return { rows: [] };
    },
    connect: async () => ({
      query: async (sql: string) => {
        // insertOrdersBatch
        if (sql.includes('INSERT INTO orders')) {
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
  storage: 'frozen',
  notes: null,
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
      'Company':        'Acme Corp',
      'Recipe':         'Pasta Primavera',
      'Serving Date':   '2025-06-15',
      'Shipment Date':  '2025-06-10',
      'Meal Category':  'Lunch',
      'Amount':         10,
      'Storage':        'frozen',
      'Notes':          null,
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

  it('maps alternative header variants (Meal Type, ServingDate)', async () => {
    const rowWithAltHeaders = {
      'Company':      'Acme Corp',
      'Recipe':       'Pasta Primavera',
      'ServingDate':  '2025-06-15',
      'ShipmentDate': '2025-06-10',
      'Meal Type':    'Lunch',
      'Amount':       10,
      'Storage':      'frozen',
      'Notes':        null,
    };

    const result = await importOrdersFromRows(
      makeMockDb() as any,
      [rowWithAltHeaders],
      'user-1',
      Object.keys(rowWithAltHeaders)
    );

    expect(result.errors).toHaveLength(0);
    expect(result.inserted).toBe(1);
  });
});

describe('importOrdersFromRows — storage mapping', () => {
  it('maps storage: frozen to frozen: true, chilled: false', async () => {
    // Verify the mapping is correct by checking what gets inserted.
    // We intercept the insertOrdersBatch call via the mock.
    let capturedOrders: any[] = [];

    const db = {
      query: async (sql: string) => {
        if (sql.includes('FROM companies')) return { rows: [{ id: 'c', name: 'Acme', active: true }] };
        if (sql.includes('FROM recipes')) return { rows: [{ id: 'r', name: 'Pasta', active: true }] };
        return { rows: [] };
      },
      connect: async () => ({
        query: async (sql: string, params?: unknown[]) => {
          if (sql.includes('INSERT INTO orders')) {
            // params are flat — frozen is at index 9 (0-based) per insertOrdersBatch param order
            capturedOrders = params as any[];
          }
          return { rows: [{ id: 'order-uuid' }] };
        },
        release: () => {},
      }),
    };

    await importOrdersFromRows(db as any, [{ ...validRow, storage: 'frozen' }], 'user-1');

    // frozen is param index 9, chilled is 10 (0-based) in the flat params array
    expect(capturedOrders[9]).toBe(true);  // frozen
    expect(capturedOrders[10]).toBe(false); // chilled
  });

  it('maps storage: chilled to frozen: false, chilled: true', async () => {
    let capturedOrders: any[] = [];

    const db = {
      query: async (sql: string) => {
        if (sql.includes('FROM companies')) return { rows: [{ id: 'c', name: 'Acme', active: true }] };
        if (sql.includes('FROM recipes')) return { rows: [{ id: 'r', name: 'Pasta', active: true }] };
        return { rows: [] };
      },
      connect: async () => ({
        query: async (sql: string, params?: unknown[]) => {
          if (sql.includes('INSERT INTO orders')) capturedOrders = params as any[];
          return { rows: [{ id: 'order-uuid' }] };
        },
        release: () => {},
      }),
    };

    await importOrdersFromRows(db as any, [{ ...validRow, storage: 'chilled' }], 'user-1');

    expect(capturedOrders[9]).toBe(false);  // frozen
    expect(capturedOrders[10]).toBe(true);  // chilled
  });
});
