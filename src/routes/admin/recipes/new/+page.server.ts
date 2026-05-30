import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { pool } from '$lib/server/db';
import { parseRecipeExcel } from '$lib/excel/parse-recipes';
import { importRecipesFromRows } from '$lib/server/queries/recipe-imports';
import { createRecipe } from '$lib/server/queries/recipes'; // your existing fn
import type { RecipeImportResult } from '$lib/types';
import logger from '$lib/server/logger';

export const load: PageServerLoad = async () => {
  return {};
};

export const actions: Actions = {
  // ── Manual single-recipe creation (existing behaviour) ──────────────────────
  create: async ({ request }) => {
    const data = await request.formData();
    const name = String(data.get('name') ?? '').trim();

    if (!name) {
      return fail(400, {
        errors: { name: 'Recipe name is required' },
        values: { name },
      });
    }

    const recipe = await createRecipe(pool, { name });
    redirect(303, `/admin/recipes/${recipe.id}/ingredients`);
  },

  // ── Excel bulk import ────────────────────────────────────────────────────────
  import: async ({ request, locals }) => {
    const formData = await request.formData();
    const file = formData.get('file');

    if (!(file instanceof File) || file.size === 0) {
      return fail(400, { importError: 'No file uploaded.' });
    }

    if (!file.name.endsWith('.xlsx')) {
      return fail(400, { importError: 'File must be an .xlsx spreadsheet.' });
    }

    const MAX_BYTES = 5 * 1024 * 1024;
    if (file.size > MAX_BYTES) {
      return fail(400, { importError: 'File exceeds 5 MB limit.' });
    }

    const buffer = Buffer.from(await file.arrayBuffer());

    // Stage 1: parse + validate
    let parsed: Awaited<ReturnType<typeof parseRecipeExcel>>;
    try {
      parsed = await parseRecipeExcel(buffer);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to read Excel file.';
      return fail(400, { importError: message });
    }

    if (parsed.errors.length > 0) {
      return fail(422, {
        parseErrors: parsed.errors,
        importError: `${parsed.errors.length} row(s) failed validation. No data was imported.`,
      });
    }

    if (parsed.rows.length === 0) {
      return fail(400, { importError: 'The spreadsheet contains no data rows.' });
    }

    // Stage 2: import
    let result: RecipeImportResult;
    try {
      const createdBy = locals.user?.email ?? 'unknown';
      result = await importRecipesFromRows(pool, parsed.rows, createdBy);
    } catch (err) {
      logger.error({ err }, 'recipe import DB error');
      return fail(500, { importError: 'Import failed due to a database error. No changes were saved.' });
    }

    return { success: true, result };
  },
};
