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

export const load: PageServerLoad = async ({ params }) => {
  const order = await getOrderById(pool, params.id);
  if (!order) error(404, 'Order not found');
  return { order };
};

export const actions: Actions = {
  update: async ({ request, params }) => {
    const form = await request.formData();

    const amount          = parseFloat(form.get('amount')?.toString() ?? '');
    const meal_category   = form.get('meal_category')?.toString().trim() ?? '';
    const serving_date    = form.get('serving_date')?.toString() ?? '';
    const shipment_date   = form.get('shipment_date')?.toString() || null;
    const frozen          = form.get('frozen') === 'true';
    const chilled         = form.get('chilled') === 'true';
    const notes           = form.get('notes')?.toString().trim() || null;

    const errors: Record<string, string> = {};
    if (isNaN(amount) || amount <= 0) errors.amount = 'Amount must be a positive number';
    if (!meal_category) errors.meal_category = 'Meal category is required';
    if (!serving_date) errors.serving_date = 'Serving date is required';
    if (frozen && chilled) errors.frozen = 'An order cannot be both frozen and chilled';
    if (shipment_date && serving_date && new Date(shipment_date) > new Date(serving_date)) {
      errors.shipment_date = 'Shipment date must be on or before serving date';
    }

    if (Object.keys(errors).length) {
      return fail(422, {
        action: 'update',
        errors,
        values: { amount, meal_category, serving_date, shipment_date, frozen, chilled, notes },
      });
    }

    await updateOrder(pool, params.id, {
      amount,
      meal_category,
      serving_date: new Date(serving_date),
      shipment_date: shipment_date ? new Date(shipment_date) : null,
      frozen,
      chilled,
      notes,
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
