import { error, redirect } from '@sveltejs/kit';
import { pool } from '$lib/server/db';
import { getInviteByToken, markInviteUsed } from '$lib/server/queries/invites';
import { internalJson, type InternalCaller } from '$lib/server/queries/_rust-dispatch';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ params, locals }) => {
  const invite = await getInviteByToken(pool, params.token);
  if (!invite) error(404, 'Invite not found');

  if (invite.used_at) {
    return { status: 'used' as const };
  }

  if (invite.expires_at < new Date()) {
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

  // Already on this company — idempotent, just redirect
  if (locals.user.companyId === invite.company_id) {
    await markInviteUsed(pool, invite.id);
    redirect(302, '/client');
  }

  // Stamp company_id via the Rust service. The handler enforces
  // "client self-assign only when companyId is None" — which is
  // exactly the invariant the above conflict/already-set checks
  // establish. Belt and braces: route check + handler check.
  const caller: InternalCaller = {
    userId: locals.user.id,
    role: locals.user.role as 'admin' | 'client',
    companyId: locals.user.companyId ?? null,
  };
  await internalJson(
    `/api/users/${locals.user.id}/assign-company`,
    caller,
    { method: 'POST', body: JSON.stringify({ company_id: invite.company_id }) },
  );
  await markInviteUsed(pool, invite.id);

  redirect(302, '/client');
};
