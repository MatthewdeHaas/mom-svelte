import type { RequestHandler } from './$types';
import { pool } from '$lib/server/db';
import { generatePackingSlips } from '$lib/pdf/packing-slip';
import { weekBoundsEST } from '$lib/utils/dates';
import { error } from '@sveltejs/kit';
import JSZip from 'jszip';

function resolve(url: URL): string {
  const weekOf = url.searchParams.get('week_of');
  const single = url.searchParams.get('shipment_date');
  const dateRe = /^\d{4}-\d{2}-\d{2}$/;

  if (weekOf?.match(dateRe)) return weekOf;
  if (single?.match(dateRe)) return single;
  throw error(400, 'Provide either week_of or shipment_date (YYYY-MM-DD)');
}

export const GET: RequestHandler = async ({ url }) => {
  const dateParam = resolve(url);
  const slips = await generatePackingSlips(pool, new Date(dateParam));

  if (slips.length === 0) throw error(404, 'No orders found for that week.');

  if (slips.length === 1) {
    return new Response(slips[0].buffer, {
      headers: {
        'Content-Type': 'application/pdf',
        'Content-Disposition': `attachment; filename="${slips[0].filename}"`,
      },
    });
  }

  const zip = new JSZip();
  for (const slip of slips) zip.file(slip.filename, slip.buffer);
  const zipBuffer = await zip.generateAsync({ type: 'nodebuffer' });
  const { from } = weekBoundsEST(dateParam);

  return new Response(zipBuffer, {
    headers: {
      'Content-Type': 'application/zip',
      'Content-Disposition': `attachment; filename="packing-slips-week-of-${from}.zip"`,
    },
  });
};
