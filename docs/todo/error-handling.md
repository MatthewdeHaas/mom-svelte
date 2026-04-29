# TODO: Centralise Postgres unique constraint error handling

## Problem

Postgres error code `23505` (unique constraint violation) is currently caught
directly in route action handlers:

- `src/routes/admin/ingredients/new/+page.server.ts`
- `src/routes/admin/ingredients/[id]/+page.server.ts`

This leaks database implementation details into the UI layer. Any future entity
with a unique constraint will require the same pattern to be re-implemented in
each action, increasing the risk of inconsistent error messages and making the
codebase harder to maintain.

## Desired solution

1. Create `src/lib/server/errors.ts` exporting a `ConflictError` class:

```typescript
export class ConflictError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'ConflictError';
  }
}
```

2. In each query function that can violate a unique constraint, catch `23505`
   and rethrow as `ConflictError`. Affected functions at time of writing:

   - `createIngredient` — unique on `name`
   - `updateIngredient` — unique on `name`
   - `createCompany` — unique on `name` (if constrained)
   - `updateCompany` — same
   - `createRecipe` — unique on `name` (if constrained)
   - `updateRecipe` — same

   Pattern:
   ```typescript
   import { ConflictError } from '$lib/server/errors';

   try {
     const result = await db.query(...);
     return result.rows[0];
   } catch (err: any) {
     if (err?.code === '23505') throw new ConflictError('A record with this name already exists');
     throw err;
   }
   ```

3. Update each affected action handler to catch `ConflictError` instead of
   checking `err?.code` directly:

   ```typescript
   import { ConflictError } from '$lib/server/errors';

   try {
     await createIngredient(pool, { name, active: true });
   } catch (err) {
     if (err instanceof ConflictError) {
       return fail(422, { errors: { name: err.message }, values: { name, allergenIds } });
     }
     throw err;
   }
   ```

## Scope

Low effort, mechanical refactor. No UI changes required. Can be done in one
sitting once the UI layer is complete.

## Added

2026-04-28
