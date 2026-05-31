import { fail, error } from '@sveltejs/kit';
import { z } from 'zod';
import { pool } from '$lib/server/db';
import { getOrders, getOrderById, cancelOrder } from '$lib/server/queries/orders';
import { parseFormData } from '$lib/server/form';
import type { Actions, PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ locals }) => {
  const companyId = locals.user!.companyId!;
  const orders = await getOrders(pool, { company_id: companyId });
  return { orders };
};

export const actions: Actions = {
  cancel: async ({ request, locals }) => {
    const companyId = locals.user!.companyId!;
    const form = await request.formData();
    const id = form.get('order_id');
    if (!id || typeof id !== 'string') return fail(422, { action: 'cancel', error: 'Missing order ID' });

    const order = await getOrderById(pool, id);
    if (!order) error(404, 'Order not found');
    if (order.company_id !== companyId) error(403, 'Forbidden');
    if (order.status !== 'pending') {
      return fail(422, { action: 'cancel', error: 'Only pending orders can be cancelled' });
    }

    await cancelOrder(pool, id);
    return { action: 'cancel', success: true };
  },
};

