import { auth } from '$lib/server/auth';
import logger from '$lib/server/logger';
import type { Handle, HandleServerError } from '@sveltejs/kit';
import { sequence } from '@sveltejs/kit/hooks';
import { svelteKitHandler } from 'better-auth/svelte-kit';
import { building } from '$app/environment';

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

  logger.info({ method, pathname: safePathname, status: response.status, ms });

  return response;
};

export const handle: Handle = sequence(handleBetterAuth, handleLogging);

export const handleError: HandleServerError = ({ error, event }) => {
  logger.error({ err: error, method: event.request.method, pathname: event.url.pathname }, 'Unhandled server error');
  return { message: 'An unexpected error occurred' };
};
