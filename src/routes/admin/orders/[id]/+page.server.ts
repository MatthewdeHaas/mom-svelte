import { fail, error } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import {
  getOrderById,
  updateOrder,
  confirmOrder,
  fulfillOrder,
  cancelOrder,
} from '$lib/server/queries/orders';
import { pool } from '$lib/server/db';
import { parseFormData } from '$lib/server/form';
import { UpdateOrderSchema } from '$lib/server/schemas/order';

export const load: PageServerLoad = async ({ params }) => {
  const order = await getOrderById(pool, params.id);
  if (!order) error(404, 'Order not found');
  return { order };
};

export const actions: Actions = {
  update: async ({ request, params }) => {
    const form = await request.formData();
    const parsed = parseFormData(UpdateOrderSchema, form);

    if (!parsed.success) {
      return fail(422, {
        action: 'update',
        errors: parsed.errors,
        values: Object.fromEntries(form),
      });
    }

    const { amount, meal_category, serving_date, shipment_date, frozen, chilled, notes } = parsed.data;

    await updateOrder(pool, params.id, {
      amount,
      meal_category,
      serving_date: new Date(serving_date),
      shipment_date: shipment_date ? new Date(shipment_date) : undefined,
      frozen,
      chilled,
      client_note: notes,
    });

    return { action: 'update', success: true };
  },

  confirm: async ({ params }) => {
    const order = await getOrderById(pool, params.id);
    if (!order) error(404, 'Order not found');
    if (order.status !== 'pending') {
      return fail(422, { action: 'confirm', error: 'Only pending orders can be confirmed' });
    }
    await confirmOrder(pool, params.id);
    return { action: 'confirm', success: true };
  },

  fulfill: async ({ params }) => {
    const order = await getOrderById(pool, params.id);
    if (!order) error(404, 'Order not found');
    if (order.status !== 'confirmed') {
      return fail(422, { action: 'fulfill', error: 'Only confirmed orders can be fulfilled' });
    }
    await fulfillOrder(pool, params.id);
    return { action: 'fulfill', success: true };
  },

  cancel: async ({ params }) => {
    const order = await getOrderById(pool, params.id);
    if (!order) error(404, 'Order not found');
    if (!['pending', 'confirmed'].includes(order.status)) {
      return fail(422, { action: 'cancel', error: 'Only pending or confirmed orders can be cancelled' });
    }
    await cancelOrder(pool, params.id);
    return { action: 'cancel', success: true };
  },
};
