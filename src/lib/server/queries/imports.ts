import { z } from 'zod';
import type { Pool } from 'pg';
import type {
  ExcelRowParsed,
  ExcelRowResolved,
  ImportResult,
  ImportRowError,
  ImportWarning,
  ImportWarningCode,
  InsertOrder,
} from '$lib/types';
import { getCompanyByName } from './companies';
import { getRecipeByName } from './recipes';
import { insertOrdersBatch } from './orders';
import type { Db } from '$lib/server/db';

// -------------------------------------------------------------
// ZOD SCHEMA
// Validates a raw row off the sheet. All fields come in as
// unknown — Zod coerces and validates in one pass.
// -------------------------------------------------------------

// 'Lunch R' and 'Soup R' are real values in the client's weekly workbook.
// They differ from Lunch/Soup only by a trailing "R" (meaning unclear —
// surfaced through unchanged so downstream still groups by meal_category).
const MEAL_CATEGORIES = ['Breakfast', 'Lunch', 'Lunch R', 'Snack', 'Snack R', 'Soup', 'Soup R', 'Dinner'] as const;
const STORAGE_VALUES = ['f', 'c'] as const;

const ISO_DATE = /^\d{4}-\d{2}-\d{2}$/;

export const ExcelRowSchema = z.object({
  company: z
    .string({ error: 'Company is required' })
    .min(1, 'Company cannot be empty'),

  recipe: z
    .string({ error: 'Recipe is required' })
    .min(1, 'Recipe cannot be empty'),

  serving_date: z
    .string({ error: 'Serving date is required' })
    .regex(ISO_DATE, 'Serving date must be in YYYY-MM-DD format'),

  shipment_date: z
    .string({ error: 'Shipment date is required' })
    .regex(ISO_DATE, 'Shipment date must be in YYYY-MM-DD format'),

  meal_category: z.enum(MEAL_CATEGORIES, {
    error: `Meal category must be one of: ${MEAL_CATEGORIES.join(', ')}`,
  }),

  amount: z
    .number({ error: 'Amount is required' })
    .positive('Amount must be greater than zero'),

  portion_size: z
    .number()
    .positive('Portion size must be greater than zero')
    .optional()
    .nullable()
    .transform((v) => v ?? null),

  day: z
    .number()
    .int('Day must be a whole number')
    .min(1, 'Day must be between 1 and 7')
    .max(7, 'Day must be between 1 and 7')
    .optional()
    .nullable()
    .transform((v) => v ?? null),

  storage: z
    .enum(STORAGE_VALUES, {
      error: `Storage must be one of: ${STORAGE_VALUES.join(', ')} (or leave blank)`,
    })
    .optional()
    .nullable()
    .transform((v) => v ?? null),

  client: z.string().optional().nullable().transform((v) => v ?? null),
}).superRefine((data, ctx) => {
  if (data.serving_date && data.shipment_date) {
    if (data.shipment_date > data.serving_date) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['shipment_date'],
        message: 'Shipment date cannot be after serving date',
      });
    }
  }
});

// -------------------------------------------------------------
// MEAL CANONICALIZATION
// The client's workbook contains values like " lunch", "Lunch R",
// "soup r", etc. Normalize whitespace + case to the canonical
// forms declared in MEAL_CATEGORIES. If the raw value differs
// from canonical after normalization, we emit a row-level
// `meal_case_normalized` warning so the operator can see what
// was coerced.
// -------------------------------------------------------------

const MEAL_CANONICAL = new Map<string, (typeof MEAL_CATEGORIES)[number]>([
  ['breakfast', 'Breakfast'],
  ['lunch',     'Lunch'],
  ['lunch r',   'Lunch R'],
  ['snack',     'Snack'],
  ['snack r',   'Snack R'],
  ['soup',      'Soup'],
  ['soup r',    'Soup R'],
  ['dinner',    'Dinner'],
]);

function canonicalizeMeal(raw: unknown): { value: unknown; normalized: boolean } {
  if (typeof raw !== 'string') return { value: raw, normalized: false };
  const key = raw.replace(/\s+/g, ' ').trim().toLowerCase();
  const canonical = MEAL_CANONICAL.get(key);
  if (!canonical) return { value: raw, normalized: false }; // let Zod reject it
  return { value: canonical, normalized: canonical !== raw };
}

// -------------------------------------------------------------
// ROW VALIDATION
// Validates a single raw row against the schema.
// Returns parsed data or a list of field-level errors.
// -------------------------------------------------------------

type RowValidationResult =
  | { success: true; data: ExcelRowParsed }
  | { success: false; errors: ImportRowError[]; row: number };

function validateRow(raw: Record<string, unknown>, rowIndex: number): RowValidationResult {
  const result = ExcelRowSchema.safeParse(raw);

  if (result.success) {
    return { success: true, data: result.data as ExcelRowParsed };
  }

  const errors: ImportRowError[] = result.error.issues.map((issue) => ({
    row: rowIndex,
    field: issue.path.length > 0 ? String(issue.path[0]) : null,
    message: issue.message,
  }));

  return { success: false, errors, row: rowIndex };
}

// -------------------------------------------------------------
// REFERENCE RESOLUTION
// Looks up company and recipe by name, returns UUIDs.
// Returns null with a descriptive error if either is not found
// or if the recipe is inactive.
// -------------------------------------------------------------

type ResolutionResult =
  | { success: true; data: ExcelRowResolved }
  | { success: false; errors: ImportRowError[] };

async function resolveRow(
  db: Db,
  parsed: ExcelRowParsed,
  rowIndex: number
): Promise<ResolutionResult> {
  const errors: ImportRowError[] = [];

  const [company, recipe] = await Promise.all([
    getCompanyByName(db, parsed.company),
    getRecipeByName(db, parsed.recipe, { includeInactive: false }),
  ]);

  if (!company) {
    errors.push({
      row: rowIndex,
      field: 'company',
      message: `Company "${parsed.company}" not found in the system`,
    });
  } else if (!company.active) {
    errors.push({
      row: rowIndex,
      field: 'company',
      message: `Company "${parsed.company}" is inactive and cannot receive orders`,
    });
  }

  if (!recipe) {
    errors.push({
      row: rowIndex,
      field: 'recipe',
      message: `Recipe "${parsed.recipe}" not found or is inactive`,
    });
  }

  if (errors.length > 0) {
    return { success: false, errors };
  }

  return {
    success: true,
    data: {
      company_id: company!.id,
      recipe_id: recipe!.id,
      serving_date: parsed.serving_date,
      shipment_date: parsed.shipment_date,
      meal_category: parsed.meal_category,
      amount: parsed.amount,
      portion_size: parsed.portion_size,
      day: parsed.day,
      frozen: parsed.storage === 'f',
      chilled: parsed.storage === 'c',
      client: parsed.client,
    },
  };
}

// -------------------------------------------------------------
// COLUMN NORMALISATION
// Maps raw sheet column headers to the schema field names.
// Case-insensitive, trims whitespace. Allows the template to
// use friendlier display names without breaking the parser.
// -------------------------------------------------------------

// `frozen` and `chilled` are intentionally NOT in this map — if present as
// separate columns we detect the split-storage shape in normaliseHeaders
// and route both columns to a single derived `storage` value.
const COLUMN_MAP: Record<string, keyof ExcelRowParsed> = {
  'company':          'company',
  'customer':         'company',
  'recipe':           'recipe',
  'serving date':     'serving_date',
  'serving_date':     'serving_date',
  'servingdate':      'serving_date',
  'shipment date':    'shipment_date',
  'shipment_date':    'shipment_date',
  'shipmentdate':     'shipment_date',
  'meal':             'meal_category',
  'meal category':    'meal_category',
  'meal_category':    'meal_category',
  'mealcategory':     'meal_category',
  'meal type':        'meal_category',
  'meal_type':        'meal_category',
  'mealtype':         'meal_category',
  'amount':           'amount',
  'order amount':     'amount',
  'order_amount':     'amount',
  'orderamount':      'amount',
  'portion size':     'portion_size',
  'portion_size':     'portion_size',
  'portionsize':      'portion_size',
  'day':              'day',
  'storage':          'storage',
  'frozen/chilled':   'storage',
  'client':           'client',
};

// Columns that are redundant with values already in canonical columns.
// Present them with a sheet-level warning instead of silently dropping —
// the real-client workbook contains both `Item` (duplicates Recipe) and
// `Quantity to Produce/Ship` (= Order Amount × Portion Size), and the
// operator should know we chose the authoritative column.
type WarnRedundantSpec = { code: ImportWarningCode; message: string };
const WARN_REDUNDANT_COLUMNS = new Map<string, WarnRedundantSpec>([
  ['item', {
    code: 'item_column_ignored',
    message: 'Item column duplicates Recipe; ignored.',
  }],
  ['quantity to produce/ship', {
    code: 'qty_formula_ignored',
    message: 'Quantity to Produce/Ship is computed from Order Amount × Portion Size; ignored.',
  }],
  ['quantity to produce', {
    code: 'qty_formula_ignored',
    message: 'Quantity to Produce/Ship is computed from Order Amount × Portion Size; ignored.',
  }],
  ['quantity_to_produce', {
    code: 'qty_formula_ignored',
    message: 'Quantity to Produce/Ship is computed from Order Amount × Portion Size; ignored.',
  }],
  ['qty', {
    code: 'qty_formula_ignored',
    message: 'Quantity to Produce/Ship is computed from Order Amount × Portion Size; ignored.',
  }],
]);

function normaliseHeader(h: string): string {
  return h.trim().toLowerCase().replace(/\s+/g, ' ');
}

interface SplitStorageSpec {
  frozenHeader: string;
  chilledHeader: string;
}

interface HeaderAnalysis {
  map: Map<string, keyof ExcelRowParsed>;
  splitStorage: SplitStorageSpec | null;
  redundantWarnings: ImportWarning[];
  itemHeader: string | null; // present if the Item column exists — used for per-row mismatch warnings
}

function analyseHeaders(rawHeaders: string[]): HeaderAnalysis {
  const map = new Map<string, keyof ExcelRowParsed>();
  const redundantWarnings: ImportWarning[] = [];
  const seenRedundantCodes = new Set<ImportWarningCode>();
  let itemHeader: string | null = null;
  let frozenHeader: string | null = null;
  let chilledHeader: string | null = null;
  let hasStorage = false;

  for (const header of rawHeaders) {
    const normalised = normaliseHeader(header);
    if (!normalised) continue;

    // Redundant columns (Item, Qty formula): emit one sheet-level warning
    // per code and skip the column so the schema never sees it.
    const redundant = WARN_REDUNDANT_COLUMNS.get(normalised);
    if (redundant) {
      if (!seenRedundantCodes.has(redundant.code)) {
        seenRedundantCodes.add(redundant.code);
        redundantWarnings.push({
          code: redundant.code,
          row: null,
          message: redundant.message,
        });
      }
      if (redundant.code === 'item_column_ignored') itemHeader = header;
      continue;
    }

    // Track frozen/chilled separately — they fold into `storage` below.
    if (normalised === 'frozen')  { frozenHeader = header; continue; }
    if (normalised === 'chilled') { chilledHeader = header; continue; }
    if (normalised === 'storage') hasStorage = true;

    const field = COLUMN_MAP[normalised];
    if (field) map.set(header, field);
  }

  // Split storage shape: both Frozen + Chilled present, no Storage column.
  // Preserve the EXACT original header strings — the real file has a
  // leading-space " Chilled" header that must round-trip verbatim through
  // sanitizer rebuilds.
  let splitStorage: SplitStorageSpec | null = null;
  if (frozenHeader && chilledHeader && !hasStorage) {
    splitStorage = { frozenHeader, chilledHeader };
  }

  return { map, splitStorage, redundantWarnings, itemHeader };
}

function normaliseRow(
  raw: Record<string, unknown>,
  headerMap: Map<string, keyof ExcelRowParsed>,
  splitStorage: SplitStorageSpec | null,
  rowNumber: number
): { row: Record<string, unknown>; warnings: ImportWarning[] } {
  const out: Record<string, unknown> = {};
  const warnings: ImportWarning[] = [];

  for (const [header, field] of headerMap.entries()) {
    const value = raw[header];

    if (field === 'storage') {
      // Explicit storage column — accept 'f'/'c'/'F'/'C' or blank.
      if (typeof value === 'string') {
        const v = value.trim().toLowerCase();
        out[field] = v === 'f' || v === 'c' ? v : null;
      } else {
        out[field] = null;
      }
      continue;
    }

    if (field === 'meal_category') {
      const { value: canon, normalized } = canonicalizeMeal(value);
      out[field] = canon;
      if (normalized) {
        warnings.push({
          code: 'meal_case_normalized',
          row: rowNumber,
          message: `Meal "${String(value).trim()}" normalized to "${String(canon)}".`,
          details: { from: String(value), to: String(canon) },
        });
      }
      continue;
    }

    out[field] = value;
  }

  // Derive storage from the Frozen/Chilled "x" markers when the sheet
  // uses the split-storage shape instead of a single Storage column.
  if (splitStorage) {
    const isSet = (v: unknown) =>
      typeof v === 'string' && v.trim().toLowerCase() === 'x';
    const frozenSet  = isSet(raw[splitStorage.frozenHeader]);
    const chilledSet = isSet(raw[splitStorage.chilledHeader]);

    if (frozenSet && chilledSet) {
      // Ambiguous — default to frozen so the row still imports.
      out.storage = 'f';
      warnings.push({
        code: 'storage_both_set',
        row: rowNumber,
        message: 'Both Frozen and Chilled are marked; defaulted to Frozen.',
      });
    } else if (frozenSet) {
      out.storage = 'f';
    } else if (chilledSet) {
      out.storage = 'c';
    } else {
      // Neither marked — in the real client's workbook 8 rows look like
      // this and the current import semantics default such rows to chilled.
      out.storage = 'c';
      warnings.push({
        code: 'storage_missing_defaulted_chilled',
        row: rowNumber,
        message: 'Neither Frozen nor Chilled is marked; defaulted to Chilled.',
      });
    }
  }

  return { row: out, warnings };
}

// -------------------------------------------------------------
// BOILERPLATE-ROW DETECTION
// Sheet 1 of a hand-maintained workbook sometimes trails off into
// office-use/packing-slip notes once the real orders end. Treat
// the first row whose Customer/Recipe/Item cell matches a known
// boilerplate token as EOF — drop it and everything after.
// -------------------------------------------------------------

const BOILERPLATE_TOKENS = [
  'packing slip',
  'office use only',
  'cambros',
  'packed by',
  'ice packs - please return to mom',
];

function isBoilerplateRow(raw: Record<string, unknown>, itemHeader: string | null): boolean {
  const candidates: string[] = [];
  for (const key of ['Customer', 'customer', 'Recipe', 'recipe']) {
    const v = raw[key];
    if (typeof v === 'string') candidates.push(v);
  }
  if (itemHeader) {
    const v = raw[itemHeader];
    if (typeof v === 'string') candidates.push(v);
  }

  for (const text of candidates) {
    const t = text.trim().toLowerCase();
    if (!t) continue;
    if (BOILERPLATE_TOKENS.some((tok) => t === tok || t.startsWith(tok))) return true;
    // "#3 of 5" style trailer rows (meal counts) land in the Customer col.
    if (/^#.*\bof\b/.test(t)) return true;
  }
  return false;
}

// -------------------------------------------------------------
// IMPORT ORCHESTRATION
// Full pipeline: validate all rows first, then resolve references
// for valid rows, then insert in a single transaction.
//
// Strategy: collect ALL errors across ALL rows before returning.
// If any row fails validation or resolution, the entire import
// is aborted — no partial inserts.
// -------------------------------------------------------------

export async function importOrdersFromRows(
  db: Pool,
  rawRows: Record<string, unknown>[],
  createdBy: string,
  headers?: string[],
  extraSheetCount = 0
): Promise<ImportResult> {
  const allWarnings: ImportWarning[] = [];

  if (extraSheetCount > 0) {
    allWarnings.push({
      code: 'extra_sheets_ignored',
      row: null,
      message: `${extraSheetCount} additional ${extraSheetCount === 1 ? 'sheet was' : 'sheets were'} ignored; only the first sheet is imported.`,
      details: { count: extraSheetCount },
    });
  }

  if (rawRows.length === 0) {
    return { inserted: 0, skipped: 0, errors: [], warnings: allWarnings };
  }

  const allErrors: ImportRowError[] = [];

  // -------------------------------------------------------------
  // STEP 1 — Analyse headers (column map, split-storage, redundant warnings)
  // -------------------------------------------------------------
  const effectiveHeaders = headers ?? Object.keys(rawRows[0]);
  const analysis = analyseHeaders(effectiveHeaders);
  allWarnings.push(...analysis.redundantWarnings);

  // -------------------------------------------------------------
  // STEP 2 — Boilerplate stop: truncate rows at the first boilerplate marker
  // -------------------------------------------------------------
  let dataRows = rawRows;
  for (let i = 0; i < rawRows.length; i++) {
    if (isBoilerplateRow(rawRows[i], analysis.itemHeader)) {
      allWarnings.push({
        code: 'boilerplate_row_stopped',
        row: i + 2,
        message: `Stopped reading at boilerplate row ${i + 2}; ${rawRows.length - i} trailing ${rawRows.length - i === 1 ? 'row' : 'rows'} ignored.`,
        details: { stopped_at: i + 2, dropped_count: rawRows.length - i },
      });
      dataRows = rawRows.slice(0, i);
      break;
    }
  }

  if (dataRows.length === 0) {
    return { inserted: 0, skipped: 0, errors: [], warnings: allWarnings };
  }

  // -------------------------------------------------------------
  // STEP 3 — Per-row item/recipe mismatch check (before normalisation
  //          strips the Item column from the record)
  // -------------------------------------------------------------
  if (analysis.itemHeader) {
    for (let i = 0; i < dataRows.length; i++) {
      const row = dataRows[i];
      const itemVal   = row[analysis.itemHeader];
      // Recipe column header may be exactly "Recipe"/"recipe".
      const recipeVal = row['Recipe'] ?? row['recipe'];
      if (
        typeof itemVal === 'string' &&
        typeof recipeVal === 'string' &&
        itemVal.trim() &&
        recipeVal.trim() &&
        itemVal.trim().toLowerCase() !== recipeVal.trim().toLowerCase()
      ) {
        allWarnings.push({
          code: 'item_recipe_mismatch',
          row: i + 2,
          message: `Item "${itemVal.trim()}" differs from Recipe "${recipeVal.trim()}"; Recipe wins.`,
          details: { item: itemVal.trim(), recipe: recipeVal.trim() },
        });
      }
    }
  }

  // -------------------------------------------------------------
  // STEP 4 — Normalise each row (meal canonicalization, split-storage)
  // -------------------------------------------------------------
  const normalisedRows: Record<string, unknown>[] = [];
  for (let i = 0; i < dataRows.length; i++) {
    const { row, warnings } = normaliseRow(dataRows[i], analysis.map, analysis.splitStorage, i + 2);
    normalisedRows.push(row);
    allWarnings.push(...warnings);
  }

  // -------------------------------------------------------------
  // STEP 5 — Validate all rows with Zod (no DB calls yet)
  // -------------------------------------------------------------
  const parsedRows: Array<ExcelRowParsed | null> = [];

  for (let i = 0; i < normalisedRows.length; i++) {
    const rowNumber = i + 2; // +2: 1-based + header row offset
    const validation = validateRow(normalisedRows[i], rowNumber);

    if (validation.success) {
      parsedRows.push(validation.data);
    } else {
      allErrors.push(...validation.errors);
      parsedRows.push(null);
    }
  }

  // Abort early if any rows failed schema validation —
  // no point hitting the DB for reference lookups.
  if (allErrors.length > 0) {
    return { inserted: 0, skipped: 0, errors: allErrors, warnings: allWarnings };
  }

  // -------------------------------------------------------------
  // STEP 6 — Resolve company + recipe references in parallel
  // Each row is resolved concurrently; errors accumulate.
  // -------------------------------------------------------------
  const resolvedRows: Array<ExcelRowResolved | null> = [];

  const resolutionResults = await Promise.all(
    (parsedRows as ExcelRowParsed[]).map((parsed, i) =>
      resolveRow(db, parsed, i + 2)
    )
  );

  for (const result of resolutionResults) {
    if (result.success) {
      resolvedRows.push(result.data);
    } else {
      allErrors.push(...result.errors);
      resolvedRows.push(null);
    }
  }

  if (allErrors.length > 0) {
    return { inserted: 0, skipped: 0, errors: allErrors, warnings: allWarnings };
  }

  // -------------------------------------------------------------
  // STEP 7 — Insert all resolved rows in a single transaction
  // -------------------------------------------------------------
  const orders: InsertOrder[] = (resolvedRows as ExcelRowResolved[]).map((row) => ({
    company_id:    row.company_id,
    recipe_id:     row.recipe_id,
    created_by:    createdBy,
    order_source:  'excel_import',
    status:        'pending',
    meal_category: row.meal_category,
    amount:        row.amount,
    portion_size:  row.portion_size,
    day:           row.day,
    serving_date:  row.serving_date,   // pass YYYY-MM-DD string — avoids timezone shift
    shipment_date: row.shipment_date,  // pass YYYY-MM-DD string — avoids timezone shift
    frozen:        row.frozen,
    chilled:       row.chilled,
    recurring:     false,
    client:        row.client,
  }));

  const client = await db.connect();

  try {
    await client.query('BEGIN');
    const inserted = await insertOrdersBatch(client, orders);
    await client.query('COMMIT');

    return { inserted: inserted.length, skipped: 0, errors: [], warnings: allWarnings };
  } catch (err) {
    await client.query('ROLLBACK');
    throw err; // Surface unexpected DB errors to the route handler
  } finally {
    client.release();
  }
}
