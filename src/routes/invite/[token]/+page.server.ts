import { error, redirect } from '@sveltejs/kit';
import { pool } from '$lib/server/db';
import { getInviteByToken, markInviteUsed } from '$lib/server/queries/invites';
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

  // Stamp the company_id directly on the user row
  await pool.query(
    `UPDATE "user" SET company_id = $1 WHERE id = $2`,
    [invite.company_id, locals.user.id]
  );
  await markInviteUsed(pool, invite.id);

  redirect(302, '/client');
};
