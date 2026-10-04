import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import fs from 'fs';
import path from 'path';
import { parseExcelBufferAsyncWithMeta } from '$lib/excel/parse';
import { importOrdersFromRows } from '$lib/server/queries/imports';
import { createCompany } from '$lib/server/queries/companies';
import { createRecipe } from '$lib/server/queries/recipes';
import { createTestContext } from '../core/helpers';
import type { ImportResult, ImportWarningCode } from '$lib/types';

const FIXTURE_DIR = 'tests/integration/excel/fixtures';

const ctx = createTestContext();

// importOrdersFromRows writes `created_by` which FKs to "user".id —
// seed one user per suite and pass its id instead of a bare string.
const USER_ID = `excel-norm-user-${Date.now()}`;

beforeAll(async () => {
  await ctx.setup();
  await ctx.pool.query(
    `INSERT INTO "user" (id, name, email, "emailVerified", role, "createdAt", "updatedAt")
     VALUES ($1, 'Excel Norm Test', $2, true, 'user', now(), now())`,
    [USER_ID, `${USER_ID}@test.local`],
  );
});
afterAll(async () => {
  await ctx.pool.query(`DELETE FROM "user" WHERE id = $1`, [USER_ID]);
  await ctx.teardown();
});

// ---------------------------------------------------------------------------
// Harness
//
// The synthetic fixtures reference a small handful of hard-coded Customer /
// Recipe names (Acme Co, Chicken Curry, Beef Stew). We seed those under a
// per-test namespace prefix, rewrite the parsed row cells to match, run
// the import against ctx.pool, and clean up by prefix in afterAll.
// Pattern mirrors tests/integration/excel/real-client.test.ts.
// ---------------------------------------------------------------------------

let nsCounter = 0;
function freshNamespace(): string {
  return `T${Date.now()}${++nsCounter}_`;
}

async function cleanupNamespace(ns: string): Promise<void> {
  await ctx.pool.query(
    `DELETE FROM "order" WHERE company_id IN (SELECT id FROM company WHERE name LIKE $1)`,
    [`${ns}%`],
  );
  await ctx.pool.query(`DELETE FROM recipe  WHERE name LIKE $1`, [`${ns}%`]);
  await ctx.pool.query(`DELETE FROM company WHERE name LIKE $1`, [`${ns}%`]);
}

async function loadNamespaced(file: string, ns: string) {
  const buf = fs.readFileSync(path.join(FIXTURE_DIR, file));
  const { rows, headers, extraSheetCount } =
    await parseExcelBufferAsyncWithMeta(buf);

  const customerKey = headers.find((h) => h.trim().toLowerCase() === 'customer')
    ?? headers.find((h) => h.trim().toLowerCase() === 'company');
  const recipeKey = headers.find((h) => h.trim().toLowerCase() === 'recipe');

  if (!customerKey || !recipeKey) {
    throw new Error(`fixture ${file} missing Customer/Recipe header`);
  }

  const companies = new Set<string>();
  const recipes = new Set<string>();

  const itemKey = headers.find((h) => h.trim().toLowerCase() === 'item');

  for (const row of rows) {
    const c = row[customerKey];
    const r = row[recipeKey];
    if (typeof c === 'string' && c.trim()) {
      // Skip boilerplate markers — those cells aren't real customer refs
      // and we don't want to seed companies named "PACKING SLIP".
      const lower = c.trim().toLowerCase();
      const isBoilerplate =
        lower === 'packing slip' ||
        lower === 'office use only' ||
        lower.startsWith('cambros') ||
        lower.startsWith('packed by') ||
        lower.startsWith('ice packs');
      if (!isBoilerplate) {
        const prefixed = `${ns}${c.trim()}`;
        row[customerKey] = prefixed;
        companies.add(prefixed);
      }
    }
    if (typeof r === 'string' && r.trim()) {
      const prefixed = `${ns}${r.trim()}`;
      row[recipeKey] = prefixed;
      recipes.add(prefixed);
      // Keep Item in sync with Recipe when they originally agreed —
      // otherwise prefix-only-on-Recipe would turn every row into a
      // spurious item_recipe_mismatch. The genuine mismatch rows
      // (e.g. "Banana" vs "Melon") have different strings, so they
      // skip this branch and the real warning still fires.
      if (itemKey) {
        const i = row[itemKey];
        if (typeof i === 'string' && i.trim() && i.trim() === r.trim()) {
          row[itemKey] = prefixed;
        }
      }
    }
  }

  return { rows, headers, extraSheetCount, companies, recipes };
}

async function seed(companies: Iterable<string>, recipes: Iterable<string>) {
  for (const name of companies) await createCompany(ctx.pool, { name });
  for (const name of recipes)   await createRecipe(ctx.pool, { name });
}

async function runImport(file: string): Promise<{
  ns: string;
  result: ImportResult;
}> {
  const ns = freshNamespace();
  const fx = await loadNamespaced(file, ns);
  await seed(fx.companies, fx.recipes);
  const result = await importOrdersFromRows(
    ctx.pool,
    fx.rows,
    USER_ID,
    fx.headers,
    fx.extraSheetCount,
  );
  return { ns, result };
}

/** Count warnings of a given code. */
function count(result: ImportResult, code: ImportWarningCode): number {
  return result.warnings.filter((w) => w.code === code).length;
}

// ---------------------------------------------------------------------------
// frozen-chilled-split.xlsx
//
// 4 rows exercising every Frozen/Chilled combination:
//   row 2: Frozen only  → storage='f', no warning
//   row 3: Chilled only → storage='c', no warning
//   row 4: both marked  → storage='f', storage_both_set
//   row 5: neither      → storage='c', storage_missing_defaulted_chilled
// ---------------------------------------------------------------------------

describe('frozen-chilled-split.xlsx', () => {
  let ns: string;
  let result: ImportResult;

  beforeAll(async () => {
    ({ ns, result } = await runImport('frozen-chilled-split.xlsx'));
  });
  afterAll(async () => cleanupNamespace(ns));

  it('inserts all 4 rows with no errors', () => {
    expect(result.errors).toEqual([]);
    expect(result.inserted).toBe(4);
  });

  it('emits one storage_both_set warning (row 4)', () => {
    const w = result.warnings.filter((w) => w.code === 'storage_both_set');
    expect(w).toHaveLength(1);
    expect(w[0].row).toBe(4);
  });

  it('emits one storage_missing_defaulted_chilled warning (row 5)', () => {
    const w = result.warnings.filter(
      (w) => w.code === 'storage_missing_defaulted_chilled',
    );
    expect(w).toHaveLength(1);
    expect(w[0].row).toBe(5);
  });

  it('does not emit storage warnings for unambiguous rows (2, 3)', () => {
    const rows = result.warnings
      .filter((w) =>
        w.code === 'storage_both_set' ||
        w.code === 'storage_missing_defaulted_chilled',
      )
      .map((w) => w.row);
    expect(rows).not.toContain(2);
    expect(rows).not.toContain(3);
  });
});

// ---------------------------------------------------------------------------
// item-column-duplicate.xlsx
//
// 2 rows with an Item column present:
//   row 2: Item === Recipe → no mismatch warning
//   row 3: Item !== Recipe → item_recipe_mismatch (Recipe wins)
// ---------------------------------------------------------------------------

describe('item-column-duplicate.xlsx', () => {
  let ns: string;
  let result: ImportResult;

  beforeAll(async () => {
    ({ ns, result } = await runImport('item-column-duplicate.xlsx'));
  });
  afterAll(async () => cleanupNamespace(ns));

  it('inserts both rows with no errors', () => {
    expect(result.errors).toEqual([]);
    expect(result.inserted).toBe(2);
  });

  it('emits item_column_ignored once (sheet-level)', () => {
    const w = result.warnings.filter((w) => w.code === 'item_column_ignored');
    expect(w).toHaveLength(1);
    expect(w[0].row).toBeNull();
  });

  it('emits item_recipe_mismatch only for the disagreeing row (row 3)', () => {
    const w = result.warnings.filter((w) => w.code === 'item_recipe_mismatch');
    expect(w).toHaveLength(1);
    expect(w[0].row).toBe(3);
    // Recipe gets the test namespace prefix via loadNamespaced; Item does not.
    const details = w[0].details as { item: string; recipe: string };
    expect(details.item).toBe('Melon');
    expect(details.recipe).toMatch(/_Beef Stew$/);
  });
});

// ---------------------------------------------------------------------------
// boilerplate-rows.xlsx
//
// 2 real orders followed by 3 trailer rows (PACKING SLIP, OFFICE USE ONLY,
// Cambros). Trailer rows must be dropped and a single boilerplate_row_stopped
// warning emitted naming the first trailer row (row 4).
// ---------------------------------------------------------------------------

describe('boilerplate-rows.xlsx', () => {
  let ns: string;
  let result: ImportResult;

  beforeAll(async () => {
    ({ ns, result } = await runImport('boilerplate-rows.xlsx'));
  });
  afterAll(async () => cleanupNamespace(ns));

  it('stops at row 4 and inserts the 2 real orders only', () => {
    expect(result.errors).toEqual([]);
    expect(result.inserted).toBe(2);
  });

  it('emits exactly one boilerplate_row_stopped warning', () => {
    const w = result.warnings.filter((w) => w.code === 'boilerplate_row_stopped');
    expect(w).toHaveLength(1);
    expect(w[0].row).toBe(4);
    expect(w[0].details).toMatchObject({ stopped_at: 4 });
  });

  it('does not emit resolution errors for the dropped trailer rows', () => {
    // If the stop logic failed, "PACKING SLIP" / "OFFICE USE ONLY" would
    // surface as "Company not found" row errors — guard against regression.
    expect(result.errors.map((e) => e.message)).not.toContain(
      expect.stringContaining('PACKING SLIP'),
    );
  });
});

// ---------------------------------------------------------------------------
// zero-amount.xlsx
//
// Single row with Order Amount = 0. Zod `.positive()` rejects it as a
// regular error (not a warning). Pins the "zero amounts aren't valid orders"
// contract — if we ever change that, the fixture updates with it.
// ---------------------------------------------------------------------------

describe('zero-amount.xlsx', () => {
  let ns: string;
  let result: ImportResult;

  beforeAll(async () => {
    ({ ns, result } = await runImport('zero-amount.xlsx'));
  });
  afterAll(async () => cleanupNamespace(ns));

  it('rejects the row — nothing inserted', () => {
    expect(result.inserted).toBe(0);
  });

  it('surfaces the rejection as a row-level amount error, not a warning', () => {
    expect(result.errors).toHaveLength(1);
    expect(result.errors[0].row).toBe(2);
    expect(result.errors[0].field).toBe('amount');
    expect(count(result, 'meal_case_normalized')).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// meal-case-variants.xlsx
//
// Row 2: " lunch"  → "Lunch"   (normalized)
// Row 3: "lunch r" → "Lunch R" (normalized)
// Row 4: "Dinner"  → unchanged (no warning)
// ---------------------------------------------------------------------------

describe('meal-case-variants.xlsx', () => {
  let ns: string;
  let result: ImportResult;

  beforeAll(async () => {
    ({ ns, result } = await runImport('meal-case-variants.xlsx'));
  });
  afterAll(async () => cleanupNamespace(ns));

  it('inserts all 3 rows with no errors', () => {
    expect(result.errors).toEqual([]);
    expect(result.inserted).toBe(3);
  });

  it('emits two meal_case_normalized warnings (rows 2 and 3)', () => {
    const w = result.warnings
      .filter((w) => w.code === 'meal_case_normalized')
      .sort((a, b) => (a.row ?? 0) - (b.row ?? 0));
    expect(w).toHaveLength(2);
    expect(w[0].row).toBe(2);
    expect(w[0].details).toMatchObject({ to: 'Lunch' });
    expect(w[1].row).toBe(3);
    expect(w[1].details).toMatchObject({ to: 'Lunch R' });
  });

  it('does not normalize the already-canonical row 4', () => {
    const rows = result.warnings
      .filter((w) => w.code === 'meal_case_normalized')
      .map((w) => w.row);
    expect(rows).not.toContain(4);
  });
});

// ---------------------------------------------------------------------------
// extra-sheets.xlsx
//
// Sheet 1 is valid (1 order). Sheets 2 and 3 are boilerplate templates
// that the importer must ignore while emitting a single sheet-level
// extra_sheets_ignored warning with count=2.
// ---------------------------------------------------------------------------

describe('extra-sheets.xlsx', () => {
  let ns: string;
  let result: ImportResult;

  beforeAll(async () => {
    ({ ns, result } = await runImport('extra-sheets.xlsx'));
  });
  afterAll(async () => cleanupNamespace(ns));

  it('inserts the single sheet-1 row with no errors', () => {
    expect(result.errors).toEqual([]);
    expect(result.inserted).toBe(1);
  });

  it('emits one extra_sheets_ignored warning with count=2', () => {
    const w = result.warnings.filter((w) => w.code === 'extra_sheets_ignored');
    expect(w).toHaveLength(1);
    expect(w[0].row).toBeNull();
    expect(w[0].details).toMatchObject({ count: 2 });
  });
});
