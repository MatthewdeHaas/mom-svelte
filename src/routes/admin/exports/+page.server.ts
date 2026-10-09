import { fail, error } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { callerFromLocals, internalFetch } from '$lib/server/api-client';

export const load: PageServerLoad = async ({ locals }) => {
	if (locals.user?.role !== 'admin') throw error(403, 'Forbidden');
	return {};
};

/**
 * Pure byte-pump form actions. Every export's business logic lives in
 * the Rust `/api/exports/*` handlers (see
 * `admin-operations-rust/crates/api/src/exports.rs`); this file only
 * shapes HTTP inputs (parse form, build URL) and forwards the response
 * body + content-type headers back to the browser.
 *
 * Error path: upstream non-2xx → `fail(status, { message })` using the
 * upstream error body as the message. We don't rethrow as 5xx because
 * the browser form flow displays `fail`s inline.
 */

async function forward(
	locals: App.Locals,
	path: string,
	errorKey: string,
): Promise<Response | ReturnType<typeof fail>> {
	const caller = callerFromLocals(locals);
	const upstream = await internalFetch(path, caller);
	if (!upstream.ok) {
		const message = await upstream.text().catch(() => 'export failed');
		return fail(upstream.status, { [errorKey]: message });
	}
	// Byte-pump: forward body + content-type + attachment disposition.
	const contentType =
		upstream.headers.get('content-type') ?? 'application/octet-stream';
	const disposition = upstream.headers.get('content-disposition') ?? '';
	return new Response(upstream.body, {
		status: 200,
		headers: {
			'Content-Type': contentType,
			'Content-Disposition': disposition,
		},
	});
}

export const actions: Actions = {
	productionOrder: async ({ request, locals }) => {
		if (locals.user?.role !== 'admin') throw error(403, 'Forbidden');
		const form = await request.formData();
		const from = String(form.get('from') ?? '');
		const to = String(form.get('to') ?? '');
		if (!from || !to) return fail(400, { productionOrderError: 'from and to are required' });
		return forward(
			locals,
			`/api/exports/production-order?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`,
			'productionOrderError',
		);
	},

	shoppingList: async ({ request, locals }) => {
		if (locals.user?.role !== 'admin') throw error(403, 'Forbidden');
		const form = await request.formData();
		const from = String(form.get('from') ?? '');
		const to = String(form.get('to') ?? '');
		if (!from || !to) return fail(400, { shoppingListError: 'from and to are required' });
		return forward(
			locals,
			`/api/exports/shopping-list?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`,
			'shoppingListError',
		);
	},

	packingSlips: async ({ request, locals }) => {
		if (locals.user?.role !== 'admin') throw error(403, 'Forbidden');
		const form = await request.formData();
		const weekOf = String(form.get('weekOf') ?? '');
		if (!weekOf) return fail(400, { packingSlipsError: 'weekOf is required' });
		// Any date in the EST week works — the Rust handler expands it
		// to the Mon-Sun window via `week_bounds_est` and names the ZIP
		// archive by the resulting Monday.
		return forward(
			locals,
			`/api/exports/packing-slips?date=${encodeURIComponent(weekOf)}`,
			'packingSlipsError',
		);
	},
};
