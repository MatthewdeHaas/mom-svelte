/**
 * Seed placeholder prices for all ingredients and recipes.
 *
 * Run: pnpm tsx scripts/seed-prices.ts
 *
 * Ingredients get a random cost_per_unit ($0.50–$8.00).
 * Recipes get a random price_per_meal ($8.00–$18.00).
 */

import pg from 'pg';

const DATABASE_URL = process.env.DATABASE_URL;
if (!DATABASE_URL) {
  console.error('DATABASE_URL environment variable is required');
  process.exit(1);
}

const pool = new pg.Pool({ connectionString: DATABASE_URL });

function randomBetween(min: number, max: number): number {
  return Math.round((Math.random() * (max - min) + min) * 100) / 100;
}

async function main() {
  const client = await pool.connect();
  try {
    await client.query('BEGIN');

    // Seed ingredient costs
    const ingredients = await client.query<{ id: string; name: string }>(
      `SELECT id, name FROM ingredient WHERE cost_per_unit IS NULL`
    );
    for (const row of ingredients.rows) {
      const cost = randomBetween(0.5, 8.0);
      await client.query(
        `UPDATE ingredient SET cost_per_unit = $1 WHERE id = $2`,
        [cost, row.id]
      );
    }
    console.log(`Seeded cost_per_unit for ${ingredients.rows.length} ingredients`);

    // Seed recipe prices
    const recipes = await client.query<{ id: string; name: string }>(
      `SELECT id, name FROM recipe WHERE price_per_meal IS NULL`
    );
    for (const row of recipes.rows) {
      const price = randomBetween(8.0, 18.0);
      await client.query(
        `UPDATE recipe SET price_per_meal = $1 WHERE id = $2`,
        [price, row.id]
      );
    }
    console.log(`Seeded price_per_meal for ${recipes.rows.length} recipes`);

    await client.query('COMMIT');
    console.log('Done.');
  } catch (err) {
    await client.query('ROLLBACK');
    throw err;
  } finally {
    client.release();
    await pool.end();
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
