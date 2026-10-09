// Server-side feature-flag guard for SvelteKit routes.
//
// The resolver lives in Rust (`crates/flags` + `/api/flags/check`); this
// file is just a thin ergonomic wrapper so route code reads as a single
// line. There is no client-side flag code — flags are a server concern.
//
// Pattern:
//   const caller = callerFromLocals(locals);
//   await requireFlag('insights', caller);  // throws 404 when disabled
//
// 404 (not 403) is deliberate: a disabled surface should be
// indistinguishable from one that never shipped.

import { error } from '@sveltejs/kit';
import { internalJson, type InternalCaller } from './api-client';

/**
 * Throws `error(404)` when the named flag is disabled for the caller's
 * context. Returns silently on success — callers don't need the value.
 *
 * Precedence (per-company override > global default > compile-time
 * false) is enforced by the Rust resolver; this helper trusts the
 * `/api/flags/check` response verbatim.
 */
export async function requireFlag(name: string, caller: InternalCaller): Promise<void> {
	const res = await internalJson<{ name: string; enabled: boolean }>(
		'/api/flags/check',
		caller,
		{ method: 'POST', body: JSON.stringify({ name }) }
	);
	if (!res.enabled) {
		error(404, 'Not Found');
	}
}
