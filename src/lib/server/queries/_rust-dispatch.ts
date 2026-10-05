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
	const log = await logger();
	log.warn(obj, msg);
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
// Re-exports — type-only; the runtime symbols `internalFetch` /
// `internalJson` are the lazy proxies declared above.
// -------------------------------------------------------------------

export type { InternalCaller };
