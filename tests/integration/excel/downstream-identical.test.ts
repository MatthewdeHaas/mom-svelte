import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import fs from 'fs';
import path from 'path';
import { parseExcelBufferAsyncWithMeta } from '$lib/excel/parse';
import { importOrdersFromRows } from '$lib/server/queries/imports';
import { createCompany } from '$lib/server/queries/companies';
import { createRecipe } from '$lib/server/queries/recipes';
import { generateProductionOrderSheet } from '$lib/excel/export/production-order';
import { generateShoppingList } from '$lib/excel/export/shopping-list';
import { generatePackingSlips } from '$lib/pdf/packing-slip';
import { createTestContext } from '../core/helpers';
import { semanticHashXlsx, semanticHashPdf } from './_semantic';
import { registerCrashCleanup } from './_crash-cleanup';
import type { InternalCaller } from '$lib/server/api-client';

// Test-only caller — threaded through the three export fns so the Rust-dispatch
// branch is actually entered when `use_rust_backend_exports` is on. The user id
// matches the one seeded in beforeAll; role=admin so Rust /api/exports/* accepts.
// companyId=null is the admin global scope.
const TEST_CALLER: InternalCaller = {
  userId: `excel-ds-user-${Date.now()}`,
  role: 'admin',
  companyId: null,
};

const FIXTURE_DIR = 'tests/integration/excel/fixtures';
const MESSY = path.join(FIXTURE_DIR, 'real-client-week-3.xlsx');
const CLEAN = path.join(FIXTURE_DIR, 'real-client-week-3-clean.xlsx');

const ctx = createTestContext();

// importOrdersFromRows writes `created_by` which FKs to "user".id —
// seed one user per suite and pass its id instead of a bare string.
const USER_ID = `excel-ds-user-${Date.now()}`;

beforeAll(async () => {
  await ctx.setup();
  // Pre-cleanup: a prior run's crash can leave TDS% rows behind, and this
  // suite seeds under a fresh TDS%${Date.now()}_ prefix so the orphans
  // wouldn't collide with us — but they'd still poison orders.test.ts's
  // "empty" assertions. Scrub proactively.
  await ctx.pool.query(
    `DELETE FROM "order" WHERE company_id IN (SELECT id FROM company WHERE name LIKE 'TDS%')`,
  );
  await ctx.pool.query(`DELETE FROM recipe  WHERE name LIKE 'TDS%'`);
  await ctx.pool.query(`DELETE FROM company WHERE name LIKE 'TDS%'`);
  // Backstop for SIGINT/SIGTERM — vitest's afterAll doesn't run on signal.
  registerCrashCleanup(ctx.pool, 'TDS');
  await ctx.pool.query(
    `INSERT INTO "user" (id, name, email, "emailVerified", role, "createdAt", "updatedAt")
     VALUES ($1, 'Excel DS Test', $2, true, 'user', now(), now())`,
    [USER_ID, `${USER_ID}@test.local`],
  );
});
afterAll(async () => {
  // Guard each cleanup so a partial beforeAll (ctx.setup threw, or a seed
  // step died) still gets us as close to clean as possible before teardown.
  try { await deleteAll(); } catch { /* best effort */ }
  try {
    await ctx.pool.query(`DELETE FROM "user" WHERE id = $1`, [USER_ID]);
  } catch { /* best effort */ }
  try { await ctx.teardown(); } catch { /* best effort */ }
});

// ---------------------------------------------------------------------------
// Shape: this is the end-to-end determinism guard.
//
//   1. Seed companies + recipes ONCE under a shared namespace so both import
//      paths resolve to the same ids. (If messy and clean used different
//      namespaces, their exports would sort/key differently and byte-equal
//      hashes would be impossible.)
//   2. Import messy → generate production/shopping/slips → sha256 each.
//   3. Delete only the orders (keep companies + recipes) → import clean
//      → generate the same three → sha256 each.
//   4. Compare messy hashes vs clean hashes.
//   5. Delete orders again → re-import clean → generate again → compare
//      vs the first clean run (second-run determinism).
//
// Date range: the real-client fixture ships week 3 orders. The sanitizer
// preserves the original Serving/Shipment dates, so we query a wide window
// that covers them all. We derive the actual min/max from the imported
// rows so the test survives future fixture refreshes.
// ---------------------------------------------------------------------------

async function loadNamespaced(file: string, ns: string) {
  const buf = fs.readFileSync(file);
  const { rows, headers, extraSheetCount } =
    await parseExcelBufferAsyncWithMeta(buf);

  const customerKey = headers.find((h) => h.trim().toLowerCase() === 'customer')
    ?? headers.find((h) => h.trim().toLowerCase() === 'company');
  const recipeKey = headers.find((h) => h.trim().toLowerCase() === 'recipe');
  const itemKey = headers.find((h) => h.trim().toLowerCase() === 'item');

  if (!customerKey || !recipeKey) {
    throw new Error(`fixture ${file} missing Customer/Recipe header`);
  }

  const companies = new Set<string>();
  const recipes = new Set<string>();

  for (const row of rows) {
    const c = row[customerKey];
    const r = row[recipeKey];
    // Trim before prefixing — createCompany / createRecipe .trim() their
    // input, so a prefix with trailing whitespace would miss the seed.
    if (typeof c === 'string' && c.trim()) {
      const prefixed = `${ns}${c.trim()}`;
      row[customerKey] = prefixed;
      companies.add(prefixed);
    }
    if (typeof r === 'string' && r.trim()) {
      const prefixed = `${ns}${r.trim()}`;
      row[recipeKey] = prefixed;
      recipes.add(prefixed);
      // Keep Item in sync when it originally agreed with Recipe, so the
      // asymmetric prefix doesn't manufacture mismatches.
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

// ---------------------------------------------------------------------------
// NS + lifecycle
// ---------------------------------------------------------------------------

const NS = `TDS${Date.now()}_`;

async function deleteOrdersOnly(): Promise<void> {
  await ctx.pool.query(
    `DELETE FROM "order" WHERE company_id IN (SELECT id FROM company WHERE name LIKE $1)`,
    [`${NS}%`],
  );
}

async function deleteAll(): Promise<void> {
  await deleteOrdersOnly();
  await ctx.pool.query(`DELETE FROM recipe  WHERE name LIKE $1`, [`${NS}%`]);
  await ctx.pool.query(`DELETE FROM company WHERE name LIKE $1`, [`${NS}%`]);
}

async function fetchDateBounds(): Promise<{ from: string; to: string }> {
  const { rows } = await ctx.pool.query<{ min: Date; max: Date }>(
    `SELECT MIN(serving_date)::date AS min, MAX(serving_date)::date AS max
     FROM "order"
     WHERE company_id IN (SELECT id FROM company WHERE name LIKE $1)`,
    [`${NS}%`],
  );
  const toYMD = (d: Date) => d.toISOString().slice(0, 10);
  return { from: toYMD(rows[0].min), to: toYMD(rows[0].max) };
}

interface ExportHashes {
  productionOrder: string;
  shoppingList: string;
  packingSlips: { company: string; sha256: string }[];
}

async function generateHashes(): Promise<ExportHashes> {
  const { from, to } = await fetchDateBounds();

  const [prod, shop, slips] = await Promise.all([
    generateProductionOrderSheet(ctx.pool, from, to, TEST_CALLER),
    generateShoppingList(ctx.pool, from, to, TEST_CALLER),
    // packing slips use week-of(referenceDate) bounds in EST — pass the
    // 'from' date which is guaranteed to be inside the data week.
    generatePackingSlips(ctx.pool, new Date(`${from}T12:00:00Z`), TEST_CALLER),
  ]);

  const packingSlips = await Promise.all(
    slips.map(async (s) => ({
      company: s.companyName,
      sha256: semanticHashPdf(s.buffer),
    })),
  );

  return {
    productionOrder: await semanticHashXlsx(prod),
    shoppingList: await semanticHashXlsx(shop),
    packingSlips: packingSlips
      // Sort for stable comparison — Map iteration order would otherwise
      // vary with insertion order (same input, same companies, but any
      // reordering would make the test flaky).
      .sort((a, b) => a.company.localeCompare(b.company)),
  };
}

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

let messyHashes: ExportHashes;
let cleanHashesRun1: ExportHashes;
let cleanHashesRun2: ExportHashes;

beforeAll(async () => {
  // Seed companies + recipes under NS by scanning the messy fixture first
  // (identical rowset to the clean fixture — same 12 companies + 79 recipes).
  const messyFx = await loadNamespaced(MESSY, NS);
  for (const name of messyFx.companies) await createCompany(ctx.pool, { name });
  for (const name of messyFx.recipes)   await createRecipe(ctx.pool, { name });

  // Pass 1 — messy
  const messyResult = await importOrdersFromRows(
    ctx.pool,
    messyFx.rows,
    USER_ID,
    messyFx.headers,
    messyFx.extraSheetCount,
  );
  if (messyResult.errors.length > 0) {
    throw new Error(
      `messy import produced errors: ${JSON.stringify(messyResult.errors.slice(0, 3))}`,
    );
  }
  messyHashes = await generateHashes();

  // Pass 2 — clean (same orders, different source format)
  await deleteOrdersOnly();
  const cleanFx = await loadNamespaced(CLEAN, NS);
  const cleanResult1 = await importOrdersFromRows(
    ctx.pool,
    cleanFx.rows,
    USER_ID,
    cleanFx.headers,
    cleanFx.extraSheetCount,
  );
  if (cleanResult1.errors.length > 0) {
    throw new Error(
      `clean import produced errors: ${JSON.stringify(cleanResult1.errors.slice(0, 3))}`,
    );
  }
  cleanHashesRun1 = await generateHashes();

  // Pass 3 — clean again (second-run determinism)
  await deleteOrdersOnly();
  const cleanFx2 = await loadNamespaced(CLEAN, NS);
  const cleanResult2 = await importOrdersFromRows(
    ctx.pool,
    cleanFx2.rows,
    USER_ID,
    cleanFx2.headers,
    cleanFx2.extraSheetCount,
  );
  if (cleanResult2.errors.length > 0) {
    throw new Error(
      `clean re-import produced errors: ${JSON.stringify(cleanResult2.errors.slice(0, 3))}`,
    );
  }
  cleanHashesRun2 = await generateHashes();
}, 120_000);

// Note: suite-level afterAll above handles cleanup — no need for a second
// delete pass here (the top-level afterAll calls deleteAll() first).

// ---------------------------------------------------------------------------
// Cross-format equivalence — messy and clean produce byte-identical exports
// ---------------------------------------------------------------------------

describe('messy ⇄ clean produce byte-identical downstream exports', () => {
  it('production order sheet sha256 matches', () => {
    expect(cleanHashesRun1.productionOrder).toBe(messyHashes.productionOrder);
  });

  it('shopping list sha256 matches', () => {
    expect(cleanHashesRun1.shoppingList).toBe(messyHashes.shoppingList);
  });

  it('packing slips: same count, same companies, same sha256 per company', () => {
    expect(cleanHashesRun1.packingSlips).toEqual(messyHashes.packingSlips);
  });
});

// ---------------------------------------------------------------------------
// Re-run determinism — same input twice produces the same bytes
// ---------------------------------------------------------------------------

describe('clean import re-run produces byte-identical exports', () => {
  it('production order sheet sha256 is stable across runs', () => {
    expect(cleanHashesRun2.productionOrder).toBe(cleanHashesRun1.productionOrder);
  });

  it('shopping list sha256 is stable across runs', () => {
    expect(cleanHashesRun2.shoppingList).toBe(cleanHashesRun1.shoppingList);
  });

  it('packing slip sha256s are stable across runs', () => {
    expect(cleanHashesRun2.packingSlips).toEqual(cleanHashesRun1.packingSlips);
  });
});
