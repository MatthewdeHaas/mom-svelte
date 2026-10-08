// Flag-gated dispatcher shim for the Rust backend.
//
// Each exported query function in this directory gains an optional
// `caller?: InternalCaller`. When the caller is passed AND the
// `use_rust_backend_<domain>` flag is on for that caller, the query
// forwards to the Rust service; otherwise it falls through to the
// existing SQL. Routes stay untouched in the parity-window commit;
// threading `caller` through is a follow-up pass per route.
//
// The flag check is cached for 30 s per `(user, company, flag)` so
// a request handler that fans out across many queries pays the HMAC
// roundtrip once, not N times. Cache is in-process; a redeploy or
// `/admin/flags` toggle is reflected on the next 30 s window.
//
// Failures (Rust unreachable, HMAC rejected, flag endpoint down) must
// NOT block the SQL path — this shim returns `false` on any error and
// logs via `logger.warn` with throttling so a quiet Rust pod doesn't
// flood logs.

// NOTE: api-client.ts imports `$env/dynamic/private` at module load, which
// vitest can't resolve (its config only aliases `$lib`). To keep the test
// loader happy when the flag is off (caller omitted → Rust path never taken),
// we type-only import the types and lazy-load the runtime module inside the
// functions that actually hit the wire. Routes still get the same ergonomics;
// test suites that transitively import orders.ts no longer explode.
import type { InternalCaller } from '$lib/server/api-client';
import { ConflictError } from '$lib/server/errors';
import type {
	Allergen,
	Attendance,
	AttendanceWithPerson,
	Company,
	DayOfWeek,
	InsertAllergen,
	InsertCompany,
	InsertMenu,
	InsertMenuRecipe,
	InsertPerson,
	InsertOrder,
	Invite,
	InsertInvite,
	Menu,
	MenuRecipe,
	MenuRecipeWithName,
	MenuWithRecipes,
	Order,
	OrderDetail,
	OrderSource,
	OrderStatus,
	OrderSummary,
	Person,
	PersonWithAllergens,
	UpdateCompany,
	UpdateMenu,
	UpdateOrder,
	UpdatePerson,
	UpsertAttendance,
	UserWithCompany,
} from '$lib/types';
import type { PaginatedResult, PaginationParams } from './pagination';

type ApiClient = typeof import('$lib/server/api-client');
let _apiClient: ApiClient | undefined;
async function apiClient(): Promise<ApiClient> {
	if (!_apiClient) _apiClient = await import('$lib/server/api-client');
	return _apiClient;
}

// Lazy logger too — `$lib/server/logger` imports `$app/environment`, which
// vitest can't resolve. We only log on the Rust path (which tests never take
// with the flag off), so loading it on demand is fine.
type Logger = (typeof import('$lib/server/logger'))['default'];
let _logger: Logger | undefined;
async function logger(): Promise<Logger> {
	if (!_logger) _logger = (await import('$lib/server/logger')).default;
	return _logger;
}

/** Lazy proxy for `internalFetch`. Dispatchers in sibling files import this. */
export async function internalFetch(
	...args: Parameters<ApiClient['internalFetch']>
): ReturnType<ApiClient['internalFetch']> {
	const m = await apiClient();
	return m.internalFetch(...args);
}

/** Lazy proxy for `internalJson<T>`. */
export async function internalJson<T>(
	...args: Parameters<ApiClient['internalJson']>
): Promise<T> {
	const m = await apiClient();
	return m.internalJson<T>(...args);
}

/**
 * Narrow `unknown` to the api-client's `InternalApiError`. We can't re-export
 * the class directly without re-triggering the `$env` import, so dispatchers
 * use this predicate instead of `err instanceof InternalApiError`.
 */
export function isInternalApiError(err: unknown): err is Error & { status: number; body: unknown } {
	return (
		typeof err === 'object' &&
		err !== null &&
		(err as { name?: string }).name === 'InternalApiError' &&
		typeof (err as { status?: unknown }).status === 'number'
	);
}

// -------------------------------------------------------------------
// Flag cache
// -------------------------------------------------------------------

interface CacheEntry {
	enabled: boolean;
	expiresAt: number;
}

const FLAG_TTL_MS = 30_000;
const flagCache = new Map<string, CacheEntry>();

function cacheKey(domain: string, caller: InternalCaller): string {
	return `${caller.userId}|${caller.companyId ?? ''}|${domain}`;
}

// Per-flag warn throttle — log at most once per minute per flag name
// so a dead Rust pod doesn't pin the log pipeline at warn volume.
const warnCooldown = new Map<string, number>();
const WARN_TTL_MS = 60_000;

async function throttledWarn(key: string, obj: Record<string, unknown>, msg: string) {
	const now = Date.now();
	const last = warnCooldown.get(key) ?? 0;
	if (now - last < WARN_TTL_MS) return;
	warnCooldown.set(key, now);
	// Logger loads `$lib/server/logger` → `$app/environment`, which vitest
	// can't resolve. Swallow the import failure so a flag-check error path
	// never bubbles up as an unhandled rejection under tests. Production
	// paths still log normally — the import only fails inside vitest.
	try {
		const log = await logger();
		log.warn(obj, msg);
	} catch {
		// Fallback for tests / environments without `$app/environment`.
		// eslint-disable-next-line no-console
		console.warn(`[rust-dispatch] ${msg}`, obj);
	}
}

/**
 * Returns `true` iff `use_rust_backend_<domain>` is enabled for the
 * given caller AND the flag endpoint answered. Any error (Rust down,
 * HMAC mismatch, timeout) returns `false` — the dispatcher falls
 * through to SQL and the caller is none the wiser.
 */
export async function useRustBackend(
	domain: string,
	caller: InternalCaller
): Promise<boolean> {
	const key = cacheKey(domain, caller);
	const now = Date.now();
	const hit = flagCache.get(key);
	if (hit && hit.expiresAt > now) {
		return hit.enabled;
	}

	let enabled = false;
	try {
		const { internalJson } = await apiClient();
		const r = await internalJson<{ enabled: boolean }>('/api/flags/check', caller, {
			method: 'POST',
			body: JSON.stringify({ name: `use_rust_backend_${domain}` }),
		});
		enabled = r.enabled;
	} catch (err) {
		throttledWarn(
			`flag:${domain}`,
			{ err, domain },
			'rust flag check failed; falling back to TS path',
		);
		enabled = false;
	}

	flagCache.set(key, { enabled, expiresAt: now + FLAG_TTL_MS });
	return enabled;
}

// -------------------------------------------------------------------
// Filter / body serialization
// -------------------------------------------------------------------

/**
 * Serialize a filter object into `[string, string][]` ready for
 * `URLSearchParams`. Drops undefined/null; converts `Date` to
 * ISO-date (`YYYY-MM-DD`) since every date filter the orders surface
 * accepts is a date-only value. Booleans become `"true"`/`"false"`.
 *
 * Exported so dispatchers in sibling query files can reuse it without
 * reimplementing the shuffling.
 */
export function serializeFilters(
	filters: Record<string, unknown>
): [string, string][] {
	const out: [string, string][] = [];
	for (const [k, v] of Object.entries(filters)) {
		if (v === undefined || v === null) continue;
		if (v instanceof Date) {
			// Pin to UTC-aware ISO date — matches the pg driver's
			// behavior for `::date` casts and keeps the Rust NaiveDate
			// parser happy.
			const yyyy = v.getUTCFullYear().toString().padStart(4, '0');
			const mm = (v.getUTCMonth() + 1).toString().padStart(2, '0');
			const dd = v.getUTCDate().toString().padStart(2, '0');
			out.push([k, `${yyyy}-${mm}-${dd}`]);
		} else if (typeof v === 'boolean') {
			out.push([k, v ? 'true' : 'false']);
		} else {
			out.push([k, String(v)]);
		}
	}
	return out;
}

/**
 * Build a query string from a filter object. Returns the leading `?`
 * only when at least one param is present.
 */
export function filtersToQueryString(filters: Record<string, unknown>): string {
	const pairs = serializeFilters(filters);
	if (pairs.length === 0) return '';
	const sp = new URLSearchParams();
	for (const [k, v] of pairs) sp.append(k, v);
	return `?${sp.toString()}`;
}

/**
 * Normalize a `Date` field in a POST/PATCH body down to a wire-safe
 * `YYYY-MM-DD` string. Non-Date values pass through. Idempotent for
 * strings and `null`.
 */
export function dateToIso(v: unknown): unknown {
	if (v instanceof Date) {
		const yyyy = v.getUTCFullYear().toString().padStart(4, '0');
		const mm = (v.getUTCMonth() + 1).toString().padStart(2, '0');
		const dd = v.getUTCDate().toString().padStart(2, '0');
		return `${yyyy}-${mm}-${dd}`;
	}
	return v;
}

/**
 * Clone + normalize every Date field in a body object (shallow). Also
 * drops `undefined` so JSON doesn't carry invalid trailing commas.
 */
export function normalizeBody<T extends Record<string, unknown>>(body: T): Record<string, unknown> {
	const out: Record<string, unknown> = {};
	for (const [k, v] of Object.entries(body)) {
		if (v === undefined) continue;
		out[k] = dateToIso(v);
	}
	return out;
}

// -------------------------------------------------------------------
// Shared result-shape helpers
// -------------------------------------------------------------------

/**
 * 404-aware wrapper. The deleted TS SQL path returned `null` on miss
 * for row-lookup functions (`getFooById`, etc.); the Rust service
 * returns 404. Translate so wrappers stay behaviourally identical.
 */
async function internalJsonOrNull<T>(
	path: string,
	caller: InternalCaller,
	opts?: InternalFetchOptions
): Promise<T | null> {
	try {
		return await internalJson<T>(path, caller, opts);
	} catch (err) {
		if (isInternalApiError(err) && err.status === 404) return null;
		throw err;
	}
}

// Re-import the options type via a type-only path so we don't pull
// $env/dynamic/private into vitest's loader. Mirrors the lazy-proxy
// pattern above for runtime symbols.
type InternalFetchOptions = import('$lib/server/api-client').InternalFetchOptions;

/**
 * Per-domain conflict-aware wrapper. Translates the Rust service's
 * 409 Conflict response into the SQL-path's `ConflictError` so route
 * `instanceof ConflictError` catches work verbatim. Four domains need
 * this (companies, recipes, ingredients, allergens) — each gets its
 * own helper to preserve the exact SQL-path message, since the Rust
 * side's message text differs per table.
 */
async function companyConflictAware<T>(
	path: string,
	caller: InternalCaller,
	opts?: InternalFetchOptions
): Promise<T> {
	try {
		return await internalJson<T>(path, caller, opts);
	} catch (err) {
		if (isInternalApiError(err) && err.status === 409) {
			throw new ConflictError('A company with this name already exists');
		}
		throw err;
	}
}

async function recipeConflictAware<T>(
	path: string,
	caller: InternalCaller,
	opts?: InternalFetchOptions
): Promise<T> {
	try {
		return await internalJson<T>(path, caller, opts);
	} catch (err) {
		if (isInternalApiError(err) && err.status === 409) {
			throw new ConflictError('A recipe with this name already exists');
		}
		throw err;
	}
}

async function ingredientConflictAware<T>(
	path: string,
	caller: InternalCaller,
	opts?: InternalFetchOptions
): Promise<T> {
	try {
		return await internalJson<T>(path, caller, opts);
	} catch (err) {
		if (isInternalApiError(err) && err.status === 409) {
			throw new ConflictError('An ingredient with this name already exists');
		}
		throw err;
	}
}

async function allergenConflictAware<T>(
	path: string,
	caller: InternalCaller,
	opts?: InternalFetchOptions
): Promise<T> {
	try {
		return await internalJson<T>(path, caller, opts);
	} catch (err) {
		if (isInternalApiError(err) && err.status === 409) {
			throw new ConflictError('An allergen with this name already exists');
		}
		throw err;
	}
}

// -------------------------------------------------------------------
// Domain wrappers — one per route-reachable function. The pattern
// mirrors the deleted SQL files' Rust-dispatch branches exactly, with
// `db` dropped and `caller` non-optional.
// -------------------------------------------------------------------

// -----
// invites — admin-only; the Rust side 403s client callers.
// -----

/** GET /api/invites/by-token?token=… — 404 → null to preserve TS semantics. */
export async function getInviteByToken(
	token: string,
	caller: InternalCaller
): Promise<Invite | null> {
	const qs = filtersToQueryString({ token });
	return internalJsonOrNull<Invite>(`/api/invites/by-token${qs}`, caller);
}

/** GET /api/invites?company_id=… — ordered `created_at DESC` on the server. */
export async function getInvitesByCompany(
	companyId: string,
	caller: InternalCaller
): Promise<Invite[]> {
	const qs = filtersToQueryString({ company_id: companyId });
	return internalJson<Invite[]>(`/api/invites${qs}`, caller);
}

/** POST /api/invites — Rust handler lowercases + trims the email. */
export async function createInvite(
	data: InsertInvite,
	caller: InternalCaller
): Promise<Invite> {
	return internalJson<Invite>('/api/invites', caller, {
		method: 'POST',
		body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>))
	});
}

/**
 * POST /api/invites/:id/mark-used. The SQL path was silent on missing
 * ids (no row-count check); Rust returns 404. Swallow 404 to preserve
 * the void-no-op contract.
 */
export async function markInviteUsed(id: string, caller: InternalCaller): Promise<void> {
	try {
		await internalJson<void>(`/api/invites/${id}/mark-used`, caller, { method: 'POST' });
	} catch (err) {
		if (isInternalApiError(err) && err.status === 404) return;
		throw err;
	}
}

/** POST /api/invites/:id/revoke — same semantics as mark-used (SQL identical). */
export async function revokeInvite(id: string, caller: InternalCaller): Promise<void> {
	try {
		await internalJson<void>(`/api/invites/${id}/revoke`, caller, { method: 'POST' });
	} catch (err) {
		if (isInternalApiError(err) && err.status === 404) return;
		throw err;
	}
}

// -----
// invites — public (HMAC-gated, no admin check). Powers the
// invite-landing flow at `routes/invite/[token]/+page.server.ts`,
// where the user is anonymous or a newly-authenticated client. The
// token's 128-bit entropy is the authorization boundary.
// -----

/**
 * GET /api/invites/by-token/public?token=… — same shape as the
 * admin-only `getInviteByToken` but accepts any authenticated internal
 * caller (admin OR client). 404 → null preserves the TS-era contract.
 */
export async function getInviteByTokenPublic(
	token: string,
	caller: InternalCaller
): Promise<Invite | null> {
	const qs = filtersToQueryString({ token });
	return internalJsonOrNull<Invite>(`/api/invites/by-token/public${qs}`, caller);
}

/**
 * POST /api/invites/consume-by-token — atomic token-scoped consume.
 * Verifies unused + unexpired, stamps `used_at`, returns `{id,
 * company_id}` in one SQL statement so a client can't race its own
 * request. Returns `null` on 404 — unknown token, already used, OR
 * expired, deliberately indistinguishable (not an existence oracle).
 */
export async function consumeInviteByToken(
	token: string,
	caller: InternalCaller
): Promise<{ id: string; company_id: string } | null> {
	return internalJsonOrNull<{ id: string; company_id: string }>(
		'/api/invites/consume-by-token',
		caller,
		{
			method: 'POST',
			body: JSON.stringify({ token })
		}
	);
}

// -----
// allergens — admin-only; the Rust side 403s client callers.
// -----

/** GET /api/allergens — ordered `name ASC` on the server. */
export async function getAllergens(caller: InternalCaller): Promise<Allergen[]> {
	return internalJson<Allergen[]>('/api/allergens', caller);
}

/** GET /api/allergens/:id — 404 → null. */
export async function getAllergenById(
	id: string,
	caller: InternalCaller
): Promise<Allergen | null> {
	return internalJsonOrNull<Allergen>(`/api/allergens/${id}`, caller);
}

/** GET /api/allergens/by-name?name=… — case-insensitive, 404 → null. */
export async function getAllergenByName(
	name: string,
	caller: InternalCaller
): Promise<Allergen | null> {
	const qs = filtersToQueryString({ name });
	return internalJsonOrNull<Allergen>(`/api/allergens/by-name${qs}`, caller);
}

/** POST /api/allergens — 409 → ConflictError with the SQL-path message. */
export async function createAllergen(
	data: InsertAllergen,
	caller: InternalCaller
): Promise<Allergen> {
	return allergenConflictAware<Allergen>('/api/allergens', caller, {
		method: 'POST',
		body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>))
	});
}

/**
 * PATCH /api/allergens/:id — 404 → null, 409 → ConflictError. The SQL
 * path returned `null` on missed id; preserve that contract.
 */
export async function updateAllergen(
	id: string,
	data: InsertAllergen,
	caller: InternalCaller
): Promise<Allergen | null> {
	try {
		return await internalJson<Allergen>(`/api/allergens/${id}`, caller, {
			method: 'PATCH',
			body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>))
		});
	} catch (err) {
		if (isInternalApiError(err) && err.status === 404) return null;
		if (isInternalApiError(err) && err.status === 409) {
			throw new ConflictError('An allergen with this name already exists');
		}
		throw err;
	}
}

/**
 * GET /api/allergens/:id/ref-count — returns `{ ingredients, persons }`
 * as JSON numbers. Rust emits `i64`, serde writes a bare JSON number,
 * so decoding straight to `number` is lossless for anything short of
 * 2^53 references (unreachable).
 */
export async function getAllergenRefCount(
	id: string,
	caller: InternalCaller
): Promise<{ ingredients: number; persons: number }> {
	return internalJson<{ ingredients: number; persons: number }>(
		`/api/allergens/${id}/ref-count`,
		caller
	);
}

/**
 * DELETE /api/allergens/:id — 204 on success, 404 when nothing matched.
 * SQL path returned `boolean`; preserve by mapping 404 → false.
 */
export async function deleteAllergen(id: string, caller: InternalCaller): Promise<boolean> {
	try {
		await internalJson<void>(`/api/allergens/${id}`, caller, { method: 'DELETE' });
		return true;
	} catch (err) {
		if (isInternalApiError(err) && err.status === 404) return false;
		throw err;
	}
}

// -----
// companies — admin surface + client-can-read-own-company. The Rust
// handlers enforce the client scoping (404 on cross-company read);
// the dispatcher only forwards.
// -----

/** GET /api/companies — ordered `name ASC` on the server. */
export async function getCompanies(
	opts: { includeInactive?: boolean } = {},
	caller: InternalCaller
): Promise<Company[]> {
	const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
	return internalJson<Company[]>(`/api/companies${qs}`, caller);
}

/** GET /api/companies/paginated — admin-only paginated list. */
export async function getCompaniesPaginated(
	opts: { includeInactive?: boolean; search?: string },
	pagination: PaginationParams,
	caller: InternalCaller
): Promise<PaginatedResult<Company>> {
	const qs = filtersToQueryString({
		include_inactive: opts.includeInactive,
		search: opts.search,
		page: pagination.page,
		per_page: pagination.perPage,
	});
	return internalJson<PaginatedResult<Company>>(`/api/companies/paginated${qs}`, caller);
}

/** GET /api/companies/:id — 404 → null (preserves TS semantics). */
export async function getCompanyById(
	id: string,
	caller: InternalCaller
): Promise<Company | null> {
	return internalJsonOrNull<Company>(`/api/companies/${id}`, caller);
}

/** GET /api/companies/by-name?name=… — case-insensitive, 404 → null. */
export async function getCompanyByName(
	name: string,
	caller: InternalCaller
): Promise<Company | null> {
	const qs = filtersToQueryString({ name });
	return internalJsonOrNull<Company>(`/api/companies/by-name${qs}`, caller);
}

/** GET /api/companies/:id/users — users ordered `email ASC`, company embedded. */
export async function getUsersByCompany(
	companyId: string,
	caller: InternalCaller
): Promise<UserWithCompany[]> {
	return internalJson<UserWithCompany[]>(`/api/companies/${companyId}/users`, caller);
}

/** POST /api/companies — 409 → ConflictError with the SQL-path message. */
export async function createCompany(
	data: InsertCompany,
	caller: InternalCaller
): Promise<Company> {
	return companyConflictAware<Company>('/api/companies', caller, {
		method: 'POST',
		body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>))
	});
}

/**
 * PATCH /api/companies/:id — 404 → null, 409 → ConflictError. The SQL
 * path returned `null` on missed id; preserve that contract.
 */
export async function updateCompany(
	id: string,
	data: UpdateCompany,
	caller: InternalCaller
): Promise<Company | null> {
	try {
		return await internalJson<Company>(`/api/companies/${id}`, caller, {
			method: 'PATCH',
			body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>))
		});
	} catch (err) {
		if (isInternalApiError(err) && err.status === 404) return null;
		if (isInternalApiError(err) && err.status === 409) {
			throw new ConflictError('A company with this name already exists');
		}
		throw err;
	}
}

/**
 * POST /api/companies/:id/deactivate — soft delete. The TS path flipped
 * `active = false` via `updateCompany`; the Rust service exposes a
 * dedicated endpoint. 404 → null preserves the SQL-path contract.
 */
export async function deactivateCompany(
	id: string,
	caller: InternalCaller
): Promise<Company | null> {
	return internalJsonOrNull<Company>(`/api/companies/${id}/deactivate`, caller, {
		method: 'POST',
	});
}

// -----
// persons + attendance — per-company data; the Rust side scopes
// client callers to their own company (404 on cross-company reads,
// same "lie about existence" rule as orders/companies).
//
// Wire-shape note: `Attendance.serving_date` is a `Date` in the TS
// surface (pg driver maps `DATE` → `Date`), but Rust emits ISO
// `YYYY-MM-DD`. The SQL path tests assert `instanceof Date` on the
// upsert return value, so we rehydrate wire strings back to `Date`
// before returning.
// -----

/** Narrow a wire string (`YYYY-MM-DD`) back to a `Date`. */
function isoToDate(iso: unknown): Date {
	if (iso instanceof Date) return iso;
	// `new Date("YYYY-MM-DD")` parses as UTC midnight — exactly what the
	// pg driver produces for a `DATE` column.
	return new Date(iso as string);
}

/** Rehydrate `serving_date` on a single attendance payload. */
function hydrateAttendance<T extends { serving_date: unknown }>(row: T): T {
	return { ...row, serving_date: isoToDate(row.serving_date) };
}

/** GET /api/persons?company_id=…&include_inactive=… — ordered `name ASC`. */
export async function getPersonsByCompany(
	companyId: string,
	opts: { includeInactive?: boolean } = {},
	caller: InternalCaller
): Promise<Person[]> {
	const qs = filtersToQueryString({
		company_id: companyId,
		include_inactive: opts.includeInactive,
	});
	return internalJson<Person[]>(`/api/persons${qs}`, caller);
}

/** GET /api/persons/:id — 404 → null. */
export async function getPersonById(
	id: string,
	opts: { includeInactive?: boolean } = {},
	caller: InternalCaller
): Promise<Person | null> {
	const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
	return internalJsonOrNull<Person>(`/api/persons/${id}${qs}`, caller);
}

/** GET /api/persons/:id/with-allergens — 404 → null. */
export async function getPersonWithAllergens(
	id: string,
	opts: { includeInactive?: boolean } = {},
	caller: InternalCaller
): Promise<PersonWithAllergens | null> {
	const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
	return internalJsonOrNull<PersonWithAllergens>(
		`/api/persons/${id}/with-allergens${qs}`,
		caller
	);
}

/** GET /api/persons/with-allergens?company_id=… — list with embedded allergens. */
export async function getPersonsWithAllergensByCompany(
	companyId: string,
	opts: { includeInactive?: boolean } = {},
	caller: InternalCaller
): Promise<PersonWithAllergens[]> {
	const qs = filtersToQueryString({
		company_id: companyId,
		include_inactive: opts.includeInactive,
	});
	return internalJson<PersonWithAllergens[]>(`/api/persons/with-allergens${qs}`, caller);
}

/** GET /api/persons/:id/allergens — ordered `name ASC`. */
export async function getPersonAllergens(
	personId: string,
	caller: InternalCaller
): Promise<Allergen[]> {
	return internalJson<Allergen[]>(`/api/persons/${personId}/allergens`, caller);
}

/** POST /api/persons. */
export async function createPerson(
	data: InsertPerson,
	caller: InternalCaller
): Promise<Person> {
	return internalJson<Person>('/api/persons', caller, {
		method: 'POST',
		body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>))
	});
}

/** PATCH /api/persons/:id — 404 → null. */
export async function updatePerson(
	id: string,
	data: UpdatePerson,
	caller: InternalCaller
): Promise<Person | null> {
	return internalJsonOrNull<Person>(`/api/persons/${id}`, caller, {
		method: 'PATCH',
		body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>))
	});
}

/** POST /api/persons/:id/deactivate — soft delete; 404 → null. */
export async function deactivatePerson(
	id: string,
	caller: InternalCaller
): Promise<Person | null> {
	return internalJsonOrNull<Person>(`/api/persons/${id}/deactivate`, caller, {
		method: 'POST',
	});
}

/** POST /api/persons/:id/allergens/:allergen_id — idempotent via ON CONFLICT. */
export async function addPersonAllergen(
	personId: string,
	allergenId: string,
	caller: InternalCaller
): Promise<void> {
	await internalJson<void>(
		`/api/persons/${personId}/allergens/${allergenId}`,
		caller,
		{ method: 'POST' }
	);
}

/** DELETE /api/persons/:id/allergens/:allergen_id — idempotent. */
export async function removePersonAllergen(
	personId: string,
	allergenId: string,
	caller: InternalCaller
): Promise<void> {
	await internalJson<void>(
		`/api/persons/${personId}/allergens/${allergenId}`,
		caller,
		{ method: 'DELETE' }
	);
}

/** PUT /api/persons/:id/allergens — replace full set. Transactional on server. */
export async function setPersonAllergens(
	personId: string,
	allergenIds: string[],
	caller: InternalCaller
): Promise<void> {
	await internalJson<void>(`/api/persons/${personId}/allergens`, caller, {
		method: 'PUT',
		body: JSON.stringify({ allergen_ids: allergenIds }),
	});
}

/**
 * GET /api/attendance?company_id=…&serving_date=YYYY-MM-DD — rows with
 * person embedded. Rehydrates `serving_date` back to `Date` to match
 * the pg driver's shape (tests assert `instanceof Date`).
 */
export async function getAttendanceByDate(
	companyId: string,
	servingDate: Date,
	caller: InternalCaller
): Promise<AttendanceWithPerson[]> {
	const qs = filtersToQueryString({
		company_id: companyId,
		serving_date: servingDate,
	});
	const rows = await internalJson<AttendanceWithPerson[]>(`/api/attendance${qs}`, caller);
	return rows.map(hydrateAttendance);
}

/** POST /api/attendance — single-row upsert. Rehydrates `serving_date`. */
export async function upsertAttendance(
	data: UpsertAttendance,
	caller: InternalCaller
): Promise<Attendance> {
	const row = await internalJson<Attendance>('/api/attendance', caller, {
		method: 'POST',
		body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>))
	});
	return hydrateAttendance(row);
}

/**
 * POST /api/attendance/batch — bulk upsert. Rust returns 204 for both
 * empty and populated batches; empty-batch no-op is handled server-side.
 */
export async function upsertAttendanceBatch(
	records: UpsertAttendance[],
	caller: InternalCaller
): Promise<void> {
	await internalJson<void>('/api/attendance/batch', caller, {
		method: 'POST',
		body: JSON.stringify({
			records: records.map((r) =>
				normalizeBody(r as unknown as Record<string, unknown>)
			),
		}),
	});
}

// -----
// menus — GLOBAL master data (no company scoping). Admin writes, both
// roles read. The Rust service 403s client callers on any mutation.
//
// Wire-shape note: `start_date` / `end_date` on `Menu` are `Date | null`
// in the TS surface (pg `DATE` → `Date`), but Rust emits ISO strings or
// `null`. Rehydrate on every return so route code that calls
// `new Date(m.start_date!) <= d` keeps working.
// -----

/** Rehydrate `start_date` / `end_date` on a single menu row. */
function hydrateMenu<T extends { start_date: unknown; end_date: unknown }>(row: T): T {
	return {
		...row,
		start_date: row.start_date == null ? null : isoToDate(row.start_date),
		end_date: row.end_date == null ? null : isoToDate(row.end_date),
	};
}

/** Rehydrate dates on a menu-with-recipes payload (menu-level only; recipe slots carry no dates). */
function hydrateMenuWithRecipes(m: MenuWithRecipes): MenuWithRecipes {
	return hydrateMenu(m);
}

/** GET /api/menus — ordered `name ASC` on the server. */
export async function getMenus(
	opts: { includeInactive?: boolean } = {},
	caller: InternalCaller
): Promise<Menu[]> {
	const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
	const rows = await internalJson<Menu[]>(`/api/menus${qs}`, caller);
	return rows.map(hydrateMenu);
}

/** GET /api/menus/:id — 404 → null. */
export async function getMenuById(
	id: string,
	opts: { includeInactive?: boolean } = {},
	caller: InternalCaller
): Promise<Menu | null> {
	const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
	const row = await internalJsonOrNull<Menu>(`/api/menus/${id}${qs}`, caller);
	return row == null ? null : hydrateMenu(row);
}

/** GET /api/menus/:id/with-recipes — 404 → null. */
export async function getMenuWithRecipes(
	id: string,
	caller: InternalCaller
): Promise<MenuWithRecipes | null> {
	const row = await internalJsonOrNull<MenuWithRecipes>(
		`/api/menus/${id}/with-recipes`,
		caller
	);
	return row == null ? null : hydrateMenuWithRecipes(row);
}

/** GET /api/menus/with-recipes — list of all active menus with embedded recipe slots. */
export async function getMenusWithRecipes(
	opts: { includeInactive?: boolean } = {},
	caller: InternalCaller
): Promise<MenuWithRecipes[]> {
	const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
	const rows = await internalJson<MenuWithRecipes[]>(`/api/menus/with-recipes${qs}`, caller);
	return rows.map(hydrateMenuWithRecipes);
}

/** GET /api/menus/:id/recipes — slot list w/ embedded `recipe_name`. */
export async function getMenuRecipes(
	menuId: string,
	caller: InternalCaller
): Promise<MenuRecipeWithName[]> {
	return internalJson<MenuRecipeWithName[]>(`/api/menus/${menuId}/recipes`, caller);
}

/**
 * GET /api/menus/active-for-date?date=YYYY-MM-DD — the active menu
 * covering a given calendar date, or `null`. Mirrors the SQL path's
 * "most-recent start_date wins" tiebreaker on the server.
 */
export async function getActiveMenuForDate(
	date: Date,
	caller: InternalCaller
): Promise<Menu | null> {
	const qs = filtersToQueryString({ date });
	const row = await internalJsonOrNull<Menu>(`/api/menus/active-for-date${qs}`, caller);
	return row == null ? null : hydrateMenu(row);
}

/**
 * GET /api/menus/:id/slot — single slot lookup by composite key. The TS
 * path returned `MenuRecipe | null`; preserve that via `internalJsonOrNull`.
 */
export async function getMenuSlot(
	menuId: string,
	cycleWeek: 1 | 2 | 3 | 4,
	dayOfWeek: DayOfWeek,
	mealCategory: string,
	caller: InternalCaller
): Promise<MenuRecipe | null> {
	const qs = filtersToQueryString({
		cycle_week: cycleWeek,
		day_of_week: dayOfWeek,
		meal_category: mealCategory,
	});
	return internalJsonOrNull<MenuRecipe>(`/api/menus/${menuId}/slot${qs}`, caller);
}

/** POST /api/menus — admin-only; 403 for clients. Rehydrates returned dates. */
export async function createMenu(
	data: InsertMenu,
	caller: InternalCaller
): Promise<Menu> {
	const row = await internalJson<Menu>('/api/menus', caller, {
		method: 'POST',
		body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>))
	});
	return hydrateMenu(row);
}

/** PATCH /api/menus/:id — 404 → null. Rehydrates returned dates. */
export async function updateMenu(
	id: string,
	data: UpdateMenu,
	caller: InternalCaller
): Promise<Menu | null> {
	const row = await internalJsonOrNull<Menu>(`/api/menus/${id}`, caller, {
		method: 'PATCH',
		body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>))
	});
	return row == null ? null : hydrateMenu(row);
}

/** POST /api/menus/:id/deactivate — soft delete; 404 → null. */
export async function deactivateMenu(
	id: string,
	caller: InternalCaller
): Promise<Menu | null> {
	const row = await internalJsonOrNull<Menu>(`/api/menus/${id}/deactivate`, caller, {
		method: 'POST',
	});
	return row == null ? null : hydrateMenu(row);
}

/**
 * PUT /api/menus/:id/slot — upsert on the composite key
 * `(menu_id, cycle_week, day_of_week, meal_category)`. Server handles
 * the ON CONFLICT.
 */
export async function setMenuSlot(
	data: InsertMenuRecipe,
	caller: InternalCaller
): Promise<MenuRecipe> {
	return internalJson<MenuRecipe>(`/api/menus/${data.menu_id}/slot`, caller, {
		method: 'PUT',
		body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>))
	});
}

/**
 * DELETE /api/menus/:id/slot — clears a single slot by composite key.
 * TS path returned void regardless of row-count; Rust 404 is swallowed.
 */
export async function clearMenuSlot(
	menuId: string,
	cycleWeek: 1 | 2 | 3 | 4,
	dayOfWeek: DayOfWeek,
	mealCategory: string,
	caller: InternalCaller
): Promise<void> {
	const qs = filtersToQueryString({
		cycle_week: cycleWeek,
		day_of_week: dayOfWeek,
		meal_category: mealCategory,
	});
	try {
		await internalJson<void>(`/api/menus/${menuId}/slot${qs}`, caller, {
			method: 'DELETE',
		});
	} catch (err) {
		if (isInternalApiError(err) && err.status === 404) return;
		throw err;
	}
}

/**
 * PUT /api/menus/:id/slots — bulk replace of all slots for a menu.
 * Server transactionally wipes + inserts the new set.
 */
export async function setAllMenuSlots(
	menuId: string,
	slots: InsertMenuRecipe[],
	caller: InternalCaller
): Promise<void> {
	await internalJson<void>(`/api/menus/${menuId}/slots`, caller, {
		method: 'PUT',
		body: JSON.stringify({
			slots: slots.map((s) => normalizeBody(s as unknown as Record<string, unknown>)),
		}),
	});
}

// -------------------------------------------------------------------
// INSIGHTS — admin-only analytics
//
// Rust owns these end-to-end: `require_admin` on every handler, 403
// for clients. All wire shapes are strings / numbers / date-strings
// only — no `Date` rehydration needed. `KpiMetrics` serializes with
// camelCase so wire keys land directly on the TS interface.
// -------------------------------------------------------------------

export interface KPIMetrics {
	totalMeals: number;
	totalMealsPriorWeek: number;
	weekOverWeekChange: number | null;
	statusCounts: Record<OrderStatus, number>;
	activeCompanies: number;
}

export interface VolumeSeries {
	date: string;
	total: number;
}

export interface TopRecipe {
	name: string;
	order_count: number;
	total_amount: number;
	percent_of_total: number;
}

export interface CompanyBreakdown {
	name: string;
	total_amount: number;
	order_count: number;
}

export interface SourceMix {
	order_source: OrderSource;
	total_amount: number;
	order_count: number;
}

export interface IngredientDemand {
	name: string;
	total_amount: number;
	unit: string;
	allergens: string;
}

export interface AttendanceDayRate {
	date: string;
	attending: number;
	total: number;
	rate: number;
}

export interface AttendanceByCompany {
	company_name: string;
	attending: number;
	total: number;
	rate: number;
}

export async function getKPIMetrics(
	from: string,
	to: string,
	priorFrom: string,
	priorTo: string,
	caller: InternalCaller,
): Promise<KPIMetrics> {
	const qs = filtersToQueryString({ from, to, prior_from: priorFrom, prior_to: priorTo });
	return internalJson<KPIMetrics>(`/api/insights/kpi${qs}`, caller);
}

export async function getOrderVolumeSeries(
	from: string,
	to: string,
	granularity: 'daily' | 'weekly',
	caller: InternalCaller,
): Promise<VolumeSeries[]> {
	const qs = filtersToQueryString({ from, to, granularity });
	return internalJson<VolumeSeries[]>(`/api/insights/volume${qs}`, caller);
}

export async function getTopRecipes(
	from: string,
	to: string,
	limit: number,
	caller: InternalCaller,
): Promise<TopRecipe[]> {
	const qs = filtersToQueryString({ from, to, limit });
	return internalJson<TopRecipe[]>(`/api/insights/top-recipes${qs}`, caller);
}

export async function getCompanyBreakdown(
	from: string,
	to: string,
	caller: InternalCaller,
): Promise<CompanyBreakdown[]> {
	const qs = filtersToQueryString({ from, to });
	return internalJson<CompanyBreakdown[]>(`/api/insights/companies${qs}`, caller);
}

export async function getSourceMix(
	from: string,
	to: string,
	caller: InternalCaller,
): Promise<SourceMix[]> {
	const qs = filtersToQueryString({ from, to });
	return internalJson<SourceMix[]>(`/api/insights/source-mix${qs}`, caller);
}

export async function getIngredientDemand(
	from: string,
	to: string,
	caller: InternalCaller,
): Promise<IngredientDemand[]> {
	const qs = filtersToQueryString({ from, to });
	return internalJson<IngredientDemand[]>(`/api/insights/ingredients${qs}`, caller);
}

export async function getAttendanceRate(
	from: string,
	to: string,
	caller: InternalCaller,
): Promise<AttendanceDayRate[]> {
	const qs = filtersToQueryString({ from, to });
	return internalJson<AttendanceDayRate[]>(`/api/insights/attendance${qs}`, caller);
}

export async function getAttendanceByCompany(
	from: string,
	to: string,
	caller: InternalCaller,
): Promise<AttendanceByCompany[]> {
	const qs = filtersToQueryString({ from, to });
	return internalJson<AttendanceByCompany[]>(`/api/insights/attendance-by-company${qs}`, caller);
}

// -------------------------------------------------------------------
// FINANCIALS — admin-only recipe margins + invoice summaries.
//
// Every Rust handler hard-rejects client callers with 403. No scoping
// logic needed here. Wire shapes:
//  - `RecipeMargin` matches Rust `RecipeMarginRow` field-for-field;
//    numeric fields serialize as plain JSON numbers on both sides.
//  - `PaginatedResult<RecipeMargin>` uses `#[serde(rename_all =
//    "camelCase")]` on the Rust side → `{data,total,page,perPage,totalPages}`.
//  - `InvoiceSummary.line_items[].serving_date` arrives as a
//    `YYYY-MM-DD` string from Rust; the only consumer wraps every
//    value in `new Date(...)` which accepts both.
//  - `getInvoiceData` preserves 404-→ null via `internalJsonOrNull`.
// -------------------------------------------------------------------

export interface RecipeMargin {
	id: string;
	name: string;
	price_per_meal: number | null;
	ingredient_cost: number | null;
	margin: number | null;
	margin_pct: number | null;
}

export interface InvoiceLineItem {
	order_id: string;
	recipe_name: string;
	serving_date: Date | string;
	amount: number;
	unit_price: number;
	line_total: number;
}

export interface InvoiceSummary {
	company_id: string;
	company_name: string;
	shipping_address: string | null;
	line_items: InvoiceLineItem[];
	subtotal: number;
}

export async function getRecipesWithMarginsPaginated(
	pagination: PaginationParams,
	caller: InternalCaller,
): Promise<PaginatedResult<RecipeMargin>> {
	const qs = filtersToQueryString({
		page: pagination.page,
		per_page: pagination.perPage,
	});
	return internalJson<PaginatedResult<RecipeMargin>>(
		`/api/financials/recipe-margins/paginated${qs}`,
		caller,
	);
}

export async function getInvoiceData(
	companyId: string,
	from: string,
	to: string,
	caller: InternalCaller,
): Promise<InvoiceSummary | null> {
	const qs = filtersToQueryString({ company_id: companyId, from, to });
	return internalJsonOrNull<InvoiceSummary>(`/api/financials/invoice${qs}`, caller);
}

// -------------------------------------------------------------------
// INGREDIENTS — admin-only master data
//
// Rust owns these end-to-end: `require_admin` on every handler, 403
// for clients. Wire shapes match TS types field-for-field (snake_case
// everywhere). Conflicts translate `409 → ConflictError('An
// ingredient with this name already exists')` on both create + update.
// 404 → null for id-scoped reads + idempotent soft-delete. The
// `setIngredientAllergens` call previously accepted a `PoolClient` to
// compose with outer transactions; no in-repo caller uses that
// capability, so the dispatcher takes `allergenIds` directly.
// -------------------------------------------------------------------

import type {
	Ingredient,
	InsertIngredient,
	UpdateIngredient,
	IngredientWithAllergens,
} from '$lib/types';

export async function getIngredients(
	opts: { includeInactive?: boolean } = {},
	caller: InternalCaller,
): Promise<Ingredient[]> {
	const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
	return internalJson<Ingredient[]>(`/api/ingredients${qs}`, caller);
}

export async function getIngredientNames(caller: InternalCaller): Promise<string[]> {
	return internalJson<string[]>('/api/ingredients/names', caller);
}

export async function getIngredientById(
	id: string,
	opts: { includeInactive?: boolean } = {},
	caller: InternalCaller,
): Promise<Ingredient | null> {
	const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
	return internalJsonOrNull<Ingredient>(`/api/ingredients/${id}${qs}`, caller);
}

export async function getIngredientWithAllergens(
	id: string,
	opts: { includeInactive?: boolean } = {},
	caller: InternalCaller,
): Promise<IngredientWithAllergens | null> {
	const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
	return internalJsonOrNull<IngredientWithAllergens>(
		`/api/ingredients/${id}/with-allergens${qs}`,
		caller,
	);
}

export async function getIngredientsWithAllergens(
	opts: { includeInactive?: boolean } = {},
	caller: InternalCaller,
): Promise<IngredientWithAllergens[]> {
	const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
	return internalJson<IngredientWithAllergens[]>(`/api/ingredients/with-allergens${qs}`, caller);
}

export async function getIngredientsWithAllergensPaginated(
	opts: { includeInactive?: boolean; search?: string },
	pagination: PaginationParams,
	caller: InternalCaller,
): Promise<PaginatedResult<IngredientWithAllergens>> {
	const qs = filtersToQueryString({
		include_inactive: opts.includeInactive,
		search: opts.search,
		page: pagination.page,
		per_page: pagination.perPage,
	});
	return internalJson<PaginatedResult<IngredientWithAllergens>>(
		`/api/ingredients/paginated${qs}`,
		caller,
	);
}

export async function createIngredient(
	data: InsertIngredient,
	caller: InternalCaller,
): Promise<Ingredient> {
	return ingredientConflictAware<Ingredient>('/api/ingredients', caller, {
		method: 'POST',
		body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>)),
	});
}

export async function updateIngredient(
	id: string,
	data: UpdateIngredient,
	caller: InternalCaller,
): Promise<Ingredient | null> {
	// Preserve three-state cost_per_unit: present+value → set, present+null →
	// clear, absent → leave alone. `normalizeBody` strips `undefined` but lets
	// `null` through, which is exactly the semantic we want on the wire.
	const body: Record<string, unknown> = { ...(data as Record<string, unknown>) };
	if (!('cost_per_unit' in data)) delete body.cost_per_unit;

	try {
		return await internalJson<Ingredient>(`/api/ingredients/${id}`, caller, {
			method: 'PATCH',
			body: JSON.stringify(normalizeBody(body)),
		});
	} catch (err) {
		if (isInternalApiError(err) && err.status === 404) return null;
		if (isInternalApiError(err) && err.status === 409) {
			throw new ConflictError('An ingredient with this name already exists');
		}
		throw err;
	}
}

export async function deactivateIngredient(
	id: string,
	caller: InternalCaller,
): Promise<Ingredient | null> {
	return internalJsonOrNull<Ingredient>(`/api/ingredients/${id}/deactivate`, caller, {
		method: 'POST',
	});
}

export async function addIngredientAllergen(
	ingredientId: string,
	allergenId: string,
	caller: InternalCaller,
): Promise<void> {
	await internalJson<void>(
		`/api/ingredients/${ingredientId}/allergens/${allergenId}`,
		caller,
		{ method: 'POST' },
	);
}

export async function removeIngredientAllergen(
	ingredientId: string,
	allergenId: string,
	caller: InternalCaller,
): Promise<void> {
	await internalJson<void>(
		`/api/ingredients/${ingredientId}/allergens/${allergenId}`,
		caller,
		{ method: 'DELETE' },
	);
}

export async function setIngredientAllergens(
	ingredientId: string,
	allergenIds: string[],
	caller: InternalCaller,
): Promise<void> {
	await internalJson<void>(
		`/api/ingredients/${ingredientId}/allergens`,
		caller,
		{
			method: 'PUT',
			body: JSON.stringify({ allergen_ids: allergenIds }),
		},
	);
}

// -------------------------------------------------------------------
// RECIPES — global master data (no company scoping).
//
// Rust owns these end-to-end: `require_admin` on every mutation, 403
// for clients. Reads are open to all authenticated callers. Wire
// shapes match TS types field-for-field. Conflicts translate
// `409 → ConflictError('A recipe with this name already exists')`
// on both create + update; 404 → null on id-scoped reads +
// idempotent soft-delete. `getRecipesWithIngredientsByIds` returns
// a Map keyed by id to preserve the SQL surface (Rust emits a flat
// Vec, we rebuild the Map client-side). The `addRecipeIngredient`
// / `removeRecipeIngredient` / `setRecipeIngredients` wrappers
// previously accepted a `PoolClient` to compose with outer
// transactions; no in-repo caller uses that capability, so the
// dispatcher drops it.
// -------------------------------------------------------------------

import type {
	Recipe,
	InsertRecipe,
	UpdateRecipe,
	RecipeWithIngredients,
	RecipeIngredient,
} from '$lib/types';

export async function getRecipes(
	opts: { includeInactive?: boolean } = {},
	caller: InternalCaller,
): Promise<Recipe[]> {
	const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
	return internalJson<Recipe[]>(`/api/recipes${qs}`, caller);
}

export async function getRecipeById(
	id: string,
	opts: { includeInactive?: boolean } = {},
	caller: InternalCaller,
): Promise<Recipe | null> {
	const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
	return internalJsonOrNull<Recipe>(`/api/recipes/${id}${qs}`, caller);
}

export async function getRecipeByName(
	name: string,
	opts: { includeInactive?: boolean } = {},
	caller: InternalCaller,
): Promise<Recipe | null> {
	const qs = filtersToQueryString({ name, include_inactive: opts.includeInactive });
	return internalJsonOrNull<Recipe>(`/api/recipes/by-name${qs}`, caller);
}

export async function getRecipeWithIngredients(
	id: string,
	opts: { includeInactive?: boolean } = {},
	caller: InternalCaller,
): Promise<RecipeWithIngredients | null> {
	const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
	return internalJsonOrNull<RecipeWithIngredients>(
		`/api/recipes/${id}/with-ingredients${qs}`,
		caller,
	);
}

export async function getRecipesWithIngredients(
	opts: { includeInactive?: boolean } = {},
	caller: InternalCaller,
): Promise<RecipeWithIngredients[]> {
	const qs = filtersToQueryString({ include_inactive: opts.includeInactive });
	return internalJson<RecipeWithIngredients[]>(`/api/recipes/with-ingredients${qs}`, caller);
}

export async function getRecipesWithIngredientsByIds(
	ids: string[],
	caller: InternalCaller,
): Promise<Map<string, RecipeWithIngredients>> {
	if (ids.length === 0) return new Map();
	const qs = filtersToQueryString({ ids: ids.join(',') });
	const rows = await internalJson<RecipeWithIngredients[]>(`/api/recipes/by-ids${qs}`, caller);
	const map = new Map<string, RecipeWithIngredients>();
	for (const r of rows) map.set(r.id, r);
	return map;
}

export async function getRecipesWithIngredientsPaginated(
	opts: { includeInactive?: boolean; search?: string },
	pagination: PaginationParams,
	caller: InternalCaller,
): Promise<PaginatedResult<RecipeWithIngredients>> {
	const qs = filtersToQueryString({
		include_inactive: opts.includeInactive,
		search: opts.search,
		page: pagination.page,
		per_page: pagination.perPage,
	});
	return internalJson<PaginatedResult<RecipeWithIngredients>>(
		`/api/recipes/paginated${qs}`,
		caller,
	);
}

export async function getRecipeIngredients(
	recipeId: string,
	caller: InternalCaller,
): Promise<RecipeIngredient[]> {
	return internalJson<RecipeIngredient[]>(`/api/recipes/${recipeId}/ingredients`, caller);
}

export async function createRecipe(
	data: InsertRecipe,
	caller: InternalCaller,
): Promise<Recipe> {
	return recipeConflictAware<Recipe>('/api/recipes', caller, {
		method: 'POST',
		body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>)),
	});
}

export async function updateRecipe(
	id: string,
	data: UpdateRecipe,
	caller: InternalCaller,
): Promise<Recipe | null> {
	// Three-state price_per_meal: present+value → set, present+null →
	// clear, absent → leave alone. `normalizeBody` strips `undefined`
	// but passes `null`, which is the semantic we want on the wire.
	const body: Record<string, unknown> = { ...(data as Record<string, unknown>) };
	if (!('price_per_meal' in data)) delete body.price_per_meal;

	try {
		return await internalJson<Recipe>(`/api/recipes/${id}`, caller, {
			method: 'PATCH',
			body: JSON.stringify(normalizeBody(body)),
		});
	} catch (err) {
		if (isInternalApiError(err) && err.status === 404) return null;
		if (isInternalApiError(err) && err.status === 409) {
			throw new ConflictError('A recipe with this name already exists');
		}
		throw err;
	}
}

export async function deactivateRecipe(
	id: string,
	caller: InternalCaller,
): Promise<Recipe | null> {
	return internalJsonOrNull<Recipe>(`/api/recipes/${id}/deactivate`, caller, {
		method: 'POST',
	});
}

export async function addRecipeIngredient(
	recipeId: string,
	ingredientId: string,
	amount: number,
	unit: string,
	caller: InternalCaller,
): Promise<void> {
	await internalJson<void>(
		`/api/recipes/${recipeId}/ingredients/${ingredientId}`,
		caller,
		{ method: 'POST', body: JSON.stringify({ amount, unit }) },
	);
}

export async function removeRecipeIngredient(
	recipeId: string,
	ingredientId: string,
	caller: InternalCaller,
): Promise<void> {
	await internalJson<void>(
		`/api/recipes/${recipeId}/ingredients/${ingredientId}`,
		caller,
		{ method: 'DELETE' },
	);
}

export async function setRecipeIngredients(
	recipeId: string,
	ingredient: Array<{ ingredient_id: string; amount: number; unit: string }>,
	caller: InternalCaller,
): Promise<void> {
	await internalJson<void>(`/api/recipes/${recipeId}/ingredients`, caller, {
		method: 'PUT',
		body: JSON.stringify({ ingredient }),
	});
}

export async function isRecipeActive(
	id: string,
	caller: InternalCaller,
): Promise<boolean> {
	const r = await internalJson<{ active: boolean }>(`/api/recipes/${id}/active`, caller);
	return r.active;
}

// -----
// orders — admin + client surface. The Rust handlers enforce the
// client scoping (404 on cross-company read for clients; admin
// unrestricted). Wire shapes for `OrderSummary` / `OrderDetail`
// match the Rust `OrderSummaryRow` / `OrderDetailRow` field for
// field except that date columns are ISO strings over JSON — we
// rehydrate `serving_date` / `shipment_date` / `created_at` back to
// `Date` so routes that already do `instanceof Date` or `.getTime()`
// against the TS-path contract keep working.
// -----

export interface OrderFilters {
	company_id?: string;
	recipe_id?: string;
	status?: OrderStatus;
	order_source?: OrderSource;
	serving_date_from?: string | Date;
	serving_date_to?: string | Date;
	shipment_date?: string | Date;
	frozen?: boolean;
	chilled?: boolean;
}

function hydrateOrder<T extends { serving_date: unknown; shipment_date: unknown; created_at: unknown }>(
	row: T,
): T {
	return {
		...row,
		serving_date: isoToDate(row.serving_date),
		shipment_date: isoToDate(row.shipment_date),
		created_at: isoToDate(row.created_at),
	};
}

function hydrateOrderDetail(row: OrderDetail): OrderDetail {
	const base = hydrateOrder(row);
	return {
		...base,
		company: {
			...row.company,
			created_at: isoToDate(row.company.created_at),
		},
		recipe: row.recipe
			? { ...row.recipe, created_at: isoToDate(row.recipe.created_at) }
			: null,
	};
}

/** GET /api/orders — ordered server-side; filters serialize as query params. */
export async function getOrders(
	filters: OrderFilters = {},
	caller: InternalCaller,
): Promise<OrderSummary[]> {
	const qs = filtersToQueryString(filters as Record<string, unknown>);
	const rows = await internalJson<OrderSummary[]>(`/api/orders${qs}`, caller);
	return rows.map((r) => hydrateOrder(r));
}

/** GET /api/orders/paginated — same filter shape plus `page` + `per_page`. */
export async function getOrdersPaginated(
	filters: OrderFilters = {},
	pagination: PaginationParams,
	caller: InternalCaller,
): Promise<PaginatedResult<OrderSummary>> {
	const qs = filtersToQueryString({
		...(filters as Record<string, unknown>),
		page: pagination.page,
		per_page: pagination.perPage,
	});
	const page = await internalJson<PaginatedResult<OrderSummary>>(
		`/api/orders/paginated${qs}`,
		caller,
	);
	return { ...page, data: page.data.map((r) => hydrateOrder(r)) };
}

/** GET /api/orders/:id — 404 → null. */
export async function getOrderById(
	id: string,
	caller: InternalCaller,
): Promise<OrderDetail | null> {
	const row = await internalJsonOrNull<OrderDetail>(`/api/orders/${id}`, caller);
	return row ? hydrateOrderDetail(row) : null;
}

/** Convenience over `getOrders` — orders going out on a specific shipment date. */
export async function getOrdersByShipmentDate(
	shipmentDate: Date,
	caller: InternalCaller,
): Promise<OrderSummary[]> {
	return getOrders({ shipment_date: shipmentDate }, caller);
}

/** GET /api/orders/counts — status→count map, backfill missing statuses with 0. */
export async function getOrderCountsByStatus(
	filters: Pick<OrderFilters, 'serving_date_from' | 'serving_date_to'> = {},
	caller: InternalCaller,
): Promise<Record<OrderStatus, number>> {
	const qs = filtersToQueryString(filters as Record<string, unknown>);
	const raw = await internalJson<Partial<Record<OrderStatus, number>>>(
		`/api/orders/counts${qs}`,
		caller,
	);
	return {
		pending: raw.pending ?? 0,
		confirmed: raw.confirmed ?? 0,
		fulfilled: raw.fulfilled ?? 0,
		cancelled: raw.cancelled ?? 0,
	};
}

/** POST /api/orders — admin + client create. */
export async function createOrder(
	data: InsertOrder,
	caller: InternalCaller,
): Promise<Order> {
	const row = await internalJson<Order>('/api/orders', caller, {
		method: 'POST',
		body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>)),
	});
	return hydrateOrder(row);
}

/** PATCH /api/orders/:id — 404 → null (preserve SQL-path contract). */
export async function updateOrder(
	id: string,
	data: UpdateOrder,
	caller: InternalCaller,
): Promise<Order | null> {
	const row = await internalJsonOrNull<Order>(`/api/orders/${id}`, caller, {
		method: 'PATCH',
		body: JSON.stringify(normalizeBody(data as unknown as Record<string, unknown>)),
	});
	return row ? hydrateOrder(row) : null;
}

/**
 * POST /api/orders/:id/confirm — pending → confirmed. 404 (or non-pending)
 * → null, matching the SQL `WHERE status = 'pending'` row-count semantics.
 */
export async function confirmOrder(
	id: string,
	caller: InternalCaller,
): Promise<Order | null> {
	const row = await internalJsonOrNull<Order>(`/api/orders/${id}/confirm`, caller, {
		method: 'POST',
	});
	return row ? hydrateOrder(row) : null;
}

/** POST /api/orders/:id/fulfill — confirmed → fulfilled; 404 → null. */
export async function fulfillOrder(
	id: string,
	caller: InternalCaller,
): Promise<Order | null> {
	const row = await internalJsonOrNull<Order>(`/api/orders/${id}/fulfill`, caller, {
		method: 'POST',
	});
	return row ? hydrateOrder(row) : null;
}

/** POST /api/orders/:id/cancel — pending|confirmed → cancelled; 404 → null. */
export async function cancelOrder(
	id: string,
	caller: InternalCaller,
): Promise<Order | null> {
	const row = await internalJsonOrNull<Order>(`/api/orders/${id}/cancel`, caller, {
		method: 'POST',
	});
	return row ? hydrateOrder(row) : null;
}

/**
 * POST /api/orders/batch/status — unconditional set over an id list.
 * Short-circuit on empty input to avoid a wasted roundtrip (and match
 * the TS-path early-return exactly).
 */
export async function setOrdersStatusBatch(
	ids: string[],
	status: OrderStatus,
	caller: InternalCaller,
): Promise<Order[]> {
	if (ids.length === 0) return [];
	const rows = await internalJson<Order[]>('/api/orders/batch/status', caller, {
		method: 'POST',
		body: JSON.stringify({ ids, status }),
	});
	return rows.map((r) => hydrateOrder(r));
}

// -------------------------------------------------------------------
// Re-exports — type-only; the runtime symbols `internalFetch` /
// `internalJson` are the lazy proxies declared above.
// -------------------------------------------------------------------

export type { InternalCaller };
