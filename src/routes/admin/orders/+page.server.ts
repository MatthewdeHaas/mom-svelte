import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { getOrdersPaginated, setOrdersStatusBatch } from '$lib/server/queries/orders';
import { getCompanies } from '$lib/server/queries/companies';
import { parsePaginationParams } from '$lib/server/queries/pagination';
import { pool } from '$lib/server/db';
import type { OrderStatus } from '$lib/types';

export const load: PageServerLoad = async ({ url }) => {
  const p = url.searchParams;

  const filters: Record<string, any> = {};

  const status = p.get('status');
  if (status) filters.status = status;

  const company_id = p.get('company_id');
  if (company_id) filters.company_id = company_id;

  const serving_from = p.get('serving_from');
  if (serving_from) filters.serving_date_from = new Date(serving_from);

  const serving_to = p.get('serving_to');
  if (serving_to) filters.serving_date_to = new Date(serving_to);

  const shipment_from = p.get('shipment_from');
  if (shipment_from) filters.shipment_date_from = new Date(shipment_from);

  const shipment_to = p.get('shipment_to');
  if (shipment_to) filters.shipment_date_to = new Date(shipment_to);

  const order_source = p.get('order_source');
  if (order_source) filters.order_source = order_source;

  const frozen = p.get('frozen');
  if (frozen === 'true') filters.frozen = true;
  else if (frozen === 'false') filters.frozen = false;

  const chilled = p.get('chilled');
  if (chilled === 'true') filters.chilled = true;
  else if (chilled === 'false') filters.chilled = false;

  const pagination = parsePaginationParams(url);

  const [ordersResult, companies] = await Promise.all([
    getOrdersPaginated(pool, filters, pagination),
    getCompanies(pool, { includeInactive: false }),
  ]);

  return {
    orders: ordersResult.data,
    pagination: {
      page: ordersResult.page,
      perPage: ordersResult.perPage,
      total: ordersResult.total,
      totalPages: ordersResult.totalPages,
    },
    companies,
    filters: {
      status: status ?? '',
      company_id: company_id ?? '',
      serving_from: serving_from ?? '',
      serving_to: serving_to ?? '',
      shipment_from: shipment_from ?? '',
      shipment_to: shipment_to ?? '',
      order_source: order_source ?? '',
      frozen: frozen ?? '',
      chilled: chilled ?? '',
    },
  };
};

export const actions: Actions = {
  bulkStatus: async ({ request }) => {
    const form = await request.formData();
    const ids = form.getAll('order_id') as string[];
    const target = form.get('target_status') as string;

    if (ids.length === 0) {
      return fail(400, { action: 'bulkStatus', error: 'No orders selected' });
    }

    const validStatuses: OrderStatus[] = ['pending', 'confirmed', 'fulfilled', 'cancelled'];
    if (!validStatuses.includes(target as OrderStatus)) {
      return fail(400, { action: 'bulkStatus', error: 'Invalid target status' });
    }

    const updated = await setOrdersStatusBatch(pool, ids, target as OrderStatus);
    return { action: 'bulkStatus', success: true, count: updated.length, target };
  },
};
