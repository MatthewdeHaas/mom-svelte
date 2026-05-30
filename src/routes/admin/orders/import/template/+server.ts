import type { RequestHandler } from '@sveltejs/kit';
import ExcelJS from 'exceljs';

export const GET: RequestHandler = async ({ locals }) => {
  if (!locals.user) {
    return new Response('Unauthorised', { status: 401 });
  }

  const workbook = new ExcelJS.Workbook();
  const ws = workbook.addWorksheet('Orders');

  ws.columns = [
    { header: 'company',       key: 'company',       width: 28 },
    { header: 'recipe',        key: 'recipe',        width: 28 },
    { header: 'serving_date',  key: 'serving_date',  width: 16 },
    { header: 'shipment_date', key: 'shipment_date', width: 16 },
    { header: 'meal_category', key: 'meal_category', width: 18 },
    { header: 'amount',        key: 'amount',        width: 10 },
    { header: 'storage',       key: 'storage',       width: 14 },
    { header: 'notes',         key: 'notes',         width: 32 },
  ];

  // Blue header row — matching production-order.ts style
  const headerRow = ws.getRow(1);
  headerRow.font = { bold: true, color: { argb: 'FFFFFFFF' } };
  headerRow.fill = {
    type: 'pattern',
    pattern: 'solid',
    fgColor: { argb: 'FF2E75B6' },
  };
  headerRow.alignment = { vertical: 'middle' };
  headerRow.height = 20;

  // Greyed italic example rows
  const exampleFill: ExcelJS.Fill = {
    type: 'pattern',
    pattern: 'solid',
    fgColor: { argb: 'FFD9D9D9' },
  };

  const exampleRows = [
    ['Brightstone Partners', 'Grilled Chicken Caesar', '2025-09-01', '2025-08-31', 'Lunch',     35, 'chilled', ''],
    ['Oakfield Group',       'Salmon & Dill Risotto',  '2025-09-01', '2025-08-31', 'Lunch',     20, 'frozen',  'Allergy note: no sesame'],
    ['Meridian Co',          'Greek Yoghurt Parfait',  '2025-09-02', '2025-09-01', 'Breakfast', 15, 'chilled', ''],
  ];

  for (const data of exampleRows) {
    const row = ws.addRow(data);
    row.eachCell((cell) => {
      cell.fill = exampleFill;
      cell.font = { italic: true, color: { argb: 'FF666666' } };
    });
  }

  // Freeze header row
  ws.views = [{ state: 'frozen', ySplit: 1 }];

  const buffer = await workbook.xlsx.writeBuffer();

  return new Response(buffer as ArrayBuffer, {
    headers: {
      'Content-Type': 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
      'Content-Disposition': 'attachment; filename="order-import-template.xlsx"',
    },
  });
};
