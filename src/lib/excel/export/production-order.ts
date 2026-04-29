import ExcelJS from 'exceljs';
import type { RecipeWithIngredients } from '$lib/types';
import { getOrdersByShipmentDate } from '$lib/server/queries/orders';
import { getRecipeWithIngredients } from '$lib/server/queries/recipes';
import type { Pool } from 'pg';

export async function generateProductionOrderSheet(
  db: pool,
  shipmentDate: string
): Promise<ExcelJS.Buffer> {
  // 1. Fetch all orders for the shipment date
	const orders = await getOrdersByShipmentDate(db, new Date(shipmentDate));


  // 2. Collect unique recipe IDs across all orders
  const recipeIds = [...new Set(orders.map((o) => o.recipe_id))];

  // 3. Resolve each recipe with its ingredients (for allergen join)
  const recipeMap = new Map<number, RecipeWithIngredients>();
  await Promise.all(
    recipeIds.map(async (id) => {
      const recipe = await getRecipeWithIngredients(db, id);
      if (recipe) recipeMap.set(id, recipe);
    })
  );

  // 4. Build one row per recipe, aggregating total amount across orders
  type Row = {
    recipe: string;
    mealCategory: string;
    amount: number;
    frozen: boolean;
    chilled: boolean;
    allergens: string;
  };

  const rowMap = new Map<number, Row>();

  for (const order of orders) {
    const recipe = recipeMap.get(order.recipe_id);
    if (!recipe) continue;

    if (!rowMap.has(order.recipe_id)) {
      // Derive allergens: flatten ingredients → allergens, deduplicate
      const allergenNames = [
        ...new Set(
          recipe.ingredients.flatMap((ing) => ing.allergens.map((a) => a.name))
        ),
      ].sort();

			rowMap.set(order.recipe_id, {
				recipe: recipe.name,
				mealCategory: order.meal_category,
				amount: 0,
				frozen: order.frozen,
				chilled: order.chilled,
				allergens: allergenNames.join(', '),
			});
    }

    rowMap.get(order.recipe_id)!.amount += order.amount;
  }

  // 5. Sort by meal category, then recipe name
  const rows = [...rowMap.values()].sort((a, b) => {
    const cat = a.mealCategory.localeCompare(b.mealCategory);
    return cat !== 0 ? cat : a.recipe.localeCompare(b.recipe);
  });

  // 6. Build workbook
  const workbook = new ExcelJS.Workbook();
  const sheet = workbook.addWorksheet('Production Order');

  // Column definitions
  sheet.columns = [
    { header: 'Recipe',        key: 'recipe',       width: 32 },
    { header: 'Meal Category', key: 'mealCategory', width: 18 },
    { header: 'Amount',        key: 'amount',       width: 10 },
    { header: 'Frozen',        key: 'frozen',       width: 10 },
    { header: 'Chilled',       key: 'chilled',      width: 10 },
    { header: 'Allergens',     key: 'allergens',    width: 36 },
  ];

  // Style header row
  const headerRow = sheet.getRow(1);
  headerRow.font = { bold: true, color: { argb: 'FFFFFFFF' } };
  headerRow.fill = {
    type: 'pattern',
    pattern: 'solid',
    fgColor: { argb: 'FF2E75B6' },
  };
  headerRow.alignment = { vertical: 'middle', horizontal: 'center' };
  headerRow.height = 20;

  // Add data rows
  for (const row of rows) {
    sheet.addRow({
      recipe: row.recipe,
      mealCategory: row.mealCategory,
      amount: row.amount,
      frozen: row.frozen ? '✓' : '',
      chilled: row.chilled ? '✓' : '',
      allergens: row.allergens,
    });
  }

  // Style data rows: centre the checkmark columns, zebra-stripe
  sheet.eachRow((row, rowNumber) => {
    if (rowNumber === 1) return;

    const isEven = rowNumber % 2 === 0;
    row.getCell('frozen').alignment  = { horizontal: 'center' };
    row.getCell('chilled').alignment = { horizontal: 'center' };
    row.getCell('amount').alignment  = { horizontal: 'right' };

    if (isEven) {
      row.eachCell((cell) => {
        cell.fill = {
          type: 'pattern',
          pattern: 'solid',
          fgColor: { argb: 'FFF2F7FB' },
        };
      });
    }
  });

  // Freeze header row
  sheet.views = [{ state: 'frozen', ySplit: 1 }];

  return workbook.xlsx.writeBuffer();
}
