import type { PageServerLoad } from './$types';
import { callerFromLocals, internalJson, InternalApiError } from '$lib/server/api-client';
import { error } from '@sveltejs/kit';

/**
 * Shape returned by the Rust `GET /api/flags/drift` endpoint. See
 * `admin-operations-rust/crates/api/src/flags_handler.rs::drift` for
 * the authoritative definition — the fields below mirror it exactly.
 *
 * `committed_*` = parsed from the embedded migration SQL
 *                 (what the files say the state SHOULD be).
 * `live_*`      = what the DB rows actually hold right now.
 */
export interface DriftResponse {
  committed_flags: { name: string; enabled_global: boolean }[];
  committed_overrides: { flag_name: string; company_id: string; enabled: boolean }[];
  live_flags: { name: string; enabled_global: boolean }[];
  live_overrides: { flag_name: string; company_id: string; enabled: boolean }[];
}

export const load: PageServerLoad = async ({ locals }) => {
  const caller = callerFromLocals(locals);

  // The Rust endpoint has its own user-id gate (same one that guards
  // /api/flags/snapshot). If this user isn't the designated operator,
  // the endpoint 403s and we surface 404 — a disabled surface should
  // read like one that never shipped.
  try {
    const drift = await internalJson<DriftResponse>('/api/flags/drift', caller);
    return { drift };
  } catch (e) {
    if (e instanceof InternalApiError && (e.status === 403 || e.status === 503)) {
      error(404, 'Not Found');
    }
    throw e;
  }
};
