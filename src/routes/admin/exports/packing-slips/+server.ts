import { error } from '@sveltejs/kit';
import type { RequestHandler } from './$types';
import { callerFromLocals, internalFetch } from '$lib/server/api-client';

/**
 * `GET /admin/exports/packing-slips?date=YYYY-MM-DD`
 *
 * Pure byte-pump — the Rust handler
 * (`admin-operations-rust/crates/api/src/exports.rs::packing_slips`)
 * decides single-PDF vs multi-slip-ZIP based on how many
 * `(company, shipment_date)` groups the EST week contains, and emits
 * the right `Content-Type` + attachment `Content-Disposition`.
 *
 *   - 0 groups → upstream 404, forwarded with its text body.
 *   - 1 group  → `application/pdf` with the slip's own filename.
 *   - N groups → `application/zip` named `packing-slips-week-of-<mon>.zip`
 *     with per-company subfolders.
 *
 * No `JSZip` import, no base64 decode, no single-vs-zip branching here —
 * the HTTP layer is just a pipe.
 */
export const GET: RequestHandler = async ({ locals, url }) => {
	if (locals.user?.role !== 'admin') throw error(403, 'Forbidden');

	const date = url.searchParams.get('date');
	if (!date) throw error(400, 'date is required');

	const caller = callerFromLocals(locals);
	const upstream = await internalFetch(
		`/api/exports/packing-slips?date=${encodeURIComponent(date)}`,
		caller,
	);
	if (!upstream.ok) {
		const body = await upstream.text().catch(() => 'export failed');
		throw error(upstream.status, body);
	}
	return new Response(upstream.body, {
		status: 200,
		headers: {
			'Content-Type':
				upstream.headers.get('content-type') ?? 'application/octet-stream',
			'Content-Disposition': upstream.headers.get('content-disposition') ?? '',
		},
	});
};
