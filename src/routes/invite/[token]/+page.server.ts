import { error, redirect } from '@sveltejs/kit';
import {
	consumeInviteByToken,
	getInviteByTokenPublic,
	internalJson
} from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals, type InternalCaller } from '$lib/server/api-client';
import type { PageServerLoad } from './$types';

// Anonymous landing-page visitors have no `locals.user`, but we still
// need SOME caller to sign the HMAC headers the Rust service gates
// `/api/*` behind. The public invite endpoints (`/by-token/public`,
// `/consume-by-token`) deliberately skip the admin check — the token's
// 128-bit entropy IS the authorization boundary, same security model
// as a password-reset link.
//
// Role is `client` because that's what this caller is: a not-yet-
// authenticated visitor who, at best, will become a client user after
// consuming the invite. Marking it `admin` would be a lie that could
// bite later if other `/api` endpoints ever read the role for anything
// beyond admin-gating.
const ANON_LANDING_CALLER: InternalCaller = {
	userId: 'anon:invite-landing',
	role: 'client',
	companyId: null
};

export const load: PageServerLoad = async ({ params, locals }) => {
	// Pre-auth read: use the real caller when we have one, fall back to
	// the anon caller for visitors who arrive without a session. Either
	// way, the Rust side doesn't check the role on `/by-token/public`.
	const readCaller = locals.user ? callerFromLocals(locals) : ANON_LANDING_CALLER;
	const invite = await getInviteByTokenPublic(params.token, readCaller);
	if (!invite) error(404, 'Invite not found');

	if (invite.used_at) {
		return { status: 'used' as const };
	}

	if (new Date(invite.expires_at) < new Date()) {
		return { status: 'expired' as const };
	}

	// Not yet authenticated — send them to login; they can return after
	if (!locals.user) {
		redirect(302, `/login`);
	}

	// Authenticated as a different company's user — unlikely but guard it
	if (locals.user.companyId && locals.user.companyId !== invite.company_id) {
		return { status: 'conflict' as const };
	}

	// Real caller for the authenticated consume path.
	const caller = callerFromLocals(locals);

	// Already on this company — idempotent, just consume + redirect.
	// `consumeInviteByToken` returns null if the invite was consumed
	// between the load check above and here (double-tab race); treat
	// as benign and redirect anyway — the user is already on-company.
	if (locals.user.companyId === invite.company_id) {
		await consumeInviteByToken(params.token, caller);
		redirect(302, '/client');
	}

	// Stamp company_id via the Rust service. The handler enforces
	// "client self-assign only when companyId is None" — which is
	// exactly the invariant the above conflict/already-set checks
	// establish. Belt and braces: route check + handler check.
	await internalJson(`/api/users/${locals.user.id}/assign-company`, caller, {
		method: 'POST',
		body: JSON.stringify({ company_id: invite.company_id }),
		requestId: locals.requestId
	});
	// Atomic consume: verifies unused+unexpired and stamps used_at in
	// one SQL statement. 404 here means the invite raced (another tab,
	// expired between load and consume) — surface as 410 Gone so the
	// user sees "this link is no longer valid" rather than a silent
	// redirect to /client where they'd wonder what happened.
	const consumed = await consumeInviteByToken(params.token, caller);
	if (!consumed) error(410, 'Invite is no longer valid');

	redirect(302, '/client');
};
