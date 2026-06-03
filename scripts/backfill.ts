/**
 * One-time backfill script — imports historical recipes and orders from Excel
 * files into the dev database.
 *
 * Usage: pnpm backfill
 *
 * Input files (expected in ~/Downloads/):
 *   - "Recipe Sheet from Matt.xlsx" → recipes + ingredients + recipe_ingredient
 *   - "sample-week-old.xlsx" → companies + orders
 *
 * This script is ADDITIVE — it does NOT wipe existing data.
 * Companies are inserted (not upserted) — will fail if they already exist.
 */

import ExcelJS from 'exceljs';
import path from 'path';
import os from 'os';
import { Pool } from 'pg';

// Connect directly — override host to localhost since we run outside Docker
const pool = new Pool({
	user: process.env.POSTGRES_USER,
	password: process.env.POSTGRES_PASSWORD,
	database: process.env.POSTGRES_DB,
	host: 'localhost',
	port: 5432,
});

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

const DOWNLOADS = path.join(os.homedir(), 'Downloads');
const RECIPE_FILE = path.join(DOWNLOADS, 'Recipe Sheet from Matt.xlsx');
const ORDER_FILE = path.join(DOWNLOADS, 'sample-week-old.xlsx');

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Parse "100g" / "5.00 ml" / "1 ea" into { amount, unit } */
function parseAmount(raw: string): { amount: number; unit: string } {
	const trimmed = (raw ?? '').toString().trim();
	// Match number (int or decimal) followed by optional space and unit
	const match = trimmed.match(/^([\d.]+)\s*(.+)$/);
	if (!match) return { amount: 0, unit: 'ea' };
	return { amount: parseFloat(match[1]), unit: match[2].trim().toLowerCase() };
}

/** Convert "3/Apr/26" → "2026-04-03" */
function parseOldDate(raw: string | Date): string {
	if (raw instanceof Date) {
		return raw.toISOString().slice(0, 10);
	}
	const str = (raw ?? '').toString().trim();
	// Format: D/Mon/YY
	const match = str.match(/^(\d{1,2})\/(\w{3})\/(\d{2})$/);
	if (!match) throw new Error(`Cannot parse date: "${str}"`);

	const day = match[1].padStart(2, '0');
	const monthMap: Record<string, string> = {
		jan: '01', feb: '02', mar: '03', apr: '04', may: '05', jun: '06',
		jul: '07', aug: '08', sep: '09', oct: '10', nov: '11', dec: '12',
	};
	const month = monthMap[match[2].toLowerCase()];
	if (!month) throw new Error(`Unknown month: "${match[2]}"`);
	const year = `20${match[3]}`;

	return `${year}-${month}-${day}`;
}

/** Normalize meal category: strip " R" suffix */
function normalizeMealCategory(raw: string): string {
	const trimmed = (raw ?? '').toString().trim();
	// Strip trailing " R"
	const base = trimmed.replace(/\s+R$/i, '');
	// Capitalize first letter
	return base.charAt(0).toUpperCase() + base.slice(1).toLowerCase();
}

// ---------------------------------------------------------------------------
// Step 1: Import Recipes + Ingredients
// ---------------------------------------------------------------------------

interface RecipeRow {
	recipe: string;
	amount: string;
	ingredient: string;
}

async function importRecipes() {
	console.log('[backfill] Reading recipe file...');
	const wb = new ExcelJS.Workbook();
	await wb.xlsx.readFile(RECIPE_FILE);
	const ws = wb.worksheets[0];

	// Parse all rows (skip header)
	const rows: RecipeRow[] = [];
	ws.eachRow((row, rowNum) => {
		if (rowNum === 1) return; // skip header
		const recipe = (row.getCell(1).value ?? '').toString().trim();
		const amount = (row.getCell(2).value ?? '').toString().trim();
		const ingredient = (row.getCell(3).value ?? '').toString().trim();
		if (recipe && ingredient) {
			rows.push({ recipe, amount, ingredient });
		}
	});

	console.log(`[backfill] Parsed ${rows.length} recipe-ingredient rows.`);

	// Extract unique recipes and ingredients
	const uniqueRecipes = [...new Set(rows.map((r) => r.recipe))];
	const uniqueIngredients = [...new Set(rows.map((r) => r.ingredient))];

	console.log(`[backfill] ${uniqueRecipes.length} unique recipes, ${uniqueIngredients.length} unique ingredients.`);

	// Insert recipes (case-insensitive lookup map)
	const recipeIds: Record<string, string> = {};
	for (const name of uniqueRecipes) {
		const { rows: inserted } = await pool.query(
			`INSERT INTO recipe (name) VALUES ($1)
			 ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name
			 RETURNING id`,
			[name]
		);
		recipeIds[name] = inserted[0].id;
		recipeIds[name.toLowerCase()] = inserted[0].id;
	}
	console.log(`[backfill] ${uniqueRecipes.length} recipes inserted.`);

	// Insert ingredients
	const ingredientIds: Record<string, string> = {};
	for (const name of uniqueIngredients) {
		const { rows: inserted } = await pool.query(
			`INSERT INTO ingredient (name) VALUES ($1)
			 ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name
			 RETURNING id`,
			[name]
		);
		ingredientIds[name] = inserted[0].id;
	}
	console.log(`[backfill] ${uniqueIngredients.length} ingredients inserted.`);

	// Insert recipe_ingredient links
	let linkCount = 0;
	for (const row of rows) {
		const recipeId = recipeIds[row.recipe];
		const ingredientId = ingredientIds[row.ingredient];
		const { amount, unit } = parseAmount(row.amount);

		if (amount <= 0) continue; // skip invalid amounts

		await pool.query(
			`INSERT INTO recipe_ingredient (recipe_id, ingredient_id, amount, unit)
			 VALUES ($1, $2, $3, $4)
			 ON CONFLICT (recipe_id, ingredient_id) DO UPDATE
			 SET amount = EXCLUDED.amount, unit = EXCLUDED.unit`,
			[recipeId, ingredientId, amount, unit]
		);
		linkCount++;
	}
	console.log(`[backfill] ${linkCount} recipe-ingredient links inserted.`);

	return recipeIds;
}

// ---------------------------------------------------------------------------
// Step 2: Import Companies + Orders
// ---------------------------------------------------------------------------

interface OldOrderRow {
	customer: string;
	servingDate: string;
	recipe: string;
	meal: string;
	orderAmount: number;
	client: string | null;
	frozen: boolean;
	chilled: boolean;
	shipmentDate: string;
}

async function importOrders(recipeIds: Record<string, string>) {
	console.log('[backfill] Reading order file...');
	const wb = new ExcelJS.Workbook();
	await wb.xlsx.readFile(ORDER_FILE);
	const ws = wb.worksheets[0];

	// Parse rows (skip header)
	const rows: OldOrderRow[] = [];
	ws.eachRow((row, rowNum) => {
		if (rowNum === 1) return; // skip header

		const customer = (row.getCell(1).value ?? '').toString().trim();
		const servingDateRaw = row.getCell(2).value;
		const recipe = (row.getCell(3).value ?? '').toString().trim();
		const meal = (row.getCell(5).value ?? '').toString().trim();
		const orderAmount = parseFloat((row.getCell(8).value ?? '0').toString());
		const client = (row.getCell(10).value ?? '').toString().trim() || null;
		const frozen = (row.getCell(11).value ?? '').toString().trim().toLowerCase() === 'x';
		const chilled = (row.getCell(12).value ?? '').toString().trim().toLowerCase() === 'x';
		const shipmentDateRaw = row.getCell(13).value;

		if (!customer || !recipe || !servingDateRaw || !shipmentDateRaw) return;

		rows.push({
			customer,
			servingDate: parseOldDate(servingDateRaw as string | Date),
			recipe,
			meal,
			orderAmount,
			client,
			frozen,
			chilled,
			shipmentDate: parseOldDate(shipmentDateRaw as string | Date),
		});
	});

	console.log(`[backfill] Parsed ${rows.length} order rows.`);

	// Extract and insert unique companies
	const uniqueCompanies = [...new Set(rows.map((r) => r.customer))];
	console.log(`[backfill] ${uniqueCompanies.length} companies: ${uniqueCompanies.join(', ')}`);

	const companyIds: Record<string, string> = {};
	for (const name of uniqueCompanies) {
		const { rows: inserted } = await pool.query(
			`INSERT INTO company (name) VALUES ($1) RETURNING id`,
			[name]
		);
		companyIds[name] = inserted[0].id;
	}
	console.log(`[backfill] ${uniqueCompanies.length} companies inserted.`);

	// Insert orders
	let orderCount = 0;
	let skipped = 0;
	let autoCreatedRecipes = 0;
	for (const row of rows) {
		const companyId = companyIds[row.customer];
		// Case-insensitive recipe lookup
		let recipeId = recipeIds[row.recipe] ?? recipeIds[row.recipe.toLowerCase()] ?? null;
		const mealCategory = normalizeMealCategory(row.meal);
		const storage = row.frozen ? 'frozen' : row.chilled ? 'chilled' : null;
		const notes = row.client; // replacement meal client info goes to notes

		if (!recipeId) {
			// Auto-create missing recipe (no ingredients — just name)
			const { rows: created } = await pool.query(
				`INSERT INTO recipe (name) VALUES ($1)
				 ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name
				 RETURNING id`,
				[row.recipe]
			);
			recipeId = created[0].id;
			recipeIds[row.recipe] = recipeId;
			recipeIds[row.recipe.toLowerCase()] = recipeId;
			autoCreatedRecipes++;
		}

		if (row.orderAmount <= 0) {
			skipped++;
			continue;
		}

		await pool.query(
			`INSERT INTO "order" (
				company_id, recipe_id, order_source, status,
				meal_category, amount, serving_date, shipment_date,
				frozen, chilled, client_note
			) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)`,
			[
				companyId,
				recipeId,
				'excel_import',
				'pending',
				mealCategory,
				row.orderAmount,
				row.servingDate,
				row.shipmentDate,
				row.frozen,
				row.chilled,
				notes,
			]
		);
		orderCount++;
	}

	console.log(`[backfill] ${orderCount} orders inserted, ${skipped} skipped.`);
	if (autoCreatedRecipes > 0) {
		console.log(`[backfill] ${autoCreatedRecipes} recipes auto-created (no ingredients on file).`);
	}
}

// ---------------------------------------------------------------------------
// Step 3: Seed placeholder prices
// ---------------------------------------------------------------------------

async function seedPrices() {
	console.log('[backfill] Seeding placeholder prices...');

	const randomBetween = (min: number, max: number) =>
		Math.round((Math.random() * (max - min) + min) * 100) / 100;

	// Ingredient costs: $0.50–$8.00 per unit
	const { rowCount: ingredientCount } = await pool.query(
		`UPDATE ingredient SET cost_per_unit = (0.5 + random() * 7.5)::NUMERIC(10,4)
		 WHERE cost_per_unit IS NULL`
	);
	console.log(`[backfill] ${ingredientCount} ingredients seeded with cost_per_unit.`);

	// Recipe prices: $8.00–$18.00 per meal
	const { rowCount: recipeCount } = await pool.query(
		`UPDATE recipe SET price_per_meal = (8.0 + random() * 10.0)::NUMERIC(10,2)
		 WHERE price_per_meal IS NULL`
	);
	console.log(`[backfill] ${recipeCount} recipes seeded with price_per_meal.`);
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main() {
	console.log('[backfill] Starting backfill...');
	console.log(`[backfill] Recipe file: ${RECIPE_FILE}`);
	console.log(`[backfill] Order file:  ${ORDER_FILE}`);

	const recipeIds = await importRecipes();
	await importOrders(recipeIds);
	await seedPrices();

	console.log('[backfill] Done.');
	await pool.end();
}

main().catch((err) => {
	console.error('[backfill] FATAL:', err);
	pool.end();
	process.exit(1);
});
