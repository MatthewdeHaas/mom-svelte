/**
 * Destructive dev seed — wipes all application data and repopulates
 * with realistic multi-company, multi-week data.
 *
 * Usage: pnpm seed:dev
 * Reads connection details from .env (dev DB).
 *
 * WARNING: This wipes ALL data except Better Auth tables ("user", "session",
 * "account", "verification"). Run only against the dev database.
 */

import { pool } from '../src/lib/server/db';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function mondayOf(date: Date): Date {
  const d = new Date(date);
  const day = d.getDay(); // 0 = Sun, 1 = Mon …
  const diff = day === 0 ? -6 : 1 - day;
  d.setDate(d.getDate() + diff);
  d.setHours(0, 0, 0, 0);
  return d;
}

function addDays(date: Date, n: number): Date {
  const d = new Date(date);
  d.setDate(d.getDate() + n);
  return d;
}

function isoDate(d: Date): string {
  return d.toISOString().slice(0, 10);
}

// ---------------------------------------------------------------------------
// Wipe application data (FK-safe deletion order)
// ---------------------------------------------------------------------------

async function wipe() {
  console.log('[seed:dev] Wiping application data...');
  await pool.query(`
    DELETE FROM attendance;
    DELETE FROM person_allergen;
    DELETE FROM person;
    DELETE FROM "order";
    DELETE FROM menu_recipe;
    DELETE FROM menu;
    DELETE FROM recipe_ingredient;
    DELETE FROM ingredient_allergen;
    DELETE FROM recipe;
    DELETE FROM ingredient;
    DELETE FROM allergen;
    DELETE FROM company;
  `);
  console.log('[seed:dev] Wipe complete.');
}

// ---------------------------------------------------------------------------
// Seed companies
// ---------------------------------------------------------------------------

async function seedCompanies(): Promise<Record<string, string>> {
  console.log('[seed:dev] Seeding companies...');

  const companies = [
    { name: 'Brightstone Partners', email: 'orders@brightstone.example', active: true },
    { name: 'Oakfield Group',       email: 'admin@oakfield.example',      active: true },
    { name: 'Meridian Co',          email: null,                           active: true },
    { name: 'Crestview Solutions',  email: 'food@crestview.example',       active: true },
    { name: 'Harlow & Associates',  email: null,                           active: false }, // inactive — edge case
  ];

  const ids: Record<string, string> = {};

  for (const c of companies) {
    const { rows } = await pool.query(
      `INSERT INTO company (name, contact_email, active)
       VALUES ($1, $2, $3)
       RETURNING id`,
      [c.name, c.email, c.active]
    );
    ids[c.name] = rows[0].id;
  }

  console.log(`[seed:dev] ${companies.length} companies inserted.`);
  return ids;
}

// ---------------------------------------------------------------------------
// Seed allergens
// ---------------------------------------------------------------------------

async function seedAllergens(): Promise<Record<string, string>> {
  console.log('[seed:dev] Seeding allergens...');

  const names = ['Gluten', 'Dairy', 'Eggs', 'Nuts', 'Shellfish', 'Soy', 'Sesame'];
  const ids: Record<string, string> = {};

  for (const name of names) {
    const { rows } = await pool.query(
      `INSERT INTO allergen (name) VALUES ($1) RETURNING id`,
      [name]
    );
    ids[name] = rows[0].id;
  }

  console.log(`[seed:dev] ${names.length} allergens inserted.`);
  return ids;
}

// ---------------------------------------------------------------------------
// Seed ingredients
// ---------------------------------------------------------------------------

async function seedIngredients(
  allergenIds: Record<string, string>
): Promise<Record<string, string>> {
  console.log('[seed:dev] Seeding ingredients...');

  const ingredients: Array<{ name: string; allergens: string[] }> = [
    { name: 'Chicken Breast',    allergens: [] },               // no allergens
    { name: 'Salmon Fillet',     allergens: [] },
    { name: 'Lamb Shoulder',     allergens: [] },
    { name: 'Avocado',           allergens: [] },
    { name: 'Greek Yoghurt',     allergens: ['Dairy'] },
    { name: 'Granola',           allergens: ['Gluten', 'Nuts'] }, // multiple allergens
    { name: 'Bread Roll',        allergens: ['Gluten', 'Eggs'] },
    { name: 'Smoked Salmon',     allergens: ['Shellfish'] },
    { name: 'Parmesan',          allergens: ['Dairy'] },
    { name: 'Sesame Dressing',   allergens: ['Sesame', 'Soy'] }, // multiple allergens
  ];

  const ids: Record<string, string> = {};

  for (const ing of ingredients) {
    const { rows } = await pool.query(
      `INSERT INTO ingredient (name, active) VALUES ($1, true) RETURNING id`,
      [ing.name]
    );
    const ingredientId = rows[0].id;
    ids[ing.name] = ingredientId;

    for (const allergenName of ing.allergens) {
      await pool.query(
        `INSERT INTO ingredient_allergen (ingredient_id, allergen_id) VALUES ($1, $2)`,
        [ingredientId, allergenIds[allergenName]]
      );
    }
  }

  console.log(`[seed:dev] ${ingredients.length} ingredients inserted.`);
  return ids;
}

// ---------------------------------------------------------------------------
// Seed recipes
// ---------------------------------------------------------------------------

async function seedRecipes(
  ingredientIds: Record<string, string>
): Promise<Record<string, string>> {
  console.log('[seed:dev] Seeding recipes...');

  const recipes: Array<{
    name: string;
    active: boolean;
    ingredients: Array<{ name: string; amount: number; unit: string }>;
  }> = [
    {
      name: 'Avocado Toast & Eggs',
      active: true,
      ingredients: [
        { name: 'Avocado',    amount: 0.5,  unit: 'each' },
        { name: 'Bread Roll', amount: 1,    unit: 'each' },
      ],
    },
    {
      name: 'Greek Yoghurt Parfait',
      active: true,
      ingredients: [
        { name: 'Greek Yoghurt', amount: 150, unit: 'g' },
        { name: 'Granola',       amount: 50,  unit: 'g' },
      ],
    },
    {
      name: 'Smoked Salmon Bagel',
      active: true,
      ingredients: [
        { name: 'Smoked Salmon', amount: 80, unit: 'g' },
        { name: 'Bread Roll',    amount: 1,  unit: 'each' },
      ],
    },
    {
      name: 'Grilled Chicken Caesar',
      active: true,
      ingredients: [
        { name: 'Chicken Breast',  amount: 150, unit: 'g' },
        { name: 'Parmesan',        amount: 20,  unit: 'g' },
        { name: 'Sesame Dressing', amount: 30,  unit: 'ml' },
      ],
    },
    {
      name: 'Thai Green Curry',
      active: true,
      ingredients: [
        { name: 'Chicken Breast', amount: 200, unit: 'g' },
      ],
    },
    {
      name: 'Salmon & Dill Risotto',
      active: true,
      ingredients: [
        { name: 'Salmon Fillet', amount: 180, unit: 'g' },
        { name: 'Parmesan',      amount: 30,  unit: 'g' },
      ],
    },
    {
      name: 'Lamb Tagine',
      active: true,
      ingredients: [
        { name: 'Lamb Shoulder', amount: 200, unit: 'g' },
      ],
    },
    {
      name: 'Granola & Berry Bowl',
      active: false, // inactive recipe — edge case
      ingredients: [
        { name: 'Granola',       amount: 80,  unit: 'g' },
        { name: 'Greek Yoghurt', amount: 100, unit: 'g' },
      ],
    },
  ];

  const ids: Record<string, string> = {};

  for (const recipe of recipes) {
    const { rows } = await pool.query(
      `INSERT INTO recipe (name, active) VALUES ($1, $2) RETURNING id`,
      [recipe.name, recipe.active]
    );
    const recipeId = rows[0].id;
    ids[recipe.name] = recipeId;

    for (const ing of recipe.ingredients) {
      await pool.query(
        `INSERT INTO recipe_ingredient (recipe_id, ingredient_id, amount, unit)
         VALUES ($1, $2, $3, $4)`,
        [recipeId, ingredientIds[ing.name], ing.amount, ing.unit]
      );
    }
  }

  console.log(`[seed:dev] ${recipes.length} recipes inserted.`);
  return ids;
}

// ---------------------------------------------------------------------------
// Seed orders — 3 weeks × multiple companies
// ---------------------------------------------------------------------------

async function seedOrders(
  companyIds: Record<string, string>,
  recipeIds: Record<string, string>
): Promise<void> {
  console.log('[seed:dev] Seeding orders...');

  const now = new Date();
  const thisMonday = mondayOf(now);
  const lastMonday = addDays(thisMonday, -7);
  const nextMonday = addDays(thisMonday, 7);

  // Admin user ID for created_by — look up the first admin in "user"
  const { rows: adminRows } = await pool.query(
    `SELECT id FROM "user" WHERE role = 'admin' LIMIT 1`
  );
  const createdBy: string | null = adminRows[0]?.id ?? null;

  type OrderSpec = {
    company: string;
    recipe: string;
    serving: Date;
    shipment: Date;
    category: string;
    amount: number;
    frozen: boolean;
    chilled: boolean;
    recurring: boolean;
    note: string | null;
  };

  const orders: OrderSpec[] = [];

  // ------------------------------------------------------------------
  // Last week — Brightstone + Oakfield (tests zip packing-slip export)
  // ------------------------------------------------------------------
  const lastWeekDays = [0, 1, 2, 3, 4].map((d) => addDays(lastMonday, d)); // Mon–Fri

  for (const day of lastWeekDays) {
    const ship = addDays(day, -1);
    orders.push(
      {
        company: 'Brightstone Partners',
        recipe: 'Grilled Chicken Caesar',
        serving: day, shipment: ship,
        category: 'Lunch', amount: 35,
        frozen: false, chilled: true, recurring: true,
        note: null,
      },
      {
        company: 'Brightstone Partners',
        recipe: 'Thai Green Curry',
        serving: day, shipment: ship,
        category: 'Dinner', amount: 20,
        frozen: true, chilled: false, recurring: false,
        note: null,
      },
      {
        company: 'Oakfield Group',
        recipe: 'Avocado Toast & Eggs',
        serving: day, shipment: ship,
        category: 'Breakfast', amount: 18,
        frozen: false, chilled: true, recurring: false,
        note: null,
      }
    );
  }

  // ------------------------------------------------------------------
  // This week — Brightstone + Meridian + Crestview
  // ------------------------------------------------------------------
  const thisWeekDays = [0, 1, 2, 3, 4].map((d) => addDays(thisMonday, d));

  for (const day of thisWeekDays) {
    const ship = addDays(day, -1);
    orders.push(
      {
        company: 'Brightstone Partners',
        recipe: 'Salmon & Dill Risotto',
        serving: day, shipment: ship,
        category: 'Lunch', amount: 40,
        frozen: false, chilled: true, recurring: true,
        note: null,
      },
      {
        company: 'Meridian Co',
        recipe: 'Greek Yoghurt Parfait',
        serving: day, shipment: ship,
        category: 'Breakfast', amount: 25,
        frozen: false, chilled: true, recurring: false,
        note: 'Please deliver before 8am',
      },
      {
        company: 'Meridian Co',
        recipe: 'Lamb Tagine',
        serving: day, shipment: ship,
        category: 'Lunch', amount: 30,
        frozen: false, chilled: true, recurring: false,
        note: null,
      },
      {
        company: 'Crestview Solutions',
        recipe: 'Smoked Salmon Bagel',
        serving: day, shipment: ship,
        category: 'Breakfast', amount: 15,
        frozen: false, chilled: true, recurring: false,
        note: null,
      },
      {
        company: 'Crestview Solutions',
        recipe: 'Grilled Chicken Caesar',
        serving: day, shipment: ship,
        category: 'Lunch', amount: 22,
        frozen: false, chilled: true, recurring: false,
        note: 'Extra dressing on the side',
      }
    );
  }

  // ------------------------------------------------------------------
  // Next week — Brightstone + Oakfield (forward-planning)
  // ------------------------------------------------------------------
  const nextWeekDays = [0, 1, 2, 3, 4].map((d) => addDays(nextMonday, d));

  for (const day of nextWeekDays) {
    const ship = addDays(day, -1);
    orders.push(
      {
        company: 'Brightstone Partners',
        recipe: 'Thai Green Curry',
        serving: day, shipment: ship,
        category: 'Lunch', amount: 35,
        frozen: false, chilled: true, recurring: false,
        note: null,
      },
      {
        company: 'Oakfield Group',
        recipe: 'Salmon & Dill Risotto',
        serving: day, shipment: ship,
        category: 'Lunch', amount: 28,
        frozen: false, chilled: true, recurring: false,
        note: null,
      }
    );
  }

  // ------------------------------------------------------------------
  // Insert all orders
  // ------------------------------------------------------------------
  for (const o of orders) {
    const cId = companyIds[o.company];
    const rId = recipeIds[o.recipe];
    if (!cId || !rId) {
      console.warn(`[seed:dev] Skipping order — unknown company/recipe: ${o.company} / ${o.recipe}`);
      continue;
    }

    await pool.query(
      `INSERT INTO "order" (
         company_id, recipe_id, created_by, order_source, status,
         meal_category, amount, serving_date, shipment_date,
         frozen, chilled, recurring, client
       ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)`,
      [
        cId, rId, createdBy, 'admin_manual', 'confirmed',
        o.category, o.amount, isoDate(o.serving), isoDate(o.shipment),
        o.frozen, o.chilled, o.recurring, o.note,
      ]
    );
  }

  console.log(`[seed:dev] ${orders.length} orders inserted.`);
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

await wipe();
const companyIds   = await seedCompanies();
const allergenIds  = await seedAllergens();
const ingredientIds = await seedIngredients(allergenIds);
const recipeIds    = await seedRecipes(ingredientIds);
await seedOrders(companyIds, recipeIds);

await pool.end();

console.log('[seed:dev] Done. Dev database is ready.');
