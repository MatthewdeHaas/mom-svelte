import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import fs from 'fs';
import path from 'path';
import { parseExcelBufferAsyncWithMeta } from '$lib/excel/parse';
import { importOrdersFromRows } from '$lib/server/queries/imports';
import { createCompany } from '$lib/server/queries/companies';
import { createRecipe } from '$lib/server/queries/recipes';
import { createTestContext } from '../core/helpers';
import { registerCrashCleanup } from './_crash-cleanup';
import type { ImportWarning, ImportWarningCode } from '$lib/types';

const FIXTURE_DIR = 'tests/integration/excel/fixtures';
const MESSY   = path.join(FIXTURE_DIR, 'real-client-week-3.xlsx');
const CLEAN   = path.join(FIXTURE_DIR, 'real-client-week-3-clean.xlsx');

const ctx = createTestContext();

// A single user is seeded for the whole suite. importOrdersFromRows writes
// `created_by` which FKs to "user".id — a hardcoded 'test-user' string
// hits orders_created_by_fkey. One row covers both describe blocks.
const USER_ID = `excel-rc-user-${Date.now()}`;

// Suite-level namespace prefix — see normalization.test.ts for rationale.
// `TRC` is distinct from `TDS` and `TN` so pre-cleanup never crosses suites.
const SUITE_PREFIX = `TRC${Date.now()}_`;

beforeAll(async () => {
  await ctx.setup();
  // Pre-cleanup: scrub any `TRC%` orphans a prior crashed run left behind.
  await ctx.pool.query(
    `DELETE FROM "order" WHERE company_id IN (SELECT id FROM company WHERE name LIKE 'TRC%')`,
  );
  await ctx.pool.query(`DELETE FROM recipe  WHERE name LIKE 'TRC%'`);
  await ctx.pool.query(`DELETE FROM company WHERE name LIKE 'TRC%'`);
  registerCrashCleanup(ctx.pool, 'TRC');
  await ctx.pool.query(
    `INSERT INTO "user" (id, name, email, "emailVerified", role, "createdAt", "updatedAt")
     VALUES ($1, 'Excel RC Test', $2, true, 'user', now(), now())`,
    [USER_ID, `${USER_ID}@test.local`],
  );
});
afterAll(async () => {
  try {
    await ctx.pool.query(`DELETE FROM "user" WHERE id = $1`, [USER_ID]);
  } catch { /* best effort */ }
  try { await ctx.teardown(); } catch { /* best effort */ }
});

// ---------------------------------------------------------------------------
// Namespace helpers
//
// importOrdersFromRows takes a Pool and runs its INSERT inside its own
// transaction via pool.connect() — those inserts do NOT ride ctx.client's
// rollback. So each test seeds companies/recipes with a unique prefix,
// rewrites the Customer/Recipe cells in the parsed rows to use that prefix,
// imports via ctx.pool, then explicitly deletes everything under the prefix
// in afterAll.
// ---------------------------------------------------------------------------

let nsCounter = 0;
function freshNamespace(): string {
  // Extends SUITE_PREFIX so the crash-cleanup backstop covers every row
  // this file ever seeds.
  return `${SUITE_PREFIX}${++nsCounter}_`;
}

async function cleanupNamespace(ns: string): Promise<void> {
  // Orders → recipes → companies. Join prefix on the parent name so we
  // never delete another test's rows that happen to share a user id.
  await ctx.pool.query(
    `DELETE FROM "order" WHERE company_id IN (SELECT id FROM company WHERE name LIKE $1)`,
    [`${ns}%`],
  );
  await ctx.pool.query(`DELETE FROM recipe  WHERE name LIKE $1`, [`${ns}%`]);
  await ctx.pool.query(`DELETE FROM company WHERE name LIKE $1`, [`${ns}%`]);
}

/**
 * Reads the real-client fixture, prefixes every Customer and Recipe cell
 * with `ns`, and returns the row list + headers + extra-sheet count ready
 * to pass to importOrdersFromRows.
 */
async function loadFixtureNamespaced(
  file: string,
  ns: string,
): Promise<{
  rows: Record<string, unknown>[];
  headers: string[];
  extraSheetCount: number;
  companies: Set<string>; // namespaced names actually used
  recipes: Set<string>;
}> {
  const buf = fs.readFileSync(file);
  const { rows, headers, extraSheetCount } =
    await parseExcelBufferAsyncWithMeta(buf);

  const companies = new Set<string>();
  const recipes = new Set<string>();

  // Find the headers that carry Customer / Recipe strings. Match the
  // real-client fixture's exact capitalisation ("Customer", "Recipe")
  // but fall back to lowercase variants so this helper works for the
  // synthetic clean file too.
  const customerKey = headers.find((h) => h.trim().toLowerCase() === 'customer')
    ?? headers.find((h) => h.trim().toLowerCase() === 'company');
  const recipeKey = headers.find((h) => h.trim().toLowerCase() === 'recipe');
  // Item column — may or may not be present. If it is, we need to rewrite
  // it in lockstep with Recipe so the importer's item/recipe matcher sees
  // the same prefixed string on both sides (otherwise every row triggers
  // a spurious item_recipe_mismatch warning).
  const itemKey = headers.find((h) => h.trim().toLowerCase() === 'item');

  if (!customerKey || !recipeKey) {
    throw new Error(
      `fixture ${file} missing Customer/Recipe header (got: ${headers.join(', ')})`,
    );
  }

  for (const row of rows) {
    const c = row[customerKey];
    const r = row[recipeKey];
    // Trim before prefixing — createCompany / createRecipe trim their
    // input so seeded names are always trim()'d. If we prefixed a cell
    // with trailing whitespace, the DB lookup would miss the seeded row.
    if (typeof c === 'string' && c.trim()) {
      const prefixed = `${ns}${c.trim()}`;
      row[customerKey] = prefixed;
      companies.add(prefixed);
    }
    if (typeof r === 'string' && r.trim()) {
      const prefixed = `${ns}${r.trim()}`;
      row[recipeKey] = prefixed;
      recipes.add(prefixed);
      // Keep Item in sync with Recipe when the two originally agreed —
      // otherwise the prefix-only-on-Recipe asymmetry would turn every
      // Item cell into a mismatch. When Item ≠ Recipe (the real-client
      // row 41 "Banana" vs "Melon" case), leave it alone so the real
      // mismatch warning still fires.
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
  for (const name of companies) {
    await createCompany(ctx.pool, { name });
  }
  for (const name of recipes) {
    await createRecipe(ctx.pool, { name });
  }
}

// ---------------------------------------------------------------------------
// Fixture metrics (confirmed by eyeballing the sanitized fixture)
// ---------------------------------------------------------------------------
//
//   sheet 1 "Prod W3": 350 data rows, 13 columns
//   sheets 2–7: label + packing-slip templates (ignored by the importer)
//
//   12 unique customers, 79 unique recipes
//   21 rows marked Frozen only  → storage='f'
//   321 rows marked Chilled only → storage='c'
//     8 rows neither marked      → storage='c' + storage_missing_defaulted_chilled
//     0 rows both marked          → no storage_both_set
//     1 row  Item ≠ Recipe        → row 41 ("Banana" vs "Melon")
//     0 zero-amount rows (sanitizer patched the one real-client typo)
//     0 meal_case_normalized rows (meal values already canonical)
//
// ---------------------------------------------------------------------------

describe('real-client-week-3.xlsx (messy, structurally faithful)', () => {
  let ns: string;
  let result: Awaited<ReturnType<typeof importOrdersFromRows>>;
  let warnings: ImportWarning[];

  beforeAll(async () => {
    ns = freshNamespace();
    const fx = await loadFixtureNamespaced(MESSY, ns);
    await seed(fx.companies, fx.recipes);

    result = await importOrdersFromRows(
      ctx.pool,
      fx.rows,
      USER_ID,
      fx.headers,
      fx.extraSheetCount,
    );
    warnings = result.warnings;
  });

  afterAll(async () => cleanupNamespace(ns));

  it('inserts all 350 data rows with no errors', () => {
    expect(result.errors).toEqual([]);
    expect(result.inserted).toBe(350);
    expect(result.skipped).toBe(0);
  });

  // -------------------------------------------------------------
  // Sheet-level warnings — one each, row === null
  // -------------------------------------------------------------

  it('emits extra_sheets_ignored (sheet-level, count=6)', () => {
    const w = warnings.filter((w) => w.code === 'extra_sheets_ignored');
    expect(w).toHaveLength(1);
    expect(w[0].row).toBeNull();
    expect(w[0].details).toMatchObject({ count: 6 });
  });

  it('emits item_column_ignored (sheet-level)', () => {
    const w = warnings.filter((w) => w.code === 'item_column_ignored');
    expect(w).toHaveLength(1);
    expect(w[0].row).toBeNull();
  });

  it('emits qty_formula_ignored (sheet-level)', () => {
    const w = warnings.filter((w) => w.code === 'qty_formula_ignored');
    expect(w).toHaveLength(1);
    expect(w[0].row).toBeNull();
  });

  it('does not emit boilerplate_row_stopped (sheet 1 has no trailer)', () => {
    const w = warnings.filter((w) => w.code === 'boilerplate_row_stopped');
    expect(w).toHaveLength(0);
  });

  // -------------------------------------------------------------
  // Row-level warnings
  // -------------------------------------------------------------

  it('defaults 8 rows with neither Frozen nor Chilled to chilled', () => {
    const w = warnings.filter(
      (w) => w.code === 'storage_missing_defaulted_chilled',
    );
    expect(w).toHaveLength(8);
    expect(w.every((e) => typeof e.row === 'number' && e.row >= 2)).toBe(true);
  });

  it('does not emit storage_both_set (no rows have both marked)', () => {
    const w = warnings.filter((w) => w.code === 'storage_both_set');
    expect(w).toHaveLength(0);
  });

  it('emits exactly one item_recipe_mismatch (row 41, Banana vs Melon)', () => {
    const w = warnings.filter((w) => w.code === 'item_recipe_mismatch');
    expect(w).toHaveLength(1);
    expect(w[0].row).toBe(41);
    // Recipe gets ns-prefixed by loadFixtureNamespaced; Item intentionally doesn't.
    const details = w[0].details as { item: string; recipe: string };
    expect(details.item).toBe('Banana');
    expect(details.recipe).toMatch(/_Melon$/);
  });

  it('does not emit meal_case_normalized (meal values already canonical)', () => {
    const w = warnings.filter((w) => w.code === 'meal_case_normalized');
    expect(w).toHaveLength(0);
  });

  // -------------------------------------------------------------
  // Taxonomy guard — nothing we didn't expect
  // -------------------------------------------------------------

  it('warning codes are drawn from the expected set only', () => {
    const expected: ImportWarningCode[] = [
      'extra_sheets_ignored',
      'item_column_ignored',
      'qty_formula_ignored',
      'storage_missing_defaulted_chilled',
      'item_recipe_mismatch',
    ];
    const seen = new Set(warnings.map((w) => w.code));
    for (const code of seen) {
      expect(expected).toContain(code);
    }
  });
});

// ---------------------------------------------------------------------------
// Clean counterpart — same 350 orders, clean 10-column template.
// No warnings expected (except possibly extra_sheets_ignored = 0 which the
// importer only emits when count > 0, so none).
// ---------------------------------------------------------------------------

describe('real-client-week-3-clean.xlsx (clean template)', () => {
  let ns: string;
  let result: Awaited<ReturnType<typeof importOrdersFromRows>>;

  beforeAll(async () => {
    ns = freshNamespace();
    const fx = await loadFixtureNamespaced(CLEAN, ns);
    await seed(fx.companies, fx.recipes);

    result = await importOrdersFromRows(
      ctx.pool,
      fx.rows,
      USER_ID,
      fx.headers,
      fx.extraSheetCount,
    );
  });

  afterAll(async () => cleanupNamespace(ns));

  it('inserts all 350 data rows with no errors', () => {
    expect(result.errors).toEqual([]);
    expect(result.inserted).toBe(350);
  });

  it('emits no warnings — clean path is warning-free', () => {
    expect(result.warnings).toEqual([]);
  });
});
