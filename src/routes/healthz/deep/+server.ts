// End-to-end liveness probe: DB on this side, plus a cross-network
// hop to the Rust service's own `/healthz`. 200 only when both are
// green; 503 otherwise. Separate from `/healthz` because couplng
// Railway's healthcheck to the Rust service would cause cascading
// restarts whenever Rust cycles.

import { json } from '@sveltejs/kit';
import { env } from '$env/dynamic/private';
import { pool } from '$lib/server/db';
import logger from '$lib/server/logger';
import type { RequestHandler } from './$types';

const DB_TIMEOUT_MS = 500;
// Longer than the local DB probe because it crosses the private
// network between SvelteKit and the Rust service.
const RUST_TIMEOUT_MS = 1000;

async function probeDb(): Promise<string> {
	try {
		await Promise.race([
			pool.query('SELECT 1'),
			new Promise((_, reject) =>
				setTimeout(() => reject(new Error('timeout')), DB_TIMEOUT_MS)
			)
		]);
		return 'ok';
	} catch (err) {
		logger.warn({ err }, 'healthz/deep: db ping failed');
		return err instanceof Error ? err.message : 'error';
	}
}

async function probeRust(): Promise<string> {
	const base = env.RUST_API_URL;
	if (!base) return 'RUST_API_URL unset';
	const url = base.replace(/\/$/, '') + '/healthz';
	try {
		const ctl = new AbortController();
		const timer = setTimeout(() => ctl.abort(), RUST_TIMEOUT_MS);
		try {
			const res = await fetch(url, { signal: ctl.signal });
			if (!res.ok) return `rust ${res.status}`;
			return 'ok';
		} finally {
			clearTimeout(timer);
		}
	} catch (err) {
		logger.warn({ err, url }, 'healthz/deep: rust probe failed');
		return err instanceof Error ? err.message : 'error';
	}
}

export const GET: RequestHandler = async () => {
	const [db, rust] = await Promise.all([probeDb(), probeRust()]);
	const ok = db === 'ok' && rust === 'ok';
	return json(
		{ status: ok ? 'ok' : 'degraded', db, rust },
		{ status: ok ? 200 : 503 }
	);
};
