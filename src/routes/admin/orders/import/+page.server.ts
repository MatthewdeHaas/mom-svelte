import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { pool } from '$lib/server/db';
import { parseExcelBufferAsync } from '$lib/excel/parse';
import { importOrdersFromRows } from '$lib/server/queries/imports';

// Surface a minimal load so the page has access to prior import
// results on navigation — the action result handles the POST case.
export const load: PageServerLoad = async () => {
  return {};
};

export const actions: Actions = {
  default: async ({ request, locals }) => {
		if (!locals.user) {
			return fail(401, { error: 'Unauthorised' });
		}

    // ----------------------------------------------------------
    // STEP 1 — Extract file from form data
    // ----------------------------------------------------------
    const formData = await request.formData();
    const file = formData.get('file');

    if (!file || !(file instanceof File)) {
      return fail(400, {
        error: 'No file provided. Please select an Excel file to upload.',
      });
    }

    if (!file.name.endsWith('.xlsx')) {
      return fail(400, {
        error: 'Invalid file type. Please upload an .xlsx file.',
      });
    }

    if (file.size === 0) {
      return fail(400, { error: 'The uploaded file is empty.' });
    }

    // Reasonable upper bound — a full week of orders across many
    // companies should never approach this.
    const MAX_SIZE_BYTES = 5 * 1024 * 1024; // 5MB
    if (file.size > MAX_SIZE_BYTES) {
      return fail(400, { error: 'File is too large. Maximum size is 5MB.' });
    }

    // ----------------------------------------------------------
    // STEP 2 — Parse
    // ----------------------------------------------------------
    let rawRows: Record<string, unknown>[];

    try {
			const buffer = Buffer.from(await file.arrayBuffer());
			rawRows = await parseExcelBufferAsync(buffer);
    } catch (err) {
      console.error('[import] parse error', err);
      return fail(400, {
        error: 'Could not read the uploaded file. Make sure it is a valid .xlsx file.',
      });
    }

    if (rawRows.length === 0) {
      return fail(400, {
        error: 'The spreadsheet contains no data rows. Please check the file and try again.',
      });
    }

    // ----------------------------------------------------------
    // STEP 3 — Validate + resolve + insert
    // ----------------------------------------------------------
    try {
			const headers = rawRows.length > 0 ? Object.keys(rawRows[0]) : [];
			const result = await importOrdersFromRows(pool, rawRows, locals.user.id, headers);

      if (result.errors.length > 0) {
        // Validation or resolution errors — return them to the page
        // for display. HTTP 422 so the form knows it was a data problem.
        return fail(422, {
          importErrors: result.errors,
          rowCount: rawRows.length,
        });
      }

      return {
        success: true,
        inserted: result.inserted,
        skipped: result.skipped,
      };
    } catch (err) {
      console.error('[import] pipeline error', err);
      return fail(500, {
        error: 'An unexpected error occurred during import. Please try again or contact support.',
      });
    }
  },
};
