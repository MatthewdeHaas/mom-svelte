import type { RequestHandler } from './$types';
import { generatePackingSlips } from '$lib/pdf/packing-slip';
import { weekBoundsEST } from '$lib/utils/dates';
import { error } from '@sveltejs/kit';
import JSZip from 'jszip';
import type { InternalCaller } from '$lib/server/api-client';

function resolve(url: URL): string {
  const weekOf = url.searchParams.get('week_of');
  const single = url.searchParams.get('shipment_date');
  const dateRe = /^\d{4}-\d{2}-\d{2}$/;
  if (weekOf?.match(dateRe)) return weekOf;
  if (single?.match(dateRe)) return single;
  throw error(400, 'Provide either week_of or shipment_date (YYYY-MM-DD)');
}

function slugify(str: string): string {
  return str.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/(^-|-$)/g, '');
}

export const GET: RequestHandler = async ({ url, locals }) => {
  if (!locals.user) throw error(401, 'Unauthorised');

  const caller: InternalCaller = {
    userId: locals.user.id,
    role: locals.user.role === 'client' ? 'client' : 'admin',
    companyId: locals.user.companyId ?? null,
  };

  const dateParam = resolve(url);
  const slips = await generatePackingSlips(new Date(dateParam), caller);

  if (slips.length === 0) throw error(404, 'No orders found for that week.');

  if (slips.length === 1) {
    return new Response(slips[0].buffer as unknown as BodyInit, {
      headers: {
        'Content-Type': 'application/pdf',
        'Content-Disposition': `attachment; filename="${slips[0].filename}"`,
      },
    });
  }

  const zip = new JSZip();
  for (const slip of slips) {
    const folder = slugify(slip.companyName);
    zip.file(`${folder}/${slip.filename}`, slip.buffer);
  }

  const zipBuffer = await zip.generateAsync({ type: 'nodebuffer' });
  const { from } = weekBoundsEST(dateParam);

  return new Response(zipBuffer as unknown as BodyInit, {
    headers: {
      'Content-Type': 'application/zip',
      'Content-Disposition': `attachment; filename="packing-slips-week-of-${from}.zip"`,
    },
  });
};
