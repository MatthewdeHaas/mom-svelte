// DB-only liveness probe. Mirrors `admin-operations-rust/crates/api/src/health.rs`:
// 200 on a round-tripped `SELECT 1`, 503 on timeout or driver failure.
// Public, no auth, no side effects. This is the path Railway should
// probe (not `/` — that returns 200 even with the DB down).

import { json } from '@sveltejs/kit';
import { pool } from '$lib/server/db';
import logger from '$lib/server/logger';
import type { RequestHandler } from './$types';

// 500ms is tight on purpose: a healthy local pool answers in single
// digits of ms. Anything slower is a signal to fail fast and let the
// platform cycle us.
const PING_TIMEOUT_MS = 500;

export const GET: RequestHandler = async () => {
	try {
		await Promise.race([
			pool.query('SELECT 1'),
			new Promise((_, reject) =>
				setTimeout(() => reject(new Error('db ping timeout')), PING_TIMEOUT_MS)
			)
		]);
		return json({ status: 'ok' });
	} catch (err) {
		logger.warn({ err }, 'healthz: db ping failed');
		return json(
			{ status: 'degraded', reason: 'db' },
			{ status: 503 }
		);
	}
};
