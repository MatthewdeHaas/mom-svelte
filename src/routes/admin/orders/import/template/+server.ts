import type { RequestHandler } from '@sveltejs/kit';
import ExcelJS from 'exceljs';

export const GET: RequestHandler = async ({ locals }) => {
  if (!locals.user) {
    return new Response('Unauthorised', { status: 401 });
  }

  const workbook = new ExcelJS.Workbook();
  const ws = workbook.addWorksheet('Orders');

  ws.columns = [
    { header: 'Customer',       key: 'customer',       width: 28 },
    { header: 'Serving Date',   key: 'serving_date',   width: 16 },
    { header: 'Recipe',         key: 'recipe',         width: 28 },
    { header: 'Meal',           key: 'meal',           width: 14 },
    { header: 'Day',            key: 'day',            width: 8 },
    { header: 'Portion Size',   key: 'portion_size',   width: 14 },
    { header: 'Order Amount',   key: 'amount',         width: 14 },
    { header: 'Client',         key: 'client',         width: 32 },
    { header: 'Storage',        key: 'storage',        width: 10 },
    { header: 'Shipment Date',  key: 'shipment_date',  width: 16 },
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
    ['Aurora CHATS', '2025-09-01', 'Grilled Chicken Caesar', 'Lunch', 2, 100, 28, '', 'c', '2025-08-31'],
    ['MIYM',         '2025-09-01', 'Salmon & Dill Risotto',  'Lunch', 2, 75,  8,  '', 'f', '2025-08-29'],
    ['SMH',          '2025-09-02', 'Greek Yoghurt Parfait',  'Breakfast', 3, 1, 24, '', 'c', '2025-09-01'],
    ['SLEC',         '2025-09-02', 'Applesauce',             'Snack R', 3, 1, 0, 'Aston - Dairy', 'c', '2025-09-01'],
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
