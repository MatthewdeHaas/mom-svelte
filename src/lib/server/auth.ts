import { betterAuth } from 'better-auth';
import { admin, magicLink } from 'better-auth/plugins';
import { pool } from './db';


// No need to change existing singular environment variable
const ADMIN_EMAILS = (process.env.ADMIN_EMAILS ?? process.env.ADMIN_EMAIL ?? '')
  .split(',')
  .map(e => e.trim())
  .filter(Boolean);

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
          from: 'onboarding@resend.dev',
          to: email,
          subject: 'Your sign-in link',
          html: `<a href="${url}">Click here to sign in</a>. Expires in 10 minutes.`,
        });
      },
    }),
  ],
});
