import { z } from 'zod';

export const IngredientSchema = z.object({
  name: z.string().min(1, 'Ingredient name is required').transform(v => v.trim()),
});

export type IngredientInput = z.output<typeof IngredientSchema>;
