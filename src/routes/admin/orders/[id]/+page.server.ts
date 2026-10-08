import { fail, error } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import {
  getOrderById,
  updateOrder,
  confirmOrder,
  fulfillOrder,
  cancelOrder,
} from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';
import { parseFormData } from '$lib/server/form';
import { UpdateOrderSchema } from '$lib/server/schemas/order';

export const load: PageServerLoad = async ({ params, locals }) => {
  const caller = callerFromLocals(locals);
  const order = await getOrderById(params.id, caller);
  if (!order) error(404, 'Order not found');
  return { order };
};

export const actions: Actions = {
  update: async ({ request, params, locals }) => {
    const form = await request.formData();
    const parsed = parseFormData(UpdateOrderSchema, form);

    if (!parsed.success) {
      return fail(422, {
        action: 'update',
        errors: parsed.errors,
        values: Object.fromEntries(form),
      });
    }

    const { amount, portion_size, day, meal_category, serving_date, shipment_date, frozen, chilled, client } = parsed.data;

    const caller = callerFromLocals(locals);
    await updateOrder(
      params.id,
      {
        amount,
        portion_size,
        day,
        meal_category,
        serving_date: new Date(serving_date),
        shipment_date: shipment_date ? new Date(shipment_date) : undefined,
        frozen,
        chilled,
        client,
      },
      caller,
    );

    return { action: 'update', success: true };
  },

  confirm: async ({ params, locals }) => {
    const caller = callerFromLocals(locals);
    const order = await getOrderById(params.id, caller);
    if (!order) error(404, 'Order not found');
    if (order.status !== 'pending') {
      return fail(422, { action: 'confirm', error: 'Only pending orders can be confirmed' });
    }
    await confirmOrder(params.id, caller);
    return { action: 'confirm', success: true };
  },

  fulfill: async ({ params, locals }) => {
    const caller = callerFromLocals(locals);
    const order = await getOrderById(params.id, caller);
    if (!order) error(404, 'Order not found');
    if (order.status !== 'confirmed') {
      return fail(422, { action: 'fulfill', error: 'Only confirmed orders can be fulfilled' });
    }
    await fulfillOrder(params.id, caller);
    return { action: 'fulfill', success: true };
  },

  cancel: async ({ params, locals }) => {
    const caller = callerFromLocals(locals);
    const order = await getOrderById(params.id, caller);
    if (!order) error(404, 'Order not found');
    if (!['pending', 'confirmed'].includes(order.status)) {
      return fail(422, { action: 'cancel', error: 'Only pending or confirmed orders can be cancelled' });
    }
    await cancelOrder(params.id, caller);
    return { action: 'cancel', success: true };
  },
};
