import { Pool } from 'pg';
import { createHash } from 'crypto';
import { readdir, readFile } from 'fs/promises';
import { join } from 'path';

// --- Config ---

const pool = new Pool({
	connectionString: `postgresql://${process.env.POSTGRES_USER}:${process.env.POSTGRES_PASSWORD}@${process.env.POSTGRES_HOST}:5432/${process.env.POSTGRES_DB}`
});

const MIGRATIONS_DIR = join(process.cwd(), 'scripts', 'migrations');

// --- Helpers ---

const hashFile = (contents: string): string => {
	return createHash('sha256').update(contents).digest('hex');
}

const log = (msg: string) => {
	console.log(`[migrate] ${msg}`);
}

const error = (msg: string) => {
	console.error(`[migrate] ERROR: ${msg}`);
	process.exit(1);
}

// --- Core ---

const ensureMigrationsTable = async (client: any) => {
	await client.query(`
		CREATE TABLE IF NOT EXISTS migrations (
			id          SERIAL PRIMARY KEY,
			filename    TEXT UNIQUE NOT NULL,
			hash        TEXT NOT NULL,
			applied_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
		)
	`);
}
 
const getAppliedMigrations = async (client: any): Promise<Map<string, string>> => {
	const result = await client.query(
		'SELECT filename, hash FROM migrations ORDER BY id ASC'
	);
	return new Map(result.rows.map((r: any) => [r.filename, r.hash]));
}

const getMigrationFiles = async (): Promise<string[]> => {
	const files = await readdir(MIGRATIONS_DIR);
	return files.filter(f => f.endsWith('.sql')).sort(); // lexicographic — relies on 001_, 002_ prefixes
}

const run = async () => {
	const client = await pool.connect();

	try {
		log('Connected to database');

		await ensureMigrationsTable(client);
		log('Migrations table ready');

		const applied = await getAppliedMigrations(client);
		const files = await getMigrationFiles();

		if (files.length === 0) {
				log('No migration files found');
				return;
		}

		// --- Hash check: verify already-applied files haven't been edited ---
		for (const filename of files) {
			if (!applied.has(filename)) continue;

			const contents = await readFile(join(MIGRATIONS_DIR, filename), 'utf8');
			const currentHash = hashFile(contents);
			const storedHash = applied.get(filename)!;

			if (currentHash !== storedHash) {
				error(
					`Migration file "${filename}" has been modified after it was applied.\n` +
					`  Stored hash:  ${storedHash}\n` +
					`  Current hash: ${currentHash}\n` +
					`  Do not edit migration files that have already been applied. Create a new migration instead.`
				);
			}
		}

		// --- Run pending migrations ---
		const pending = files.filter(f => !applied.has(f));

		if (pending.length === 0) {
			log('Database is up to date — no pending migrations');
			return;
		}

		log(`Found ${pending.length} pending migration(s)`);

		for (const filename of pending) {
			const contents = await readFile(join(MIGRATIONS_DIR, filename), 'utf8');
			const hash = hashFile(contents);

			log(`Applying: ${filename}`);

			// Run each migration in its own transaction so a failure
			// doesn't leave the DB in a partial state
			await client.query('BEGIN');
			try {
				await client.query(contents);
				await client.query(
						'INSERT INTO migrations (filename, hash) VALUES ($1, $2)',
						[filename, hash]
				);
				await client.query('COMMIT');
				log(`Applied:  ${filename} (${hash.slice(0, 8)}...)`);
			} catch (err: any) {
				await client.query('ROLLBACK');
				error(
					`Failed to apply "${filename}":\n  ${err.message}\n  Transaction rolled back.`
				);
			}
		}

			log('All migrations applied successfully');
	} finally {
		client.release();
		await pool.end();
	}
}

run();
