import ExcelJS from 'exceljs';
import { getOrders } from '$lib/server/queries/orders';
import { getRecipesWithIngredientsByIds } from '$lib/server/queries/recipes';
import type { Pool } from 'pg';

export async function generateShoppingList(
  db: Pool,
  from: string,
  to: string
): Promise<ExcelJS.Buffer> {

  const orders = await getOrders(db, {
    serving_date_from: new Date(from),
    serving_date_to: new Date(to),
  });


  const recipeIds = [...new Set(orders.map((o) => o.recipe_id))];

  const recipeMap = await getRecipesWithIngredientsByIds(db, recipeIds);

  // 3. Aggregate ingredients
  //    Key: `${ingredient_name}::${unit}` — same name, different unit = separate row
  type AggregatedIngredient = {
    name: string;
    totalAmount: number;
    unit: string;
    allergens: string;
  };

  const ingredientMap = new Map<string, AggregatedIngredient>();

  for (const order of orders) {
    const recipe = recipeMap.get(order.recipe_id);
    if (!recipe) continue;

    for (const ingredient of recipe.ingredients) {
      const key = `${ingredient.name}::${ingredient.unit ?? ''}`;

      if (!ingredientMap.has(key)) {
        const allergenNames = [
          ...new Set(ingredient.allergens.map((a) => a.name)),
        ].sort();

        ingredientMap.set(key, {
          name: ingredient.name,
          totalAmount: 0,
          unit: ingredient.unit ?? '',
          allergens: allergenNames.join(', '),
        });
      }

      // recipe_ingredient.amount × order.amount = contribution to total
      ingredientMap.get(key)!.totalAmount +=
        ingredient.amount * order.amount;
    }
  }

  // 4. Sort by ingredient name (case-insensitive)
  const rows = [...ingredientMap.values()].sort((a, b) =>
    a.name.localeCompare(b.name, undefined, { sensitivity: 'base' })
  );

  // 5. Build workbook
  const workbook = new ExcelJS.Workbook();
  const sheet = workbook.addWorksheet('Shopping List');

  sheet.columns = [
    { header: 'Ingredient',    key: 'name',        width: 32 },
    { header: 'Total Amount',  key: 'totalAmount', width: 14 },
    { header: 'Unit',          key: 'unit',        width: 10 },
    { header: 'Allergens',     key: 'allergens',   width: 36 },
  ];

  // Style header row
  const headerRow = sheet.getRow(1);
  headerRow.font = { bold: true, color: { argb: 'FFFFFFFF' } };
  headerRow.fill = {
    type: 'pattern',
    pattern: 'solid',
    fgColor: { argb: 'FF375623' }, // green — distinct from production sheet blue
  };
  headerRow.alignment = { vertical: 'middle', horizontal: 'center' };
  headerRow.height = 20;

  // Add data rows
  for (const row of rows) {
    sheet.addRow({
      name: row.name,
      totalAmount: Math.round(row.totalAmount * 1000) / 1000, // avoid float noise
      unit: row.unit,
      allergens: row.allergens,
    });
  }

  // Style data rows
  sheet.eachRow((row, rowNumber) => {
    if (rowNumber === 1) return;

    const isEven = rowNumber % 2 === 0;
    row.getCell('totalAmount').alignment = { horizontal: 'right' };
    row.getCell('unit').alignment        = { horizontal: 'center' };

    if (isEven) {
      row.eachCell((cell) => {
        cell.fill = {
          type: 'pattern',
          pattern: 'solid',
          fgColor: { argb: 'FFF2F8F0' }, // green-tinted stripe
        };
      });
    }
  });

  // Number format on amount column — show up to 3 decimal places, drop trailing zeros
  sheet.getColumn('totalAmount').numFmt = '#,##0.###';

  // Freeze header row
  sheet.views = [{ state: 'frozen', ySplit: 1 }];

  return workbook.xlsx.writeBuffer();
}
