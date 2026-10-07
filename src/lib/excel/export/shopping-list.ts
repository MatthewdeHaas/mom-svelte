import ExcelJS from 'exceljs';
import { getOrders } from '$lib/server/queries/orders';
import { getRecipesWithIngredientsByIds } from '$lib/server/queries/recipes';
import { pinXlsxDates } from '$lib/excel/determinism';
import {
  useRustBackend,
  internalFetch,
  type InternalCaller,
} from '$lib/server/queries/_rust-dispatch';
import type { Pool } from 'pg';

export async function generateShoppingList(
  db: Pool,
  from: string,
  to: string,
  caller?: InternalCaller
): Promise<Buffer> {

  // Flag-gated Rust forward — see production-order.ts for the shape rationale.
  if (caller && (await useRustBackend('exports', caller))) {
    const qs = new URLSearchParams({ from, to }).toString();
    const res = await internalFetch(`/api/exports/shopping-list?${qs}`, caller);
    const bytes = new Uint8Array(await res.arrayBuffer());
    return Buffer.from(bytes);
  }

  const orders = await getOrders(db, {
    serving_date_from: from,
    serving_date_to: to,
  });


  const recipeIds = [...new Set(orders.map((o) => o.recipe_id).filter((id): id is string => !!id))];

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
    if (!order.recipe_id) continue;
    const recipe = recipeMap.get(order.recipe_id);
    if (!recipe) continue;

    for (const ingredient of recipe.ingredients) {
      const key = `${ingredient.name}::${ingredient.unit ?? ''}`;

      if (!ingredientMap.has(key)) {
        const allergenNames = [
          ...new Set(ingredient.allergen.map((a) => a.name)),
        ].sort();

        ingredientMap.set(key, {
          name: ingredient.name,
          totalAmount: 0,
          unit: ingredient.unit ?? '',
          allergens: allergenNames.join(', '),
        });
      }

      // recipe_ingredient.amount × production_quantity = contribution to total
      // production_quantity = portion_size * order_amount (or just order_amount if no portion_size)
      const productionQty = order.portion_size != null
        ? Number(order.portion_size) * Number(order.amount)
        : Number(order.amount);
      ingredientMap.get(key)!.totalAmount +=
        ingredient.amount * productionQty;
    }
  }

  // 4. Sort by ingredient name (case-insensitive)
  const rows = [...ingredientMap.values()].sort((a, b) =>
    a.name.localeCompare(b.name, undefined, { sensitivity: 'base' })
  );

  // 5. Build workbook — pin metadata so two runs produce byte-identical output.
  const workbook = new ExcelJS.Workbook();
  workbook.created = new Date(0);
  workbook.modified = new Date(0);
  workbook.creator = '';
  workbook.lastModifiedBy = '';
  workbook.company = '';
  workbook.title = '';
  workbook.description = '';
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

  const buf = await workbook.xlsx.writeBuffer();
  return pinXlsxDates(Buffer.from(buf));
}
