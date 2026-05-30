import { betterAuth } from 'better-auth';
import { admin, magicLink } from 'better-auth/plugins';
import { pool } from './db';

// Support both singular ADMIN_EMAIL and comma-separated ADMIN_EMAILS
const ADMIN_EMAILS = (process.env.ADMIN_EMAILS ?? process.env.ADMIN_EMAIL ?? '')
  .split(',')
  .map((e) => e.trim())
  .filter(Boolean);

const EMAIL_FROM = process.env.EMAIL_FROM;
if (!EMAIL_FROM) {
  throw new Error(
    'EMAIL_FROM environment variable is required (e.g. "Meals on the Move <noreply@yourdomain.com>")'
  );
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
        // Only send magic links to registered users. Querying Better Auth's
        // own "user" table is the authoritative check regardless of role.
        const registered = await pool.query<{ role: string | null }>(
          `SELECT role FROM "user" WHERE email = $1`,
          [email]
        );
        if (!registered.rows[0]) return; // not registered — silently drop

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
