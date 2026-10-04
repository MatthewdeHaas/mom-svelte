import ExcelJS from 'exceljs';
import { getOrders } from '$lib/server/queries/orders';
import { pinXlsxDates } from '$lib/excel/determinism';
import type { Pool } from 'pg';

export async function generateProductionOrderSheet(
  db: Pool,
  from: string,
  to: string
): Promise<Buffer> {

  const orders = await getOrders(db, {
    serving_date_from: from,
    serving_date_to: to,
  });

  // Sort by recipe, then customer, then id as a total-order tiebreaker so
  // two runs over the same data produce byte-identical output.
  const sortedOrders = [...orders].sort((a, b) => {
    const recipe = (a.recipe_name ?? '').localeCompare(b.recipe_name ?? '');
    if (recipe !== 0) return recipe;
    const company = a.company_name.localeCompare(b.company_name);
    if (company !== 0) return company;
    return a.id.localeCompare(b.id);
  });

  // Build workbook — explicitly pin every metadata field that ExcelJS would
  // otherwise stamp with `new Date()` or the running user's name.
  const workbook = new ExcelJS.Workbook();
  workbook.created = new Date(0);
  workbook.modified = new Date(0);
  workbook.creator = '';
  workbook.lastModifiedBy = '';
  workbook.company = '';
  workbook.title = '';
  workbook.description = '';
  const sheet = workbook.addWorksheet('Production Order');

  // Column definitions matching the new format
  sheet.columns = [
    { header: 'Customer',                  key: 'customer',      width: 24 },
    { header: 'Serving Date',              key: 'serving_date',  width: 14 },
    { header: 'Recipe',                    key: 'recipe',        width: 32 },
    { header: 'Meal',                      key: 'meal',          width: 12 },
    { header: 'Day',                       key: 'day',           width: 6 },
    { header: 'Portion Size',              key: 'portion_size',  width: 14 },
    { header: 'Order Amount',              key: 'amount',        width: 14 },
    { header: 'Quantity to Produce/Ship',  key: 'qty_produce',   width: 22 },
    { header: 'Client',                    key: 'client',        width: 32 },
    { header: 'Storage',                   key: 'storage',       width: 10 },
    { header: 'Shipment Date',             key: 'shipment_date', width: 14 },
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
  for (const order of sortedOrders) {
    const portionSize = order.portion_size != null ? Number(order.portion_size) : null;
    const amount = Number(order.amount);
    const qtyProduce = portionSize != null ? portionSize * amount : amount;

    sheet.addRow({
      customer: order.company_name,
      serving_date: order.serving_date,
      recipe: order.recipe_name ?? '—',
      meal: order.meal_category ?? '',
      day: order.day ?? '',
      portion_size: portionSize ?? '',
      amount: amount,
      qty_produce: qtyProduce,
      client: order.client ?? '',
      storage: order.frozen ? 'f' : order.chilled ? 'c' : '',
      shipment_date: order.shipment_date,
    });
  }

  // Style data rows
  sheet.eachRow((row, rowNumber) => {
    if (rowNumber === 1) return;

    const isEven = rowNumber % 2 === 0;
    row.getCell('storage').alignment = { horizontal: 'center' };
    row.getCell('amount').alignment = { horizontal: 'right' };
    row.getCell('portion_size').alignment = { horizontal: 'right' };
    row.getCell('qty_produce').alignment = { horizontal: 'right' };
    row.getCell('day').alignment = { horizontal: 'center' };

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

  // Date format for serving_date and shipment_date columns
  sheet.getColumn('serving_date').numFmt = 'yyyy-mm-dd';
  sheet.getColumn('shipment_date').numFmt = 'yyyy-mm-dd';

  // Freeze header row
  sheet.views = [{ state: 'frozen', ySplit: 1 }];

  const buf = await workbook.xlsx.writeBuffer();
  return pinXlsxDates(Buffer.from(buf));
}
