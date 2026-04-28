import { auth } from '$lib/server/auth';
import type { Handle } from '@sveltejs/kit';
import { svelteKitHandler } from 'better-auth/svelte-kit';
import { building } from '$app/environment';

const handleBetterAuth: Handle = async ({ event, resolve }) => {
  if (building) return resolve(event);

	const session = await auth.api.getSession({ headers: event.request.headers });
	console.log('session user:', JSON.stringify(session?.user));
  event.locals.user = session?.user ?? null;
  event.locals.session = session?.session ?? null;

	return svelteKitHandler({ event, resolve, auth, building });
};

export const handle: Handle = handleBetterAuth;
