import { z } from 'zod';

const nullableDate = z
  .string()
  .transform(v => v.trim() || null)
  .nullable()
  .optional()
  .transform(v => v ?? null);

export const MenuSchema = z
  .object({
    name: z.string().min(1, 'Menu name is required').transform(v => v.trim()),
    start_date: nullableDate,
    end_date: nullableDate,
  })
  .superRefine((d, ctx) => {
    if (d.start_date && d.end_date && new Date(d.start_date) > new Date(d.end_date)) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['end_date'],
        message: 'End date must be on or after start date',
      });
    }
  });

export const MenuSlotSchema = z.object({
  cycle_week: z.coerce
    .number({ error: 'Invalid cycle week' })
    .int()
    .min(1)
    .max(4),
  day_of_week: z.string().min(1, 'Day of week is required'),
  meal_category: z.string().min(1, 'Meal category is required'),
  recipe_id: z.string().min(1, 'Recipe is required'),
});

export const MenuClearSlotSchema = z.object({
  cycle_week: z.coerce
    .number({ error: 'Invalid cycle week' })
    .int()
    .min(1)
    .max(4),
  day_of_week: z.string().min(1, 'Day of week is required'),
  meal_category: z.string().min(1, 'Meal category is required'),
});

export type MenuInput = z.output<typeof MenuSchema>;
export type MenuSlotInput = z.output<typeof MenuSlotSchema>;
export type MenuClearSlotInput = z.output<typeof MenuClearSlotSchema>;
