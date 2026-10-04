import { betterAuth } from 'better-auth';
import { admin, magicLink } from 'better-auth/plugins';
import { pool } from './db';
import { isRateLimited } from './rate-limit';

// Support both singular ADMIN_EMAIL and comma-separated ADMIN_EMAILS
const ADMIN_EMAILS = (process.env.ADMIN_EMAILS ?? process.env.ADMIN_EMAIL ?? '')
  .split(',')
  .map((e) => e.trim())
  .filter(Boolean);

export async function sendInviteEmail(email: string, inviteUrl: string): Promise<void> {
  if (isRateLimited('invite', email, 5, 60_000)) {
    throw new Error('Too many invite attempts. Please wait a minute.');
  }
  const EMAIL_FROM = process.env.EMAIL_FROM;
  if (!EMAIL_FROM) throw new Error('EMAIL_FROM is not configured');
  const { Resend } = await import('resend');
  const resend = new Resend(process.env.RESEND_API_KEY);
  await resend.emails.send({
    from: EMAIL_FROM,
    to: email,
    subject: "You've been invited to Meals on the Move",
    html: `
      <p>You've been invited to access the Meals on the Move client portal.</p>
      <p>Click the link below to set up your account. This invite expires in 48 hours.</p>
      <p><a href="${inviteUrl}">Accept invitation</a></p>
      <p style="color:#888;font-size:12px;">If you weren't expecting this, you can ignore this email.</p>
    `,
  });
}

export const auth = betterAuth({
  database: pool,
  secret: process.env.BETTER_AUTH_SECRET,
  baseURL: process.env.BETTER_AUTH_URL,
  user: {
    additionalFields: {
      // company_id lives on the "user" table. It is managed by the admin
      // via the app — not settable through the auth API (input: false).
      companyId: {
        type: 'string',
        nullable: true,
        fieldName: 'company_id',
        input: false,
      },
    },
  },
  plugins: [
    admin(),
    magicLink({
      sendMagicLink: async ({ email, url }) => {
        // Rate limit: max 3 magic link requests per email per 15 minutes
        if (isRateLimited('magic-link', email, 3, 15 * 60_000)) {
					console.log('[magic-link] RATE LIMITED:', email);
					return;
				}

        // Only send magic links to registered users. Querying Better Auth's
        // own "user" table is the authoritative check regardless of role.
        const registered = await pool.query<{ role: string | null }>(
          `SELECT role FROM "user" WHERE email = $1`,
          [email]
        );
        if (!registered.rows[0]) return; // not registered — silently drop

        const EMAIL_FROM = process.env.EMAIL_FROM;
        if (!EMAIL_FROM) {
          throw new Error(
            'EMAIL_FROM environment variable is required (e.g. "Meals on the Move <noreply@yourdomain.com>")'
          );
        }

        const { Resend } = await import('resend');
        const resend = new Resend(process.env.RESEND_API_KEY);

        const isAdmin = ADMIN_EMAILS.includes(email);
        await resend.emails.send({
          from: EMAIL_FROM,
          to: email,
          subject: isAdmin
            ? 'Your sign-in link – Meals on the Move Admin'
            : 'Your sign-in link – Meals on the Move Client Portal',
          html: `<p>Click below to sign in to Meals on the Move. This link expires in 10 minutes.</p><p><a href="${url}">Sign in</a></p>`,
        });
      },
    }),
  ],
});
