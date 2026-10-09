import { error } from '@sveltejs/kit';
import type { RequestHandler } from './$types';
import { callerFromLocals, internalFetch } from '$lib/server/api-client';

/**
 * `GET /admin/exports/shopping-list?from=&to=`
 *
 * Pure byte-pump — the Rust handler
 * (`admin-operations-rust/crates/api/src/exports.rs::shopping_list`)
 * owns the generator and determinism contract. This route forwards the
 * query string, verifies admin, and streams the response body back with
 * its upstream `Content-Type` + `Content-Disposition` headers intact.
 */
export const GET: RequestHandler = async ({ locals, url }) => {
	if (locals.user?.role !== 'admin') throw error(403, 'Forbidden');

	const from = url.searchParams.get('from');
	const to = url.searchParams.get('to');
	if (!from || !to) throw error(400, 'from and to are required');

	const caller = callerFromLocals(locals);
	const upstream = await internalFetch(
		`/api/exports/shopping-list?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`,
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
				upstream.headers.get('content-type') ??
				'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
			'Content-Disposition': upstream.headers.get('content-disposition') ?? '',
		},
	});
};
