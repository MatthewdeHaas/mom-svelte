import type { RequestHandler } from '@sveltejs/kit';
import ExcelJS from 'exceljs';

export const GET: RequestHandler = async ({ locals }) => {
  if (!locals.user) {
    return new Response('Unauthorised', { status: 401 });
  }

  const workbook = new ExcelJS.Workbook();
  const ws = workbook.addWorksheet('Recipes');

  ws.columns = [
    { header: 'Recipe',     key: 'recipe',     width: 30 },
    { header: 'Ingredient', key: 'ingredient', width: 28 },
    { header: 'Amount',     key: 'amount',     width: 12 },
    { header: 'Unit',       key: 'unit',       width: 12 },
    { header: 'Allergens',  key: 'allergens',  width: 24 },
  ];

  // Blue header row
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
    ['Grilled Chicken Caesar', 'Chicken Breast', 0.2,  'kg', ''],
    ['Grilled Chicken Caesar', 'Romaine Lettuce', 0.1, 'kg', ''],
    ['Grilled Chicken Caesar', 'Parmesan',       0.03, 'kg', 'Dairy'],
    ['Salmon & Dill Risotto',  'Salmon Fillet',  0.18, 'kg', 'Fish'],
    ['Salmon & Dill Risotto',  'Arborio Rice',   0.12, 'kg', 'Gluten'],
    ['Salmon & Dill Risotto',  'Fresh Dill',     0.01, 'kg', ''],
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
      'Content-Disposition': 'attachment; filename="recipe-import-template.xlsx"',
    },
  });
};
