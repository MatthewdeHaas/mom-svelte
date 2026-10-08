/**
 * Scratch verification: generate each export twice from identical DB state
 * and assert sha256 equality. Pins the determinism contract from Track 2.
 *
 * Usage: pnpm exec tsx scripts/verify-determinism.ts
 * Requires .env.test loaded + meals_test migrated.
 */
import { Pool } from 'pg';
import { createHash } from 'crypto';
import { createCompany } from '../src/lib/server/queries/companies';
import { createRecipe } from '../src/lib/server/queries/recipes';
import { createOrder } from '../src/lib/server/queries/orders';
import { generateProductionOrderSheet } from '../src/lib/excel/export/production-order';
import { generateShoppingList } from '../src/lib/excel/export/shopping-list';
import { generatePackingSlips } from '../src/lib/pdf/packing-slip';

function sha(buf: Buffer): string {
  return createHash('sha256').update(buf).digest('hex');
}

async function main() {
  const connectionString = `postgresql://${process.env.POSTGRES_USER}:${process.env.POSTGRES_PASSWORD}@${process.env.POSTGRES_HOST}:5432/${process.env.POSTGRES_DB}`;
  const pool = new Pool({ connectionString });

  // Isolate inside a transaction we never commit.
  const client = await pool.connect();
  await client.query('BEGIN');

  try {
    // Seed deterministic test data.
    const companyA = await createCompany(client, { name: 'Alpha Co', shipping_address: '1 Alpha St' });
    const companyB = await createCompany(client, { name: 'Beta Co',  shipping_address: '2 Beta Ave' });
    const recipeA = await createRecipe(client, { name: 'Chicken Curry' });
    const recipeB = await createRecipe(client, { name: 'Beef Stew' });

    const serving  = new Date('2025-06-15');
    const shipment = new Date('2025-06-10');

    // Two orders with same (date, company) to force the id tiebreaker.
    for (const [co, re] of [
      [companyA, recipeA],
      [companyA, recipeB],
      [companyB, recipeA],
      [companyB, recipeB],
    ] as const) {
      await createOrder(client, {
        company_id: co.id,
        recipe_id: re.id,
        order_source: 'admin_manual',
        meal_category: 'lunch',
        amount: 10,
        portion_size: 1.5,
        day: 1,
        serving_date: serving,
        shipment_date: shipment,
        chilled: true,
        client: 'Test Client',
      });
    }

    // Build two of each export back-to-back from the same client/txn.
    // Use the per-test client as the db — queries accept Pool|PoolClient.
    const po1 = await generateProductionOrderSheet(client as unknown as Pool, '2025-06-10', '2025-06-10');
    const po2 = await generateProductionOrderSheet(client as unknown as Pool, '2025-06-10', '2025-06-10');

    const sl1 = await generateShoppingList(client as unknown as Pool, '2025-06-10', '2025-06-10');
    const sl2 = await generateShoppingList(client as unknown as Pool, '2025-06-10', '2025-06-10');

    const ps1 = await generatePackingSlips(client as unknown as Pool, new Date('2025-06-10'));
    const ps2 = await generatePackingSlips(client as unknown as Pool, new Date('2025-06-10'));

    const results = [
      ['production-order.xlsx', sha(po1), sha(po2), po1.equals(po2)],
      ['shopping-list.xlsx',    sha(sl1), sha(sl2), sl1.equals(sl2)],
    ];

    console.log('\n=== Determinism check (run1 vs run2) ===');
    for (const [name, h1, h2, eq] of results) {
      console.log(`${eq ? '✓' : '✗'} ${name}`);
      console.log(`  run1: ${h1}`);
      console.log(`  run2: ${h2}`);
    }

    console.log(`\npacking slips: ${ps1.length} vs ${ps2.length}`);
    let allPsEq = ps1.length === ps2.length;
    for (let i = 0; i < Math.min(ps1.length, ps2.length); i++) {
      const h1 = sha(ps1[i].buffer);
      const h2 = sha(ps2[i].buffer);
      const eq = ps1[i].buffer.equals(ps2[i].buffer);
      if (!eq) allPsEq = false;
      console.log(`${eq ? '✓' : '✗'} ${ps1[i].filename} (${ps1[i].companyName})`);
      console.log(`  run1: ${h1}`);
      console.log(`  run2: ${h2}`);
    }

    const allEq = results.every((r) => r[3]) && allPsEq;
    console.log(`\n${allEq ? '✅ ALL DETERMINISTIC' : '❌ NON-DETERMINISM DETECTED'}\n`);
    process.exitCode = allEq ? 0 : 1;
  } finally {
    await client.query('ROLLBACK');
    client.release();
    await pool.end();
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
