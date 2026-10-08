import { fail, error } from '@sveltejs/kit';
import {
  getOrders,
  getOrderById,
  cancelOrder,
} from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';
import type { Actions, PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ locals }) => {
  const companyId = locals.user!.companyId!;
  const caller = callerFromLocals(locals);
  const orders = await getOrders({ company_id: companyId }, caller);
  return { orders };
};

export const actions: Actions = {
  cancel: async ({ request, locals }) => {
    const companyId = locals.user!.companyId!;
    const form = await request.formData();
    const id = form.get('order_id');
    if (!id || typeof id !== 'string') return fail(422, { action: 'cancel', error: 'Missing order ID' });

    const caller = callerFromLocals(locals);
    const order = await getOrderById(id, caller);
    if (!order) error(404, 'Order not found');
    if (order.company_id !== companyId) error(403, 'Forbidden');
    if (order.status !== 'pending') {
      return fail(422, { action: 'cancel', error: 'Only pending orders can be cancelled' });
    }

    await cancelOrder(id, caller);
    return { action: 'cancel', success: true };
  },
};

