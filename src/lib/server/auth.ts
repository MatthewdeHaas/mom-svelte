import { betterAuth } from 'better-auth';
import { pg } from 'better-auth/adapters/postgresql';
import sql from '$lib/server/db';

export const auth = betterAuth({
    database: pg(sql),
});
