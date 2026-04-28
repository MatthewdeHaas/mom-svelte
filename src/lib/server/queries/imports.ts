import { z } from 'zod';
import type { Pool } from 'pg';
import type {
  ExcelRowParsed,
  ExcelRowResolved,
  ImportResult,
  ImportRowError,
  InsertOrder,
} from '$lib/types';
import { getCompanyByName } from './companies';
import { getRecipeByName } from './recipes';
import { insertOrdersBatch } from './orders';

// -------------------------------------------------------------
// ZOD SCHEMA
// Validates a raw row off the sheet. All fields come in as
// unknown — Zod coerces and validates in one pass.
// -------------------------------------------------------------

const MEAL_CATEGORIES = ['Breakfast', 'Lunch', 'Snack', 'Soup', 'Dinner'] as const;
const STORAGE_VALUES = ['frozen', 'chilled'] as const;

const ISO_DATE = /^\d{4}-\d{2}-\d{2}$/;

export const ExcelRowSchema = z.object({
  company: z
    .string({ required_error: 'Company is required' })
    .min(1, 'Company cannot be empty'),

  recipe: z
    .string({ required_error: 'Recipe is required' })
    .min(1, 'Recipe cannot be empty'),

  serving_date: z
    .string({ required_error: 'Serving date is required' })
    .regex(ISO_DATE, 'Serving date must be in YYYY-MM-DD format'),

  shipment_date: z
    .string({ required_error: 'Shipment date is required' })
    .regex(ISO_DATE, 'Shipment date must be in YYYY-MM-DD format'),

  meal_category: z.enum(MEAL_CATEGORIES, {
    errorMap: () => ({
      message: `Meal category must be one of: ${MEAL_CATEGORIES.join(', ')}`,
    }),
  }),

  amount: z
    .number({ required_error: 'Amount is required', invalid_type_error: 'Amount must be a number' })
    .positive('Amount must be greater than zero'),

  storage: z.enum(STORAGE_VALUES, {
    errorMap: () => ({
      message: `Storage must be one of: ${STORAGE_VALUES.join(', ')}`,
    }),
  }),

  notes: z.string().optional().nullable().transform((v) => v ?? null),
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
  db: Pool,
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
      frozen: parsed.storage === 'frozen',
      chilled: parsed.storage === 'chilled',
      client_note: parsed.notes ?? null,
    },
  };
}

// -------------------------------------------------------------
// COLUMN NORMALISATION
// Maps raw sheet column headers to the schema field names.
// Case-insensitive, trims whitespace. Allows the template to
// use friendlier display names without breaking the parser.
// -------------------------------------------------------------

const COLUMN_MAP: Record<string, keyof ExcelRowParsed> = {
  'company':        'company',
  'recipe':         'recipe',
  'serving date':   'serving_date',
  'servingdate':    'serving_date',
  'shipment date':  'shipment_date',
  'shipmentdate':   'shipment_date',
  'meal category':  'meal_category',
  'mealcategory':   'meal_category',
  'meal type':      'meal_category',
  'mealtype':       'meal_category',
  'amount':         'amount',
  'storage':        'storage',
  'notes':          'notes',
};

function normaliseHeaders(rawHeaders: string[]): Map<string, keyof ExcelRowParsed> {
  const map = new Map<string, keyof ExcelRowParsed>();
  for (const header of rawHeaders) {
    const normalised = header.trim().toLowerCase().replace(/\s+/g, ' ');
    const field = COLUMN_MAP[normalised];
    if (field) map.set(header, field);
  }
  return map;
}

function normaliseRow(
  raw: Record<string, unknown>,
  headerMap: Map<string, keyof ExcelRowParsed>
): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [header, field] of headerMap.entries()) {
    out[field] = raw[header];
  }
  return out;
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
  headers?: string[]
): Promise<ImportResult> {
  if (rawRows.length === 0) {
    return { inserted: 0, skipped: 0, errors: [] };
  }

  const allErrors: ImportRowError[] = [];

  // -------------------------------------------------------------
  // STEP 1 — Normalise headers if provided
  // -------------------------------------------------------------
  const headerMap = headers ? normaliseHeaders(headers) : null;

  const normalisedRows = headerMap
    ? rawRows.map((row) => normaliseRow(row, headerMap))
    : rawRows;

  // -------------------------------------------------------------
  // STEP 2 — Validate all rows with Zod (no DB calls yet)
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
    return { inserted: 0, skipped: 0, errors: allErrors };
  }

  // -------------------------------------------------------------
  // STEP 3 — Resolve company + recipe references in parallel
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
    return { inserted: 0, skipped: 0, errors: allErrors };
  }

  // -------------------------------------------------------------
  // STEP 4 — Insert all resolved rows in a single transaction
  // -------------------------------------------------------------
  const orders: InsertOrder[] = (resolvedRows as ExcelRowResolved[]).map((row) => ({
    company_id:   row.company_id,
    recipe_id:    row.recipe_id,
    created_by:   createdBy,
    order_source: 'excel_import',
    status:       'pending',
    meal_category: row.meal_category,
    amount:       row.amount,
    serving_date: new Date(row.serving_date),
    shipment_date: new Date(row.shipment_date),
    frozen:       row.frozen,
    chilled:      row.chilled,
    recurring:    false,
    client_note:  row.client_note,
  }));

  const client = await db.connect();

  try {
    await client.query('BEGIN');
    const inserted = await insertOrdersBatch(client, orders);
    await client.query('COMMIT');

    return { inserted: inserted.length, skipped: 0, errors: [] };
  } catch (err) {
    await client.query('ROLLBACK');
    throw err; // Surface unexpected DB errors to the route handler
  } finally {
    client.release();
  }
}
