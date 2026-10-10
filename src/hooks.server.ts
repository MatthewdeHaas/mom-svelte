import { randomUUID } from 'node:crypto';
import { auth } from '$lib/server/auth';
import logger from '$lib/server/logger';
import type { Handle, HandleServerError } from '@sveltejs/kit';
import { sequence } from '@sveltejs/kit/hooks';
import { svelteKitHandler } from 'better-auth/svelte-kit';
import { building } from '$app/environment';

// Narrow charset + length cap so a hostile `X-Request-Id` can't inject
// control chars into our logs or Rust's propagated header. Matches the
// Rust-side `MakeRequestUuid`-friendly subset.
const REQUEST_ID_RE = /^[A-Za-z0-9._~-]{1,128}$/;

/**
 * Mint or echo a per-request correlation id and stash it on `locals`.
 * Must come first in `sequence(...)` so every downstream hook — and
 * every route handler — sees `locals.requestId`. The response gets the
 * same id echoed on `x-request-id` so clients and reverse proxies can
 * correlate without reading logs.
 */
const handleRequestId: Handle = async ({ event, resolve }) => {
  const incoming = event.request.headers.get('x-request-id');
  const requestId = incoming && REQUEST_ID_RE.test(incoming) ? incoming : randomUUID();
  event.locals.requestId = requestId;
  const response = await resolve(event);
  response.headers.set('x-request-id', requestId);
  return response;
};

const handleBetterAuth: Handle = async ({ event, resolve }) => {
  if (building) return resolve(event);

	const session = await auth.api.getSession({ headers: event.request.headers });
  event.locals.user = session?.user ?? null;
  event.locals.session = session?.session ?? null;

	return svelteKitHandler({ event, resolve, auth, building });
};

const handleLogging: Handle = async ({ event, resolve }) => {
  const start = Date.now();
  const response = await resolve(event);
  const ms = Date.now() - start;
  const { method } = event.request;
  const { pathname } = event.url;

  // Redact auth pathnames to avoid logging magic link tokens or secrets
  const safePathname = pathname.startsWith('/auth/') ? '/auth/[redacted]' : pathname;

  logger.info(
    { requestId: event.locals.requestId, method, pathname: safePathname, status: response.status, ms },
  );

  return response;
};

export const handle: Handle = sequence(handleRequestId, handleBetterAuth, handleLogging);

export const handleError: HandleServerError = ({ error, event }) => {
  logger.error(
    { requestId: event.locals.requestId, err: error, method: event.request.method, pathname: event.url.pathname },
    'Unhandled server error',
  );
  return { message: 'An unexpected error occurred' };
};
