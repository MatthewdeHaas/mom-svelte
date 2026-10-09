# Handoff — Track 5c: drive `pnpm test:e2e` to green

Fresh-session doc. Picks up after Track 5a landed the Playwright plumbing
(SvelteKit + Rust API webServers, migration-ledger backfill). With 5a in,
55 of 83 e2e specs pass; this track resolves the remaining **27 failures**
and the single port-contract bug they surfaced in the production UI.

## State at handoff

- **Branch:** `main` after the 5a commit. Nothing from 5c is in-tree yet.
- **Working directory:** must be clean before starting.
- **Green:** `pnpm check` 0/0/0, `pnpm test` 7/7, `cargo test --workspace`
  524/524, `cargo clippy --workspace -- -D warnings` 0, `cargo fmt --check` 0.
- **Red:** `pnpm test:e2e` — **27 failures, 55 passed, 1 did not run**.

All 27 failures are **pre-existing debt from the Rust port**, not from 5a.
They sort into three clusters by root cause.

## Standing user directives (verbatim — do NOT deviate)

- *"I would like everything to be in Rust. If it requires business logic or
  data layer interaction, it should be in Rust. Typescript should be for
  client side interaction and handling http interactions, and should not
  have any logic at all."*
- *"Please don't say nix develop. Just run cargo ... as it is on the repo's
  path with the flake."*
- *"no cleartext stored anywhere"*
- Commits stay local until the user explicitly asks to push.

## Flake / cargo invocation

```sh
bash -c '. .direnv/flake-profile-*.rc && cd admin-operations-rust && cargo ...'
```

For Rust integration tests, export `DATABASE_URL` first:
```sh
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/meals_test
```

Postgres via `docker compose up -d db`.

## Cluster 1 — Exports contract (20 failures)

### Root cause

The Rust port of exports (`f8bf2716`) changed the public query shape on
`/admin/exports/{production-order,shopping-list}` from
`?shipment_date=YYYY-MM-DD` (pre-port: single-date, "week-of" semantics)
to `?from=YYYY-MM-DD&to=YYYY-MM-DD` (what the Rust handler takes). The
specs AND the production UI still emit the old shape:

- `src/routes/admin/exports/+page.svelte:81` — day-mode: `${base}?shipment_date=${dates[i]}`
  → 400 "from and to are required" when a user clicks "Day" and then any
  export button. **This is a live user-facing bug**, not just a test bug.
- `tests/e2e/admin/exports.spec.ts` lines 88, 99, 108, 122, 148, 158, 167,
  177, 200, 209, 219, 229 — 16 call sites, all `?shipment_date=`.
- `tests/e2e/admin/backfill-roundtrip.spec.ts` lines 266, 300, 334 —
  3 more call sites, same shape.

Preview routes (`/preview/+server.ts` for all three endpoints) already
accept both `?week_of=` and `?shipment_date=` via a local `resolve()`
helper — the main routes just never got the same aliasing.

### Fix

Extract one shared resolver and apply it everywhere the pattern repeats.

**New file:** `src/lib/server/exports/resolve-range.ts`

```ts
import { error } from '@sveltejs/kit';
import { weekBoundsEST } from '$lib/utils/dates';

const DATE_RE = /^\d{4}-\d{2}-\d{2}$/;

export interface ExportRange { from: string; to: string; }

/** For production-order + shopping-list (xlsx). */
export function resolveExportRange(url: URL): ExportRange {
  const from = url.searchParams.get('from');
  const to = url.searchParams.get('to');
  if (from && to) {
    if (!DATE_RE.test(from) || !DATE_RE.test(to))
      throw error(400, 'from and to must be YYYY-MM-DD');
    return { from, to };
  }
  const weekOf = url.searchParams.get('week_of');
  if (weekOf) {
    if (!DATE_RE.test(weekOf)) throw error(400, 'week_of must be YYYY-MM-DD');
    return weekBoundsEST(weekOf);
  }
  const shipmentDate = url.searchParams.get('shipment_date');
  if (shipmentDate) {
    if (!DATE_RE.test(shipmentDate))
      throw error(400, 'shipment_date must be YYYY-MM-DD');
    return weekBoundsEST(shipmentDate);
  }
  throw error(400, 'provide from+to, week_of, or shipment_date (YYYY-MM-DD)');
}

/** For packing-slips — Rust takes a single `?date=`. */
export function resolvePackingSlipsDate(url: URL): string {
  const date =
    url.searchParams.get('date') ??
    url.searchParams.get('week_of') ??
    url.searchParams.get('shipment_date');
  if (!date) throw error(400, 'provide date, week_of, or shipment_date (YYYY-MM-DD)');
  if (!DATE_RE.test(date)) throw error(400, 'date must be YYYY-MM-DD');
  return date;
}
```

**Apply to** the three main export routes:

- `src/routes/admin/exports/production-order/+server.ts` — swap `const from = ...; const to = ...; if (!from || !to) throw error(400, ...);` for `const { from, to } = resolveExportRange(url);`.
- `src/routes/admin/exports/shopping-list/+server.ts` — same change.
- `src/routes/admin/exports/packing-slips/+server.ts` — swap for `const date = resolvePackingSlipsDate(url);`.

**Fix the UI**: `src/routes/admin/exports/+page.svelte` `buildUrl`:

```ts
function buildUrl(i: number, path: string): string {
  const base = `/admin/exports/${path}`;
  if (modes[i] === 'week') return `${base}?week_of=${dates[i]}`;
  // Day mode: xlsx exports want a range; packing-slips wants a reference date.
  if (path === 'packing-slips') return `${base}?date=${dates[i]}`;
  return `${base}?from=${dates[i]}&to=${dates[i]}`;
}
```

Specs stay unchanged — the routes now accept their shape via the alias.

### Verification

```sh
pnpm test:e2e -g "Production Order export"      # 4 specs → pass
pnpm test:e2e -g "Shopping List export"          # 4 specs → pass
pnpm test:e2e -g "Packing Slips export"          # 4 specs → pass
pnpm test:e2e tests/e2e/admin/backfill-roundtrip.spec.ts
                                                 # 4 specs → pass
```

Expected net: **20 of 27 failures resolved**. Zero spec edits.

## Cluster 2 — Client portal regressions (6 failures)

### Failing specs

```
tests/e2e/client/portal.spec.ts:88   can add a person and be redirected to their detail page
tests/e2e/client/portal.spec.ts:97   person detail page has allergen checkboxes
tests/e2e/client/portal.spec.ts:109  person name update succeeds
tests/e2e/client/portal.spec.ts:124  deactivating a person redirects to roster list
tests/e2e/client/portal.spec.ts:160  already-ordered slot shows "Ordered" badge and no Order button
tests/e2e/client/portal.spec.ts:310  attendance: checking a box and saving round-trips correctly
```

### Confirmed signal

Running spec `:97` solo (clean DB) reproduces with error-context showing:

```yaml
- heading "500" [level=1]
- paragraph: An unexpected error occurred
```

That's an SSR 500 from `/client/roster/[id]/+page.server.ts` — Rust-port
contract gap, not a selector miss. The other 4 person-CRUD failures very
likely share this root cause (person creation redirects to the same
detail page, which then 500s). The two non-person failures (menu badge,
attendance save) are plausibly separate issues; diagnose them
independently.

### Suggested investigation order

1. Look at the Rust API response for `GET /api/persons/:id/with-allergens`
   when the person has no allergens yet. Compare to what the SvelteKit
   `+page.server.ts` for `/client/roster/[id]` expects. Likely contract
   mismatch (null vs empty array, missing field, etc).
2. The route reaches Rust via `src/lib/server/queries/persons.ts` →
   `_rust-dispatch.ts` wrapper → `internalFetch('/api/persons/:id/with-allergens')`.
   Tail the Rust logs (`tail -f` the webServer stdout or temporarily set
   `RUST_LOG=debug` in `playwright.config.ts`) while running the spec.
3. For the "✓ Ordered" badge (:160): SvelteKit's `/client/menu/+page.server.ts`
   builds slots and checks each against the user's confirmed orders.
   Probably a serving-date filter contract mismatch between the TS
   builder and `GET /api/orders` with the Rust-side query shape.
4. For attendance save-strip (:310): the save form action posts to
   `/client/attendance?/save` which calls `POST /api/attendance/batch`.
   The missing `.success-strip` suggests the action is returning `fail`
   instead of a form-ok — probably a validation shape mismatch.

### Watch out

- `tests/e2e/client/portal.spec.ts` tests are **not** `test.describe.serial`
  — each has its own clean state. Any shared assumption about a
  previously-created person won't hold.
- `clientPage` fixture uses the seeded `E2E_CLIENT_EMAIL` client user tied
  to `E2E_COMPANY_NAME` with the far-future `tomorrowStr` order.
- Pre-5a this cluster also failed, but masked behind the global
  `RUST_API_URL is not set` 500. These issues predate 5a.

### Verification

Each spec runs individually in <10s:
```sh
pnpm test:e2e -g "person detail page has allergen checkboxes"
pnpm test:e2e -g "attendance: checking a box"
pnpm test:e2e -g "already-ordered slot shows"
# etc.
```

End-of-cluster gate:
```sh
pnpm test:e2e tests/e2e/client/portal.spec.ts
```

Expected net: **6 of 27 failures resolved**.

## Cluster 3 — Ingredients + pagination + import (3 failures)

### Failing specs

```
tests/e2e/admin/ingredients.spec.ts:32    Ingredients CRUD → edit the ingredient name
tests/e2e/admin/pagination.spec.ts:110    Pagination → Recipes page → displays pagination controls
tests/e2e/admin/pagination.spec.ts:117    Pagination → Recipes page → search resets page
tests/e2e/admin/import.spec.ts:150        Order import → happy path
tests/e2e/admin/import.spec.ts:191        Order import → error path
```

Note: cluster 3 is actually **5 specs**, not 3 as the earlier triage said.
Rechecked against the final failure list.

### Confirmed signal — Ingredients `:32`

Timeout on `tbody td` filter(hasText: NAME). The spec creates
`E2E-Ingredient-${Date.now()}` on `/admin/ingredients/new`, then navigates
to `/admin/ingredients` and expects to see the row. 30-second wait, nothing
appears. Likely: the ingredients paginated query doesn't include recently-
created rows (sort order, cache, or filter contract).

### Suggested investigation order

**Ingredients** — check `src/lib/server/queries/ingredients.ts` →
`_rust-dispatch.ts`. The list page uses `/api/ingredients/paginated`
with default sort. New ingredient should appear on page 1. Verify:
```sh
curl -s "http://localhost:5174/admin/ingredients?q=E2E" | grep -o 'E2E-Ingredient-[0-9]*'
```
after a `pnpm test:e2e -g "create a new ingredient"` run (which creates
and leaves the row in the DB).

**Pagination (Recipes)** — the pagination test seeds no recipes of its
own; it only visits `/admin/recipes` and expects `data-testid="pagination"`
to be visible. If the global-setup seeded recipes aren't enough to trigger
the pagination component to render (threshold is probably >20 rows?),
the component is skipped. Check `src/routes/admin/recipes/+page.svelte`
pagination gate. Fix is either: seed more test recipes in `global-setup.ts`,
OR always render the pagination control (even if there's only one page).

**Import** — the happy-path test uploads an XLSX. Looking at the import
page's expected success selector (`.success-header`, `.stat-value`), the
test needs the import to actually commit. The error-path test expects
`.importErrors` to render with a count. Both involve the Rust
`/api/import/preview` + `/api/import/commit` endpoints. Suspect: the
preview response shape doesn't match what SvelteKit's import page expects
to render. Run `pnpm test:e2e -g "happy path" -g "import"` and inspect
`test-results/.../error-context.md`.

### Verification

```sh
pnpm test:e2e tests/e2e/admin/ingredients.spec.ts
pnpm test:e2e tests/e2e/admin/pagination.spec.ts
pnpm test:e2e tests/e2e/admin/import.spec.ts
```

Expected net: **5 of 27 failures resolved** (3 were triaged incorrectly;
actual count is 5).

## Verification gates (all must pass before commit)

```sh
pnpm check                                     # 0 / 0 / 0
pnpm test                                      # 7/7
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/meals_test
bash -c '. .direnv/flake-profile-*.rc && cd admin-operations-rust && cargo fmt --check'
bash -c '. .direnv/flake-profile-*.rc && cd admin-operations-rust && cargo clippy --workspace --all-targets -- -D warnings'
bash -c '. .direnv/flake-profile-*.rc && cd admin-operations-rust && cargo test --workspace'
pnpm test:e2e                                  # 0 failures, 83/83 pass  ← the goal
```

## Commits

5c probably wants **three logical commits**, not one — the clusters touch
very different surfaces and are easier to review split:

1. `fix(exports): accept legacy ?shipment_date= and ?week_of= aliases on main routes`
   — resolver + the three routes + the UI buildUrl.
2. `fix(client-portal): close Rust-port contract gaps on roster/menu/attendance`
   — the 6 client-portal failures (likely multiple sub-fixes).
3. `fix(admin): close ingredients list, recipes pagination, order import gaps`
   — cluster 3.

Each commit should include running the subset of specs that cluster
covers to prove it green, plus the full `pnpm test:e2e` on the last
commit to confirm no regressions.

Stay local; do NOT push.

## After 5c — what's next

Preserved from the original plan file:

1. **Track 5b — Importer property tests** (~0.5 day).
   `admin-operations-rust/crates/excel/tests/import_proptest.rs` using
   `proptest`.
2. **Track 6 — Docs** (~0.5–1 day). `docs/auth.md`, `docs/excel-import.md`,
   `docs/architecture.md`, `docs/ops.md`.
3. **Track 7 — Feature-flag TUI** (deferred).
4. **Track 4 — CI/CD + Infisical** (LAST).

## Pointers

- Track 5a handoff (predecessor): `docs/handoff-track5a.md`
- Full plan: `/Users/matthewdehaas/.claude/plans/i-need-to-do-fluttering-hellman.md`
- Prior transcript (where this triage happened): check your transcripts dir
- Rust exports handler: `admin-operations-rust/crates/api/src/exports.rs`
- Preview-route resolve helpers (reference pattern):
  - `src/routes/admin/exports/production-order/preview/+server.ts:7-15`
  - `src/routes/admin/exports/shopping-list/preview/+server.ts:10-18`
  - `src/routes/admin/exports/packing-slips/preview/+server.ts:7-15`
- Week-of helper: `src/lib/utils/dates.ts::weekBoundsEST`
- SvelteKit → Rust client: `src/lib/server/api-client.ts`
- Playwright config: `playwright.config.ts`
- Playwright globalSetup: `tests/e2e/global-setup.ts`
