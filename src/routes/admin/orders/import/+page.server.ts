import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { pool } from '$lib/server/db';
import { parseExcelBufferAsyncWithMeta } from '$lib/excel/parse';
import { importOrdersFromRows } from '$lib/server/queries/imports';
import { internalFetch, internalJson, type InternalCaller } from '$lib/server/api-client';
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
    // STEP 1b — Flag-gated Rust forward
    //
    // When `use_rust_backend_imports` is on, pipe the multipart
    // body straight to the Rust service and translate its
    // `ImportResult` into the same shape the TS path returns.
    // The TS path stays live underneath so a flag flip reverts
    // instantly to the known-good importer.
    // ----------------------------------------------------------
    const caller: InternalCaller = {
      userId: locals.user.id,
      role: locals.user.role === 'client' ? 'client' : 'admin',
      companyId: locals.user.companyId ?? null,
    };

    let useRust = false;
    try {
      const r = await internalJson<{ enabled: boolean }>('/api/flags/check', caller, {
        method: 'POST',
        body: JSON.stringify({ name: 'use_rust_backend_imports' }),
      });
      useRust = r.enabled;
    } catch (err) {
      // Flag check failures must NOT block imports — fall back to
      // the TS path and log for ops. Rust unreachable → TS still works.
      logger.warn({ err }, 'flag check failed; falling back to TS importer');
    }

    if (useRust) {
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
    }

    // ----------------------------------------------------------
    // STEP 2 — Parse
    // ----------------------------------------------------------
    let rawRows: Record<string, unknown>[];
    let extraSheetCount = 0;
    let headers: string[] = [];

    try {
			const buffer = Buffer.from(await file.arrayBuffer());
			const parsed = await parseExcelBufferAsyncWithMeta(buffer);
			rawRows = parsed.rows;
			extraSheetCount = parsed.extraSheetCount;
			headers = parsed.headers;
    } catch (err) {
      logger.error({ err }, 'import parse error');
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
			const result = await importOrdersFromRows(
				pool,
				rawRows,
				locals.user.id,
				headers,
				extraSheetCount,
			);

      if (result.errors.length > 0) {
        // Validation or resolution errors — return them to the page
        // for display. HTTP 422 so the form knows it was a data problem.
        // Warnings travel alongside errors so the operator sees both.
        return fail(422, {
          importErrors: result.errors,
          importWarnings: result.warnings,
          rowCount: rawRows.length,
        });
      }

      return {
        success: true,
        inserted: result.inserted,
        skipped: result.skipped,
        warnings: result.warnings,
      };
    } catch (err) {
      logger.error({ err }, 'import pipeline error');
      return fail(500, {
        error: 'An unexpected error occurred during import. Please try again or contact support.',
      });
    }
  },
};
