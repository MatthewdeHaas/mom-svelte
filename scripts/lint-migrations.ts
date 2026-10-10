// Strict regex gate over `scripts/migrations/*.sql`. Fails CI on any
// bare DDL that would make a re-apply non-idempotent. No overrides, no
// allowlist — per the Track 4a plan. If a legitimate pattern starts
// tripping, fix the migration, not the lint.
//
// Comments (`-- ...`) and `/* ... */` blocks are stripped before matching
// so a comment that happens to say `CREATE TABLE` doesn't trigger.
// `CREATE TYPE` enum literals wrapped in `DO $$ BEGIN ... EXCEPTION ...
// END $$` are accepted — Postgres has no `CREATE TYPE IF NOT EXISTS`,
// that's the idiom.

import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';

const MIGRATIONS_DIR = join(process.cwd(), 'scripts', 'migrations');

interface Violation {
	file: string;
	line: number;
	rule: string;
	snippet: string;
}

const RULES: Array<{ name: string; pattern: RegExp }> = [
	{
		name: 'CREATE TABLE without IF NOT EXISTS',
		pattern: /^\s*CREATE\s+TABLE\s+(?!IF\s+NOT\s+EXISTS\b)/i
	},
	{
		name: 'CREATE UNIQUE INDEX without IF NOT EXISTS',
		pattern: /^\s*CREATE\s+UNIQUE\s+INDEX\s+(?!IF\s+NOT\s+EXISTS\b)/i
	},
	{
		name: 'CREATE INDEX without IF NOT EXISTS',
		// Negative lookahead for UNIQUE so the UNIQUE rule above owns that case.
		pattern: /^\s*CREATE\s+INDEX\s+(?!IF\s+NOT\s+EXISTS\b)/i
	},
	{
		name: 'ADD COLUMN without IF NOT EXISTS',
		// Matches both single-line `ALTER TABLE t ADD COLUMN c ...` and
		// the comma-continuation form on its own line inside a multi-
		// column `ALTER TABLE t ADD COLUMN a, ADD COLUMN b` block.
		pattern: /\bADD\s+COLUMN\s+(?!IF\s+NOT\s+EXISTS\b)/i
	},
	{
		// Postgres has no `CREATE TYPE IF NOT EXISTS`. The idiomatic
		// escape is a `DO $$ BEGIN ... EXCEPTION WHEN duplicate_object
		// THEN NULL; END $$` wrapper. We check per-file (not per-line)
		// below so this entry is a marker; the actual exemption logic
		// lives in `fileHasBareCreateType`.
		name: 'CREATE TYPE outside DO $$ ... EXCEPTION block',
		pattern: /^\s*CREATE\s+TYPE\s+/i
	}
];

/**
 * Strip `--` line comments and `/* ... *\/` block comments from SQL
 * text before pattern-matching. Preserves line numbers (replaces with
 * blanks of the same length) so violation reports point at the right
 * source line.
 */
function stripComments(sql: string): string {
	// Block comments first; replace with spaces + newlines so line
	// numbers don't shift.
	let out = sql.replace(/\/\*[\s\S]*?\*\//g, (m) =>
		m.replace(/[^\n]/g, ' ')
	);
	// Line comments: everything from `--` to end-of-line.
	out = out.replace(/--[^\n]*/g, (m) => m.replace(/[^\n]/g, ' '));
	return out;
}

/**
 * True iff the file has at least one `CREATE TYPE` statement that is
 * NOT wrapped in a `DO $$ ... EXCEPTION ... END $$` block. The naive
 * check — "any CREATE TYPE at all" — would fire on legitimate wrapped
 * enum creation, which is the only idempotent form Postgres supports.
 */
function fileHasBareCreateType(stripped: string): boolean {
	// Any CREATE TYPE at all?
	if (!/\bCREATE\s+TYPE\b/i.test(stripped)) return false;
	// Collect every `DO $$ ... END $$` block (non-greedy) and check
	// whether every CREATE TYPE sits inside one WITH an EXCEPTION
	// handler. Match loosely — the exact `duplicate_object` text can
	// vary; the key signal is that the block has an `EXCEPTION` clause.
	const bareRe = /\bCREATE\s+TYPE\b/gi;
	const match = bareRe.exec(stripped);
	if (!match) return false;
	// For each CREATE TYPE occurrence, verify it's inside a DO $$ ...
	// EXCEPTION ... END $$ block.
	let idx = 0;
	while ((bareRe.lastIndex = idx, bareRe.exec(stripped))) {
		const pos = bareRe.lastIndex - 'CREATE TYPE'.length;
		if (!isInsideDoExceptionBlock(stripped, pos)) return true;
		idx = bareRe.lastIndex;
	}
	return false;
}

function isInsideDoExceptionBlock(sql: string, pos: number): boolean {
	// Find the nearest preceding `DO $$` or `DO $tag$`.
	const before = sql.slice(0, pos);
	const doMatch = before.match(/\bDO\s+\$([a-zA-Z0-9_]*)\$[\s\S]*$/i);
	if (!doMatch) return false;
	const tag = doMatch[1] ?? '';
	const terminator = new RegExp(`\\$${tag}\\$`, 'g');
	terminator.lastIndex = pos;
	const end = terminator.exec(sql);
	if (!end) return false;
	const block = sql.slice(before.length - doMatch[0].length, end.index);
	return /\bEXCEPTION\b/i.test(block);
}

async function lintFile(path: string, file: string): Promise<Violation[]> {
	const raw = await readFile(path, 'utf8');
	const stripped = stripComments(raw);
	const violations: Violation[] = [];
	const lines = stripped.split('\n');

	// Per-line checks for the first four rules.
	for (let i = 0; i < lines.length; i++) {
		const line = lines[i];
		for (const rule of RULES.slice(0, 4)) {
			if (rule.pattern.test(line)) {
				violations.push({
					file,
					line: i + 1,
					rule: rule.name,
					snippet: raw.split('\n')[i]?.trim() ?? line.trim()
				});
			}
		}
	}

	// Per-file check for CREATE TYPE.
	if (fileHasBareCreateType(stripped)) {
		// Find a representative line.
		const idx = stripped.search(/\bCREATE\s+TYPE\b/i);
		const lineNo = stripped.slice(0, idx).split('\n').length;
		violations.push({
			file,
			line: lineNo,
			rule: RULES[4].name,
			snippet: raw.split('\n')[lineNo - 1]?.trim() ?? ''
		});
	}

	return violations;
}

async function main() {
	const entries = await readdir(MIGRATIONS_DIR);
	const sqlFiles = entries.filter((f) => f.endsWith('.sql')).sort();

	const allViolations: Violation[] = [];
	for (const file of sqlFiles) {
		const violations = await lintFile(join(MIGRATIONS_DIR, file), file);
		allViolations.push(...violations);
	}

	if (allViolations.length === 0) {
		console.log(`[lint-migrations] clean (${sqlFiles.length} files)`);
		return;
	}

	console.error(`[lint-migrations] ${allViolations.length} violation(s):`);
	for (const v of allViolations) {
		console.error(`  ${v.file}:${v.line}  ${v.rule}`);
		console.error(`    ${v.snippet}`);
	}
	process.exit(1);
}

main().catch((err) => {
	console.error('[lint-migrations] crashed:', err);
	process.exit(2);
});
