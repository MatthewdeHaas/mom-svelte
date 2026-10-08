# Handoff — Track 3 Step 6: Cutover (Rust Backend Flip + TS Query Deletion)

**Status at handoff:** orientation complete, zero edits made. Next action is Phase 1 (expand `_rust-dispatch.ts`). Four locked decisions already approved by the user: (a) wrapper port, (b) non-optional `caller`, (c) single commit on `main`, (d) co-locate interfaces inside `_rust-dispatch.ts`.

**Approved plan:** `~/.claude/plans/snoopy-pondering-quail.md` — read it in full before touching code. This handoff is the complement, not a replacement: it records what the plan's narrative got wrong, what the orientation pass turned up, and the exact ordering for execution.

---

## What the plan narrative got wrong (critical — do not skip)

The plan file was authored on partial information. These two corrections reshape the scope:

1. **`_rust-dispatch.ts` does NOT re-export domain functions today.** It exports only utilities (`internalJson`, `internalFetch`, `useRustBackend`, `isInternalApiError`, `filtersToQueryString`, `normalizeBody`, `dateToIso`, `serializeFilters`, type-only `InternalCaller`). Routes import domain functions directly from `$lib/server/queries/<domain>`. `git rm`-ing the 13 TS files without first adding wrappers to the dispatcher would break every route.

2. **Most routes don't thread a caller today.** Of 22 route files that import from queries, only ~6 pass a `caller`. The remaining ~16 call `getOrders(pool, filters)` with no caller, so the existing Rust-dispatch branch (`if (caller && await useRustBackend(...))`) is never taken — flipping the flag alone changes nothing. Threading `locals.user` into a caller across every route is a prerequisite for the flag flip.

So Step 6 is three mechanical passes landing in **one** commit:
- **Phase 1** — wrapper port (expand `_rust-dispatch.ts`)
- **Phase 2** — route refresh (drop `pool`, add `caller`, update imports)
- **Phase 3** — flag flip + file delete + migration 023

---

## In-force constraints (preserve verbatim)

- **Don't touch Better Auth files.** `auth.$context` calls (`createUser`, `linkAccount`, `createAccount`, `updatePassword`, `findAccounts`) stay in TS — they're not an HTTP surface.
- **Don't touch `pagination.ts`** — it exports `parsePaginationParams` / `paginationSQL` / `buildPaginatedResult`; routes use `parsePaginationParams` directly. The other two die with the query files but the file itself stays.
- **Don't touch `_rust-dispatch.ts`'s existing lazy-proxy pattern for `internalFetch`/`internalJson`.** `$env/dynamic/private` can't load under vitest; the lazy proxy is why tests pass. Preserve it when adding wrappers — wrappers call the already-proxied `internalJson`/`internalFetch`, they don't import `$env` themselves.
- **Don't delete migration files 013–022** — historical record. Only ADD migration 023.
- **Don't start Tracks 4/5/6.**
- **Rollback discipline:** migration 023 alone is NOT a fallback. Rollback requires BOTH flipping the flag back to FALSE AND `git revert` to restore the TS files. State this in both the migration comment and the commit message (plan already specifies the wording).
- **Use pnpm**, not npm. **Use Docker for Postgres** (`docker compose up -d db`), never raw `postgres`/`pg_ctl`.
- **Single commit on `main`.** No intermediate commits, no branching unless something unexpectedly destructive comes up (then surface it, don't just do it).

---

## Phase 1 — Expand `src/lib/server/queries/_rust-dispatch.ts` (234 → ~900 LOC)

Land in this order so each addition compiles standalone:

### 1a. Imports and shared helpers

- Add `import { ConflictError } from '$lib/server/errors';` at the top (sibling to the existing imports). Do NOT re-export it — routes import `ConflictError` directly from `$lib/server/errors` already; keep that path. The dispatcher only needs it to construct instances inside the per-domain conflict-aware wrappers.
- Add shared `internalJsonOrNull<T>` helper (lift from `orders.ts:47-65`, drop the comment references to orders). Pattern:
  ```ts
  async function internalJsonOrNull<T>(path, caller, opts?) {
    try { return await internalJson<T>(path, caller, opts); }
    catch (err) { if (isInternalApiError(err) && err.status === 404) return null; throw err; }
  }
  ```
- Add four **per-domain** `internalJsonConflictAware` variants — DO NOT collapse into one. Each preserves its exact SQL-path message verbatim so the `instanceof ConflictError` catches in routes don't need to inspect the message:
  - companies: `'A company with this name already exists'`
  - recipes: `'A recipe with this name already exists'`
  - ingredients: `'An ingredient with this name already exists'`
  - allergens: `'An allergen with this name already exists'`

### 1b. Co-located interfaces

Lift these verbatim (field-for-field) from their current homes. They move, not copy — the TS files are deleted in Phase 3, but during Phase 1 both files will have the type defined; TS is fine with that as long as routes still import from the old location until Phase 2. To avoid duplicate-identifier compile errors during Phase 1, give the dispatcher copies unique local names if needed, OR land Phase 1 and Phase 2 for a single domain together (recommended — see "Execution strategy" below).

- `OrderFilters` ← `orders.ts:71`
- `RecipeMargin`, `InvoiceLineItem`, `InvoiceSummary` ← `financials.ts:52,61,70`
- Eight insights interfaces ← `insights.ts`: `KPIMetrics`, `VolumeSeries`, `TopRecipe`, `CompanyBreakdown`, `SourceMix`, `IngredientDemand`, `AttendanceDayRate`, `AttendanceByCompany`
- `ExcelRowSchema` ← `imports.ts:31` (needed? only if any route still imports it — grep first. Likely dies with `imports.ts` since the Rust-forwarding import route doesn't need the schema client-side)

### 1c. Date rehydration helpers

- `hydrateMenu(m)` — restore `start_date`, `end_date`, `created_at` as `Date` instances (`menus.test.ts` asserts `instanceof Date`).
- `hydrateAttendance(a)` — restore `serving_date` as `Date` (`persons.test.ts:502` asserts).
- Wrappers for `getMenus`, `getMenuById`, `getMenuWithRecipes`, `getMenusWithRecipes`, `getAttendanceByDate`, `upsertAttendanceBatch` must pipe their result through the appropriate hydrator before returning.

### 1d. Wrappers — ~55 total, grouped by domain

Signature pattern: lift the exact Rust-path branch from the top of each TS function, strip the `db` param, make `caller` non-optional, drop the `useRustBackend` guard. Example:

```ts
// From current orders.ts:87-95:
//   if (caller && (await useRustBackend('orders', caller))) {
//     const qs = filtersToQueryString(filters as Record<string, unknown>);
//     return internalJson<OrderSummary[]>(`/api/orders${qs}`, caller);
//   }
// Becomes:
export async function getOrders(filters: OrderFilters, caller: InternalCaller): Promise<OrderSummary[]> {
  const qs = filtersToQueryString(filters as Record<string, unknown>);
  return internalJson<OrderSummary[]>(`/api/orders${qs}`, caller);
}
```

Full list (from the plan, re-verified against routes):
- **orders** (10): `getOrders`, `getOrdersPaginated`, `getOrderById`, `getOrderCountsByStatus`, `createOrder`, `updateOrder`, `confirmOrder`, `fulfillOrder`, `cancelOrder`, `setOrdersStatusBatch`
- **recipes** (11): `getRecipes`, `getRecipeById`, `getRecipeWithIngredients`, `getRecipesWithIngredientsByIds` (Vec→Map transform), `getRecipesWithIngredientsPaginated`, `createRecipe`, `updateRecipe`, `deactivateRecipe`, `addRecipeIngredient`, `removeRecipeIngredient`, `setRecipeIngredients`
- **companies** (7): `getCompanies`, `getCompaniesPaginated`, `getCompanyById`, `getUsersByCompany`, `createCompany`, `updateCompany`, `deactivateCompany`
- **ingredients** (7): `getIngredients`, `getIngredientNames`, `getIngredientById`, `getIngredientWithAllergens`, `getIngredientsWithAllergensPaginated`, `createIngredient`, `updateIngredient`, `deactivateIngredient`, `setIngredientAllergens` — three-state `cost_per_unit` via `'cost_per_unit' in data` sentinel preserved in `normalizeBody` or the wrapper (verify)
- **allergens** (7): `getAllergens`, `getAllergenById`, `getAllergenByName` (yes — IS called by routes), `createAllergen`, `updateAllergen`, `getAllergenRefCount` (yes — IS called by routes; returns `{ ingredients: number, persons: number }`), `deleteAllergen`
- **persons** (10): `getPersonsByCompany`, `getPersonById`, `getPersonWithAllergens`, `getPersonsWithAllergensByCompany`, `createPerson`, `updatePerson`, `deactivatePerson`, `setPersonAllergens`, `getAttendanceByDate`, `upsertAttendanceBatch`
- **invites** (5): `getInviteByToken`, `getInvitesByCompany`, `createInvite`, `markInviteUsed` (swallows 404), `revokeInvite` (swallows 404)
- **menus** (9): `getMenus`, `getMenuById`, `getMenuWithRecipes`, `getMenusWithRecipes`, `getMenuRecipes`, `createMenu`, `updateMenu`, `deactivateMenu`, `setMenuSlot`, `clearMenuSlot`
- **financials** (2): `getRecipesWithMarginsPaginated`, `getInvoiceData`
- **insights** (8): `getKPIMetrics`, `getOrderVolumeSeries`, `getTopRecipes`, `getCompanyBreakdown`, `getSourceMix`, `getIngredientDemand`, `getAttendanceRate`, `getAttendanceByCompany`

### 1e. Explicitly SKIP (zero route call sites — die with their files, no wrapper needed)

`insertOrdersBatch`, `getOrdersByShipmentDate`, `getRecipeByName`, `getRecipeIngredients`, `getRecipesWithIngredients` (non-paginated), `isRecipeActive`, `getCompanyByName`, `getPersonAllergens`, `addPersonAllergen`, `removePersonAllergen`, `addIngredientAllergen`, `removeIngredientAllergen`, `getIngredientsWithAllergens` (non-paginated), `getActiveMenuForDate`, `getMenuSlot`, `setAllMenuSlots`, `getRecipesWithMargins` (non-paginated). Also `importOrdersFromRows`, `importRecipesFromRows`, `ExcelRowSchema` — the entire `imports.ts` and `recipe-imports.ts` files die; routes already have `internalFetch('/api/import/commit', ...)` branches.

---

## Phase 2 — Route refresh (~22 routes + 3 lib helpers)

Mechanical pattern for every file:
1. Change `import { getFoo } from '$lib/server/queries/<domain>'` → `from '$lib/server/queries/_rust-dispatch'`
2. Add `callerFromLocals` import from `$lib/server/api-client` (confirmed present at `api-client.ts:50-61`)
3. Add `const caller = callerFromLocals(locals);` once per handler
4. Drop leading `pool` from every call site
5. Add `caller` as the last argument
6. Remove `pool` import if no longer referenced
7. Keep `ConflictError` imports from `$lib/server/errors` as-is

### Specific route gotchas (do not forget)

- **`admin/companies/[id]/+page.server.ts`** — delete the duplicate `callerFromLocals` at lines 61–67 (the one at `api-client.ts:50-61` is the keeper). Then: `deactivate` action (line 111) and `reactivate` action (line 121) call `getCompanyById(pool, params.id)` / `updateCompany(pool, params.id, {active:true})` **without caller**. Add `const caller = callerFromLocals(locals);` to both actions and thread it through.
- **`invite/[token]/+page.server.ts`** — has inline caller construction at lines 39–43; swap to `callerFromLocals(locals)`.
- **`admin/orders/import/+page.server.ts`** — delete the SQL fall-through branch (STEP 2 + STEP 3 blocks, `importOrdersFromRows` call). Always forward to `/api/import/commit`. The `useRustBackend` guard can be deleted too — flag is TRUE after Phase 3.
- **`admin/recipes/new/+page.server.ts`** — same shape. Delete the SQL fuzzy-match path (uses `getIngredientNames`, dies with it) and the `importRecipesFromRows` call. Keep `createRecipe` (routes to wrapper) and the `/api/recipe-imports/*` forwarding branch.
- **`admin/orders/[id]/+page.server.ts`** — no `locals` destructured in action signatures today. Add `{ request, params, locals }` to each action.
- **Six `admin/insights/api/*/+server.ts` endpoints** — none destructure `locals` today. Add it + `callerFromLocals`.
- **Three `admin/exports/*/preview/+server.ts` endpoints** — no caller today in previews. Add it.
- **`client/attendance/+page.server.ts`** — has a direct `pool.query` on the `attendance` table (lines ~50–56) that is NOT wrapped by any query file. **Flag this during Phase 2**: likely needs to switch to the `getAttendanceByDate` wrapper (shape differs — may need composition over multiple dates) OR be left as a known gap for a follow-up track. Do not silently leave raw SQL in a route if the plan's goal is "zero business logic in Node" — surface it to the user, don't guess.

### 3 lib helpers (same mechanical pattern)

- `src/lib/excel/export/production-order.ts` — `generateProductionOrderSheet(db, from, to, caller?)` → drop `db`, make `caller` required
- `src/lib/excel/export/shopping-list.ts` — same shape; also uses `getRecipesWithIngredientsByIds`
- `src/lib/pdf/packing-slip.ts` — `generatePackingSlips(db, referenceDate, caller?)` → drop `db`, make `caller` required

Each already has a Rust-forwarding branch using `/api/exports/*`; after cutover that branch is the only path. The SQL fall-through can be deleted entirely.

---

## Phase 3 — Delete + flag flip

```bash
git rm src/lib/server/queries/{orders,recipes,companies,ingredients,allergens,persons,invites,menus,financials,insights,imports,recipe-imports}.ts
```

Keep `_rust-dispatch.ts` and `pagination.ts`.

Create `scripts/migrations/023_cutover_rust_backend.sql`:

```sql
-- 023_cutover_rust_backend.sql
--
-- Track 3 Step 6: flip every use_rust_backend_* flag to
-- enabled_global = TRUE. From this point forward the TS query
-- modules are dead code (removed in the same commit).
--
-- Rollback: flip these back to FALSE AND `git revert` to restore
-- the TS modules. The dispatcher pattern alone is not enough — the
-- TS files have to exist for the SQL path to run.

UPDATE feature_flag SET enabled_global = TRUE
 WHERE name LIKE 'use_rust_backend_%';
```

Then `pnpm rust:sync-migrations` to copy into `admin-operations-rust/migrations/`.

Expected flag count: 13 or 14 (verify with `psql $DATABASE_URL -c "SELECT name FROM feature_flag WHERE name LIKE 'use_rust_backend_%' ORDER BY name"` — users flag may have been added in `844702b1`).

---

## Phase 4 — Verification (run in order; stop on first failure)

1. `docker compose up -d db` (ensure Postgres is up)
2. `pnpm db:reset-test && pnpm run migrate` — confirm `psql $DATABASE_URL -c "SELECT name, enabled_global FROM feature_flag WHERE name LIKE 'use_rust_backend_%'"` shows all TRUE
3. `pnpm check` → **0 warnings** (this is the compile-time check that every call site has a caller — if it fails, a route is missing `callerFromLocals`)
4. `pnpm test` → full green (baseline: 437/437)
5. `pnpm test:e2e` → full green
6. `cargo test --manifest-path admin-operations-rust/Cargo.toml --workspace` → full green (baseline: 65)
7. UI spot-checks (manual, from the plan):
   - `/admin/orders` loads, filtering works
   - `/admin/recipes` loads, margins compute
   - `/admin/companies/<id>` loads with users embedded
   - Order import works end-to-end with a real workbook
   - Export production-order xlsx, shopping-list xlsx, packing-slip pdf — all return non-empty files
   - `/admin/flags` shows all flags enabled globally
8. `rg -n 'pool\.query|client\.query' src/` returns zero business-logic hits outside Better Auth internals (`client/attendance` raw SQL from Phase 2 must be resolved by here)

---

## Execution strategy (recommended)

To keep each `pnpm check` run green and the diff reviewable, land **one domain at a time end-to-end** rather than all of Phase 1 before any of Phase 2:

1. Pick a small domain first (invites — 5 wrappers, 2 route files). Add its wrappers to `_rust-dispatch.ts`, update its routes, run `pnpm check`.
2. If it goes clean, do allergens, then companies, then persons, menus, insights, financials, ingredients, recipes, orders (orders last — biggest surface).
3. Leave the TS query files on disk through all of Phase 1+2. Delete them only in Phase 3.
4. Phase 1 interfaces will temporarily collide with the originals in the still-present TS files. Resolve by: moving the interface to the dispatcher first, updating the TS file to re-export from the dispatcher during the transition, then deleting the TS file in Phase 3. OR: do the domain's wrapper port + route refresh + TS file delete all in one go per-domain, so there's no collision window. The second approach keeps `git diff` cleaner.

Either way, the final commit on `main` must be ONE commit — stage everything up and commit once with the plan's wording:

```
feat(rust): Track 3 Step 6 — cutover

Flip every use_rust_backend_* flag to enabled_global = TRUE and
delete the TS query modules. SvelteKit is now a pure HTTP/SSR
shell: route handlers forward to Rust via _rust-dispatch.ts, with
zero business logic or raw SQL in the Node service.

Rollback: flip 023's UPDATE back to FALSE and `git revert` to
restore the TS modules. The dispatcher alone is not a fallback —
the TS files have to exist for the SQL path to run.

Closes Track 3.
```

---

## Risks to watch during execution

- **Test files importing from queries.** Not yet grepped. If any `*.test.ts` depends on the SQL path for coverage, `pnpm test` will surface it. Those tests either rewrite against the Rust path or get deleted — don't guess.
- **`ConflictError` shape parity.** Routes catch via `instanceof ConflictError`. The per-domain `internalJsonConflictAware` variants must `throw new ConflictError(message)` so `instanceof` still works. Do not substitute `InternalApiError` with status 409 — the catch sites won't match.
- **`client/attendance` raw SQL** — see Phase 2 gotchas. Surface this to the user before Phase 3; don't silently leave it.
- **Compile-time break if a route has no `locals.user`.** The non-optional `caller` signature makes `pnpm check` fail loudly on any such route. If one shows up, pause and surface it — don't fabricate a caller or add a null-guard.

---

## Pre-flight to run on arrival (fresh session)

```bash
cd /Users/matthewdehaas/Professional/meals-on-the-move/admin-operations-svelte
git status                                                 # confirm clean except pre-existing untracked
git log --oneline -5                                       # confirm 844702b1 is HEAD
cat ~/.claude/plans/snoopy-pondering-quail.md | head -50   # re-read the plan
cat HANDOFF_CUTOVER.md                                     # this file
docker compose up -d db                                    # Postgres up
pnpm check                                                 # baseline warning count (should be 49 per memory)
```

Then start Phase 1 with the invites domain as the pilot.
