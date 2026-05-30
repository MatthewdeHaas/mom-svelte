import type { ZodType } from 'zod';

/**
 * Parse FormData through a Zod schema, returning either the validated data
 * or a flat error map (first error message per field).
 *
 * Multi-value fields (getAll) and File inputs must be extracted separately
 * before calling this helper — they are not part of Object.fromEntries().
 */
export function parseFormData<T>(
  schema: ZodType<T>,
  formData: FormData,
): { success: true; data: T } | { success: false; errors: Record<string, string> } {
  const raw = Object.fromEntries(formData);
  const result = schema.safeParse(raw);

  if (result.success) {
    return { success: true, data: result.data };
  }

  const errors: Record<string, string> = {};
  for (const issue of result.error.issues) {
    const key = issue.path.length > 0 ? String(issue.path[0]) : '_form';
    if (!errors[key]) errors[key] = issue.message;
  }

  return { success: false, errors };
}
