import { Pool } from 'pg';
import { env } from '$env/static/private';

// SvelteKit doesn't support variable interpolation in .env files (but Docker does)
const connectionString = `postgresql://${process.env.POSTGRES_USER}:${process.env.POSTGRES_PASSWORD}@${process.env.POSTGRES_HOST}:5432/${process.env.POSTGRES_DB}`;

export const pool = new Pool({ connectionString });
