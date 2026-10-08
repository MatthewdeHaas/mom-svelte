import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { createRecipe } from '$lib/server/queries/_rust-dispatch';
import { ConflictError } from '$lib/server/errors';
import type { RecipeImportResult, ImportRowError } from '$lib/types';
import logger from '$lib/server/logger';

// Inlined from the deleted $lib/utils/fuzzy-match — Rust owns the
// matching now; this type only shapes the preview response.
interface FuzzyMatch {
  input: string;
  match: string;
  score: number;
}
import { parseFormData } from '$lib/server/form';
import { RecipeNameSchema } from '$lib/server/schemas/recipe';
import { internalFetch, callerFromLocals, type InternalCaller } from '$lib/server/api-client';

export const load: PageServerLoad = async () => {
  return {};
};

export const actions: Actions = {
  // ── Manual single-recipe creation ───────────────────────────────────────────
  create: async ({ request, locals }) => {
    const form = await request.formData();
    const parsed = parseFormData(RecipeNameSchema, form);

    if (!parsed.success) {
      return fail(400, {
        errors: parsed.errors,
        values: { name: form.get('name')?.toString() ?? '' },
      });
    }

    const { name } = parsed.data;

    const caller = callerFromLocals(locals);
    let recipe;
    try {
      recipe = await createRecipe({ name, active: true }, caller);
    } catch (err) {
      if (err instanceof ConflictError) {
        return fail(422, {
          errors: { name: 'A recipe with this name already exists' },
          values: { name },
        });
      }
      throw err;
    }
    redirect(303, `/admin/recipes/${recipe.id}/ingredients`);
  },

  // ── Excel bulk import ────────────────────────────────────────────────────────
  import: async ({ request, locals }) => {
    if (!locals.user) {
      return fail(401, { importError: 'Unauthorised' });
    }

    const formData = await request.formData();
    const file = formData.get('file');
    const mode = (formData.get('mode') as string) || 'replace';
    const confirmed = formData.get('confirmed') === 'true';

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

    // ──────────────────────────────────────────────────────────────────────────
    // Forward multipart body to Rust importer.
    //
    // Rust owns parse + validate + commit for recipe imports; the
    // preview endpoint returns fuzzy warnings only when `mode === 'add'`
    // and the user has not yet confirmed. There is no TS fall-through —
    // Track 3 Step 6 flipped the flag on permanently; rollback is
    // `git revert`, not a flag flip.
    // ──────────────────────────────────────────────────────────────────────────
    const caller: InternalCaller = callerFromLocals(locals);

    try {
      const forward = new FormData();
      forward.append(
        'file',
        new Blob([await file.arrayBuffer()], {
          type: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
        }),
        file.name,
      );

      const usePreview = mode === 'add' && !confirmed;
      const endpoint = usePreview
        ? '/api/recipe-imports/preview'
        : '/api/recipe-imports/commit';

      const res = await internalFetch(endpoint, caller, {
        method: 'POST',
        body: forward,
      });

      // 422: row-level validation failures (both endpoints).
      if (res.status === 422) {
        const body = (await res.json().catch(() => null)) as
          | { parse_errors?: ImportRowError[] }
          | null;
        const parseErrors = body?.parse_errors ?? [];
        return fail(422, {
          parseErrors,
          importError: `${parseErrors.length} row(s) failed validation. No data was imported.`,
        });
      }

      if (!res.ok) {
        const body = (await res.json().catch(() => null)) as
          | { message?: string }
          | null;
        const message = body?.message
          ?? (res.status >= 500
            ? 'Import failed due to a database error. No changes were saved.'
            : 'Import failed.');
        logger.error({ status: res.status, body }, 'rust recipe-import failed');
        return fail(res.status < 500 ? 400 : 500, { importError: message });
      }

      if (usePreview) {
        const preview = (await res.json()) as {
          rows_parsed: number;
          parse_errors: ImportRowError[];
          fuzzy_warnings: FuzzyMatch[];
        };

        if (preview.fuzzy_warnings.length > 0) {
          const warnings: ImportRowError[] = preview.fuzzy_warnings.map((m) => ({
            row: 0, // row context lives in the Rust parse; sentinel is fine
            field: 'Ingredient',
            message: `Similar ingredient already exists: "${m.match}" (${Math.round(m.score * 100)}% match)`,
            raw_value: m.input,
            severity: 'warning' as const,
            suggestion: m.match,
          }));

          return {
            warnings,
            parsedRowCount: preview.rows_parsed,
            mode,
          };
        }

        // Preview turned up no fuzzy warnings — fall through to commit.
        const commitForward = new FormData();
        commitForward.append(
          'file',
          new Blob([await file.arrayBuffer()], {
            type: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
          }),
          file.name,
        );
        const commitRes = await internalFetch(
          '/api/recipe-imports/commit',
          caller,
          { method: 'POST', body: commitForward },
        );
        if (!commitRes.ok) {
          const body = (await commitRes.json().catch(() => null)) as
            | { message?: string }
            | null;
          logger.error(
            { status: commitRes.status, body },
            'rust recipe-import commit failed after clean preview',
          );
          return fail(500, {
            importError: body?.message ?? 'Import failed after preview.',
          });
        }
        const result = (await commitRes.json()) as RecipeImportResult;
        return { success: true, result };
      }

      const result = (await res.json()) as RecipeImportResult;
      return { success: true, result };
    } catch (err) {
      logger.error({ err }, 'rust recipe-import forward error');
      return fail(500, {
        importError: 'Import failed due to a database error. No changes were saved.',
      });
    }
  },
};
