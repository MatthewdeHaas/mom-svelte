import { auth } from '../src/lib/server/auth';

const email = process.env.ADMIN_EMAIL;
const password = process.env.ADMIN_PASSWORD;

if (!email || !password) {
	console.error('[seed:admin] ERROR: ADMIN_EMAIL and ADMIN_PASSWORD must be set in .env');
	process.exit(1);
}

console.log(`[seed:admin] Creating admin account for ${email}...`);

await auth.api.createUser({
	body: {
		name: 'Admin',
		email,
		password,
		role: 'admin'
	}
});

console.log('[seed:admin] Admin account created successfully');
