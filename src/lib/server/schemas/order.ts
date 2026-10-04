import { z } from 'zod';

export const UpdateOrderSchema = z
  .object({
    amount: z.coerce
      .number({ error: 'Amount must be a positive number' })
      .gt(0, 'Amount must be a positive number'),
    portion_size: z.coerce
      .number()
      .positive('Portion size must be positive')
      .optional()
      .nullable()
      .transform(v => (v === undefined || isNaN(v as number)) ? null : v),
    day: z.coerce
      .number()
      .int('Day must be a whole number')
      .min(1, 'Day must be between 1 and 7')
      .max(7, 'Day must be between 1 and 7')
      .optional()
      .nullable()
      .transform(v => (v === undefined || isNaN(v as number)) ? null : v),
    meal_category: z.string().min(1, 'Meal category is required').transform(v => v.trim()),
    serving_date: z.string().min(1, 'Serving date is required'),
    shipment_date: z
      .string()
      .transform(v => v.trim() || null)
      .nullable()
      .optional()
      .transform(v => v ?? null),
    // FormData sends 'true'/'false' strings for these hidden inputs
    frozen: z
      .string()
      .transform(v => v === 'true')
      .pipe(z.boolean()),
    chilled: z
      .string()
      .transform(v => v === 'true')
      .pipe(z.boolean()),
    client: z
      .string()
      .transform(v => v.trim() || null)
      .nullable()
      .optional()
      .transform(v => v ?? null),
  })
  .superRefine((d, ctx) => {
    if (d.frozen && d.chilled) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['frozen'],
        message: 'An order cannot be both frozen and chilled',
      });
    }
    if (
      d.shipment_date &&
      d.serving_date &&
      new Date(d.shipment_date) > new Date(d.serving_date)
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['shipment_date'],
        message: 'Shipment date must be on or before serving date',
      });
    }
  });

export const ExportDateSchema = z.object({
  shipment_date: z
    .string()
    .regex(/^\d{4}-\d{2}-\d{2}$/, 'Invalid or missing shipment date.'),
});

export type UpdateOrderInput = z.output<typeof UpdateOrderSchema>;
export type ExportDateInput = z.output<typeof ExportDateSchema>;
