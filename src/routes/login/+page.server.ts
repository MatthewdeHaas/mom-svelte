import { auth } from '$lib/server/auth';
import { redirect, fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ request }) => {
    const session = await auth.api.getSession({
        headers: request.headers
    });

    if (session) {
        redirect(302, '/admin/dashboard');
    }
};

export const actions: Actions = {
    default: async ({ request }) => {
        const form = await request.formData();
        const email = form.get('email') as string;
        const password = form.get('password') as string;

        try {
            const response = await auth.api.signInEmail({
                body: { email, password },
                asResponse: true
            });

            // Forward the set-cookie header from Better Auth back to the browser
            const setCookie = response.headers.get('set-cookie');
            if (!setCookie) {
                return fail(401, { error: 'Invalid email or password' });
            }

            return new Response(null, {
                status: 302,
                headers: {
                    location: '/admin/dashboard',
                    'set-cookie': setCookie
                }
            });
        } catch {
            return fail(401, { error: 'Invalid email or password' });
        }
    }
};
