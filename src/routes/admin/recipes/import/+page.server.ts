import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { pool } from '$lib/server/db';
import { parseRecipeExcel } from '$lib/excel/parse-recipes';
import { importRecipesFromRows } from '$lib/server/queries/recipe-imports';
import { getIngredientNames } from '$lib/server/queries/ingredients';
import { findFuzzyWarnings } from '$lib/utils/fuzzy-match';
import type { RecipeImportResult, ImportRowError } from '$lib/types';
import logger from '$lib/server/logger';

export const load: PageServerLoad = async () => {
  return {};
};

export const actions: Actions = {
  import: async ({ request, locals }) => {
    const formData = await request.formData();
    const file = formData.get('file');
    const mode = (formData.get('mode') as string) || 'replace';
    const confirmed = formData.get('confirmed') === 'true';

    if (!(file instanceof File) || file.size === 0) {
      return fail(400, { error: 'No file uploaded.' });
    }

    if (!file.name.endsWith('.xlsx')) {
      return fail(400, { error: 'File must be an .xlsx spreadsheet.' });
    }

    const MAX_BYTES = 5 * 1024 * 1024; // 5 MB
    if (file.size > MAX_BYTES) {
      return fail(400, { error: 'File exceeds 5 MB limit.' });
    }

    // Convert File → Buffer for exceljs
    const arrayBuffer = await file.arrayBuffer();
    const buffer = Buffer.from(arrayBuffer);

    // ── Stage 1: Parse + validate ──────────────────────────────────────────────
    let parsed: Awaited<ReturnType<typeof parseRecipeExcel>>;
    try {
      parsed = await parseRecipeExcel(buffer);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to read Excel file.';
      return fail(400, { error: message });
    }

    if (parsed.errors.length > 0) {
      return fail(422, {
        parseErrors: parsed.errors,
        error: `${parsed.errors.length} row(s) failed validation. No data was imported.`,
      });
    }

    if (parsed.rows.length === 0) {
      return fail(400, { error: 'The spreadsheet contains no data rows.' });
    }

    // ── Stage 1.5: Fuzzy ingredient matching (Add mode only) ──────────────────
    if (mode === 'add' && !confirmed) {
      const ingredientNamesInFile = [
        ...new Set(
          parsed.rows
            .map((r) => r.ingredient)
            .filter((n): n is string => !!n)
        ),
      ];

      const existingNames = await getIngredientNames(pool);
      const fuzzyMatches = findFuzzyWarnings(ingredientNamesInFile, existingNames);

      if (fuzzyMatches.length > 0) {
        // Map fuzzy matches back to row numbers for display
        const warnings: ImportRowError[] = fuzzyMatches.map((m) => {
          // Find first row containing this ingredient
          const rowIdx = parsed.rows.findIndex((r) => r.ingredient === m.input);
          return {
            row: rowIdx + 2, // +2: 1-based + header row
            field: 'Ingredient',
            message: `Similar ingredient already exists: "${m.match}" (${Math.round(m.score * 100)}% match)`,
            raw_value: m.input,
            severity: 'warning' as const,
            suggestion: m.match,
          };
        });

        return {
          warnings,
          parsedRowCount: parsed.rows.length,
          mode,
        };
      }
    }

    // ── Stage 2: Import ────────────────────────────────────────────────────────
    let result: RecipeImportResult;
    try {
      const createdBy = locals.user?.email ?? 'unknown';
      result = await importRecipesFromRows(pool, parsed.rows, createdBy);
    } catch (err) {
      logger.error({ err }, 'recipe import DB error');
      return fail(500, { error: 'Import failed due to a database error. No changes were saved.' });
    }

    return { success: true, result };
  },
};
