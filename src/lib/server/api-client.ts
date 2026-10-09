// HMAC-signed internal client for the Rust API.
//
// Mirrors `admin-operations-rust/crates/api/src/auth.rs`. Any change
// to the canonical payload, header names, or timestamp window must
// land in both files in the same commit — the Rust side 401s every
// request on mismatch.
//
// Threat model is in `docs/internal-auth.md`.

import { createHmac, randomBytes } from 'node:crypto';
import { env } from '$env/dynamic/private';

/**
 * Shape of the authenticated caller passed to the Rust service.
 * `user.id` is a Better Auth string id (not a UUID); `companyId` is
 * a UUID or null for admins whose scope is global.
 */
export interface InternalCaller {
	userId: string;
	role: 'admin' | 'client';
	companyId: string | null;
}

export interface InternalFetchOptions extends Omit<RequestInit, 'headers'> {
	headers?: Record<string, string>;
}

export class InternalApiError extends Error {
	readonly status: number;
	readonly code: string | null;
	constructor(status: number, code: string | null, message: string) {
		super(message);
		this.name = 'InternalApiError';
		this.status = status;
		this.code = code;
	}
}

/**
 * Build an `InternalCaller` from a SvelteKit request's `locals`. The
 * `role` cast is safe: Better Auth stores either `'admin'` or `'user'`
 * (client), matching the Rust `Role` enum once narrowed.
 *
 * Post-cutover (Track 3 Step 6), every domain wrapper in
 * `src/lib/server/queries/_rust-dispatch.ts` takes a non-optional
 * `InternalCaller` — there is no SQL fallback. Callers that have no
 * `locals.user` should not be invoking domain functions at all; this
 * helper throws loudly so such a slip is caught in dev.
 */
export function callerFromLocals(locals: App.Locals): InternalCaller {
	if (!locals.user) {
		throw new Error(
			'callerFromLocals: locals.user is null — route is not behind an auth hook'
		);
	}
	return {
		userId: locals.user.id,
		role: locals.user.role as 'admin' | 'client',
		companyId: locals.user.companyId ?? null
	};
}

/**
 * `fetch` wrapper that adds the six `X-Internal-*` headers.
 *
 * - `path` is appended to `RUST_API_URL` (no trailing slash required).
 * - `caller` identifies the authenticated user; the Rust side attaches
 *   it to request extensions for handlers to use.
 * - JSON errors (`{error, message}`) are turned into `InternalApiError`;
 *   non-JSON failures surface as plain `Error` with the body text.
 */
export async function internalFetch(
	path: string,
	caller: InternalCaller,
	opts: InternalFetchOptions = {}
): Promise<Response> {
	const base = env.RUST_API_URL;
	if (!base) {
		throw new Error('RUST_API_URL is not set — Rust service unavailable');
	}
	const secret = env.INTERNAL_AUTH_SECRET;
	if (!secret || secret.length < 32) {
		// Fail loud at the call site rather than ship a request the
		// Rust side will reject — easier to diagnose in dev.
		throw new Error('INTERNAL_AUTH_SECRET must be set and >=32 bytes');
	}

	const url = base.replace(/\/$/, '') + (path.startsWith('/') ? path : `/${path}`);

	const ts = Math.floor(Date.now() / 1000).toString();
	// 16 random bytes = 32 hex chars; matches the Rust `nonce.len() >= 32` guard.
	const nonce = randomBytes(16).toString('hex');
	const company = caller.companyId ?? '';
	const payload = `${caller.userId}|${caller.role}|${company}|${ts}|${nonce}`;
	const signature = createHmac('sha256', secret).update(payload).digest('hex');

	const headers: Record<string, string> = {
		...(opts.headers ?? {}),
		'x-internal-user': caller.userId,
		'x-internal-role': caller.role,
		'x-internal-company': company,
		'x-internal-timestamp': ts,
		'x-internal-nonce': nonce,
		'x-internal-auth': signature
	};

	// Only set content-type for bodied requests when the caller hasn't
	// already provided one AND the body isn't a shape that owns its own
	// content-type. FormData generates a boundary that only `fetch` can
	// stamp onto the header; forcing `application/json` here breaks the
	// multipart parse on the Rust side (EPIPE as the server rejects
	// mid-stream). Same reasoning applies to Blob/ReadableStream/
	// URLSearchParams — let undici pick the right type.
	const bodyOwnsContentType =
		opts.body instanceof FormData ||
		opts.body instanceof Blob ||
		opts.body instanceof URLSearchParams ||
		opts.body instanceof ReadableStream;
	if (
		opts.body &&
		!bodyOwnsContentType &&
		!headers['content-type'] &&
		!headers['Content-Type']
	) {
		headers['content-type'] = 'application/json';
	}

	const res = await fetch(url, { ...opts, headers });
	return res;
}

/** Typed JSON wrapper around `internalFetch`. Throws on non-2xx. */
export async function internalJson<T>(
	path: string,
	caller: InternalCaller,
	opts: InternalFetchOptions = {}
): Promise<T> {
	const res = await internalFetch(path, caller, opts);
	if (!res.ok) {
		let code: string | null = null;
		let message = res.statusText;
		const text = await res.text();
		try {
			const body = JSON.parse(text) as { error?: string; message?: string };
			code = body.error ?? null;
			message = body.message ?? message;
		} catch {
			if (text) message = text;
		}
		throw new InternalApiError(res.status, code, message);
	}
	// 204 No Content (and any empty body) → undefined. Several Rust
	// handlers (set_allergens, add_allergen, remove_allergen, batch
	// attendance upsert, …) answer 204 on success; `res.json()` on an
	// empty body throws `SyntaxError: Unexpected end of JSON input`.
	// The TS surface wrappers call these as `internalJson<void>`, so
	// returning `undefined` for void-shaped endpoints is correct.
	if (res.status === 204) return undefined as T;
	const text = await res.text();
	if (text === '') return undefined as T;
	return JSON.parse(text) as T;
}
