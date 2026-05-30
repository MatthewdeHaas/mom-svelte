import { betterAuth } from 'better-auth';
import { admin, magicLink } from 'better-auth/plugins';
import { pool } from './db';

// No need to change existing singular environment variable
const ADMIN_EMAILS = (process.env.ADMIN_EMAILS ?? process.env.ADMIN_EMAIL ?? '')
  .split(',')
  .map(e => e.trim())
  .filter(Boolean);

const EMAIL_FROM = process.env.EMAIL_FROM;
if (!EMAIL_FROM) {
  throw new Error('EMAIL_FROM environment variable is required (e.g. "Meals on the Move <noreply@yourdomain.com>")');
}

export const auth = betterAuth({
  database: pool,
  secret: process.env.BETTER_AUTH_SECRET,
  baseURL: process.env.BETTER_AUTH_URL,
  databaseHooks: {
    user: {
      create: {
        after: async (user) => {
          await pool.query(
            `INSERT INTO users (id, email, role)
             VALUES ($1, $2, $3)
             ON CONFLICT (id) DO NOTHING`,
            [user.id, user.email, user.role ?? 'admin']
          );
        },
      },
    },
  },
  plugins: [
    admin(),
    magicLink({
      sendMagicLink: async ({ email, url }) => {
				if (!ADMIN_EMAILS.includes(email)) return;
        const { Resend } = await import('resend');
        const resend = new Resend(process.env.RESEND_API_KEY);
        await resend.emails.send({
          from: EMAIL_FROM,
          to: email,
          subject: 'Your sign-in link – Meals on the Move',
          html: `<p>Click below to sign in to Meals on the Move. This link expires in 10 minutes.</p><p><a href="${url}">Sign in</a></p>`,
        });
      },
    }),
  ],
});
