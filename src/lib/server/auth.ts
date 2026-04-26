import { betterAuth } from 'better-auth';
import { admin, magicLink } from 'better-auth/plugins';
import { pool } from './db';

export const auth = betterAuth({
  database: pool,
  secret: process.env.BETTER_AUTH_SECRET,
  baseURL: process.env.BETTER_AUTH_URL,
  plugins: [
    admin(),
    magicLink({
      sendMagicLink: async ({ email, url }) => {
        if (email !== process.env.ADMIN_EMAIL) return;
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
