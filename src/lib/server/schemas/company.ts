import { z } from 'zod';

const nullableString = z
  .string()
  .transform(v => v.trim() || null)
  .nullable()
  .optional()
  .transform(v => v ?? null);

export const CompanySchema = z.object({
  name: z.string().min(1, 'Company name is required').transform(v => v.trim()),
  contact_email: nullableString,
  phone: nullableString,
  shipping_address: nullableString,
});

export type CompanyInput = z.output<typeof CompanySchema>;
