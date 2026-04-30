import ExcelJS from 'exceljs';
import { z } from 'zod';
import type { RecipeExcelRowParsed } from '$lib/types';
import type { ImportRowError } from '$lib/types';

// ─── Schema ──────────────────────────────────────────────────────────────────

const RecipeRowSchema = z.object({
  recipe: z.string().min(1, 'Recipe name is required'),
  ingredient: z.string().nullable().optional(),
  amount: z.number().positive('Amount must be greater than zero').nullable().optional(),
  unit: z.string().nullable().optional(),
  allergens: z.string().nullable().optional(),
}).superRefine((data, ctx) => {
  const hasIngredient = !!data.ingredient;
  const hasAmount = data.amount != null;
  const hasUnit = !!data.unit;
  if (hasIngredient && (!hasAmount || !hasUnit)) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      message: 'Ingredient rows must have amount and unit',
    });
  }
});

// ─── Column name normaliser ───────────────────────────────────────────────────

function normaliseHeader(raw: unknown): string {
  return String(raw ?? '').trim().toLowerCase().replace(/\s+/g, '_');
}

function cellText(cell: ExcelJS.Cell): string | null {
  const v = cell.value;
  if (v == null) return null;
  if (typeof v === 'object' && 'text' in v) return String((v as ExcelJS.CellRichTextValue).text).trim() || null;
  return String(v).trim() || null;
}

function cellNumber(cell: ExcelJS.Cell): number | null {
  const v = cell.value;
  if (v == null) return null;
  const n = Number(v);
  return isNaN(n) ? null : n;
}

// ─── Parser ───────────────────────────────────────────────────────────────────

export interface ParseRecipesResult {
  rows: RecipeExcelRowParsed[];
  errors: ImportRowError[];
}

/**
 * Parses an exceljs Buffer/ArrayBuffer into validated recipe rows.
 * Returns all rows + any per-row validation errors.
 * Callers should abort the import if errors.length > 0.
 */
export async function parseRecipeExcel(buffer: Buffer): Promise<ParseRecipesResult> {
  const workbook = new ExcelJS.Workbook();
  await workbook.xlsx.load(buffer);

  const sheet = workbook.worksheets[0];
  if (!sheet) throw new Error('Excel file contains no worksheets');

  // Build header→column-index map from row 1
  const headerRow = sheet.getRow(1);
  const colIndex: Record<string, number> = {};
  headerRow.eachCell((cell, colNumber) => {
    colIndex[normaliseHeader(cell.value)] = colNumber;
  });

  const required = ['recipe', 'ingredient', 'amount', 'unit', 'allergens'];
  const missing = required.filter((h) => !(h in colIndex));
  if (missing.length) {
    throw new Error(`Excel template missing columns: ${missing.join(', ')}`);
  }

  const rows: RecipeExcelRowParsed[] = [];
  const errors: ImportRowError[] = [];

  sheet.eachRow((row, rowNumber) => {
    if (rowNumber === 1) return; // skip header

    const raw = {
      recipe:     cellText(row.getCell(colIndex['recipe'])),
      ingredient: cellText(row.getCell(colIndex['ingredient'])),
      amount:     cellNumber(row.getCell(colIndex['amount'])),
      unit:       cellText(row.getCell(colIndex['unit'])),
      allergens:  cellText(row.getCell(colIndex['allergens'])),
    };

    // Skip entirely blank rows
    if (!raw.recipe && !raw.ingredient) return;

    const result = RecipeRowSchema.safeParse(raw);

    if (!result.success) {
      errors.push({
        row: rowNumber,
        field: result.error.issues[0]?.path.join('.') ?? 'unknown',
        message: result.error.issues.map((i) => i.message).join('; '),
        raw_value: JSON.stringify(raw),
      });
      return;
    }

    rows.push({
      recipe:     result.data.recipe,
      ingredient: result.data.ingredient ?? null,
      amount:     result.data.amount ?? null,
      unit:       result.data.unit ?? null,
      allergens:  result.data.allergens ?? null,
    });
  });

  return { rows, errors };
}
