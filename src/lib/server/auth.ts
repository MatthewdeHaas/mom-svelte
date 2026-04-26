import { betterAuth } from 'better-auth';
import { pool } from '$lib/server/db';

export const auth = betterAuth({
    database: pool
});
