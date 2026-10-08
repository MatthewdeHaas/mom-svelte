import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { internalFetch, callerFromLocals } from '$lib/server/api-client';
import type { ImportResult } from '$lib/types';
import logger from '$lib/server/logger';

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
    // STEP 2 — Forward multipart body to Rust importer.
    //
    // Rust owns the whole pipeline (parse + validate + commit) and
    // returns the same `ImportResult` shape the page expects. There
    // is no TS fall-through — Track 3 Step 6 flipped the flag on
    // permanently; rollback is `git revert`, not a flag flip.
    // ----------------------------------------------------------
    const caller = callerFromLocals(locals);

    try {
      const forward = new FormData();
      forward.append(
        'file',
        new Blob([await file.arrayBuffer()], {
          type: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
        }),
        file.name,
      );

      const res = await internalFetch('/api/import/commit', caller, {
        method: 'POST',
        body: forward,
      });

      if (!res.ok) {
        const text = await res.text().catch(() => '');
        logger.error({ status: res.status, body: text }, 'rust import failed');
        return fail(500, {
          error: 'An unexpected error occurred during import. Please try again or contact support.',
        });
      }

      const result = (await res.json()) as ImportResult;

      if (result.errors.length > 0) {
        return fail(422, {
          importErrors: result.errors,
          importWarnings: result.warnings,
          rowCount: result.inserted + result.errors.length,
        });
      }

      return {
        success: true,
        inserted: result.inserted,
        skipped: result.skipped,
        warnings: result.warnings,
      };
    } catch (err) {
      logger.error({ err }, 'rust import forward error');
      return fail(500, {
        error: 'An unexpected error occurred during import. Please try again or contact support.',
      });
    }
  },
};
