import { z } from 'zod';

export const RecipeNameSchema = z.object({
  name: z.string().min(1, 'Recipe name is required').transform(v => v.trim()),
});

export const RecipeIngredientSchema = z.object({
  ingredient_id: z.string().min(1, 'Please select an ingredient'),
  amount: z.coerce.number({ error: 'Amount must be a positive number' }).gt(0, 'Amount must be a positive number'),
  unit: z.string().min(1, 'Unit is required').transform(v => v.trim()),
});

export type RecipeNameInput = z.output<typeof RecipeNameSchema>;
export type RecipeIngredientInput = z.output<typeof RecipeIngredientSchema>;
