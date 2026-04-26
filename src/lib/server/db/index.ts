import postgres from 'postgres';
import { env } from '$env/static/private';

const sql = postgres(env.DATABASE_URL);

export default sql;
