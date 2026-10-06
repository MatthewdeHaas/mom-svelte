import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { pool } from '$lib/server/db';
import { parseRecipeExcel } from '$lib/excel/parse-recipes';
import { importRecipesFromRows } from '$lib/server/queries/recipe-imports';
import { getIngredientNames } from '$lib/server/queries/ingredients';
import { findFuzzyWarnings } from '$lib/utils/fuzzy-match';
import { createRecipe } from '$lib/server/queries/recipes';
import { ConflictError } from '$lib/server/errors';
import type { RecipeImportResult, ImportRowError } from '$lib/types';
import type { FuzzyMatch } from '$lib/utils/fuzzy-match';
import logger from '$lib/server/logger';
import { parseFormData } from '$lib/server/form';
import { RecipeNameSchema } from '$lib/server/schemas/recipe';
import { internalFetch, type InternalCaller } from '$lib/server/api-client';
import { useRustBackend } from '$lib/server/queries/_rust-dispatch';

export const load: PageServerLoad = async () => {
  return {};
};

export const actions: Actions = {
  // ── Manual single-recipe creation ───────────────────────────────────────────
  create: async ({ request }) => {
    const form = await request.formData();
    const parsed = parseFormData(RecipeNameSchema, form);

    if (!parsed.success) {
      return fail(400, {
        errors: parsed.errors,
        values: { name: form.get('name')?.toString() ?? '' },
      });
    }

    const { name } = parsed.data;

    let recipe;
    try {
      recipe = await createRecipe(pool, { name, active: true });
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
    // Flag-gated Rust forward.
    //
    // Unlike the eight dispatcher-plumbed domains, recipe-imports forwards at
    // the ROUTE level — `importRecipesFromRows` consumes an already-parsed
    // `RecipeExcelRowParsed[]`, so by the time we'd reach the query layer the
    // multipart bytes are gone. Forwarding here keeps the Rust path
    // byte-identical to a direct POST against `/api/recipe-imports/*`.
    //
    // Decision: `(mode === 'add' && !confirmed)` → `/preview` (returns fuzzy
    // warnings for user review); any other combination → `/commit` (writes).
    // Shape translation on the preview response: Rust emits `FuzzyMatch[]`;
    // the SvelteKit page expects `ImportRowError[]` warnings. We synthesise
    // one `ImportRowError` per fuzzy match. Row numbers aren't recoverable
    // without the parsed rows (the Rust side owns the parse), so `row: 0` is
    // used as a sentinel — the message carries the user-visible content.
    // ──────────────────────────────────────────────────────────────────────────
    const caller: InternalCaller = {
      userId: locals.user.id,
      role: locals.user.role === 'client' ? 'client' : 'admin',
      companyId: locals.user.companyId ?? null,
    };

    const useRust = await useRustBackend('recipe_imports', caller);
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
          // 400 (bad upload / empty rows / unparseable) and 500 (DB) share
          // a terse JSON `{error, message}` shape via ApiError. Prefer
          // `message` where present so the user sees the specific cause
          // (e.g. "Excel template missing columns: ...").
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
          // We re-forward the file so the Rust side owns the single
          // write path; piggy-backing on the already-parsed preview isn't
          // worth the state-threading complexity here.
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

    // Stage 1.5: Fuzzy ingredient matching (Add mode only)
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
        const warnings: ImportRowError[] = fuzzyMatches.map((m) => {
          const rowIdx = parsed.rows.findIndex((r) => r.ingredient === m.input);
          return {
            row: rowIdx + 2,
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
