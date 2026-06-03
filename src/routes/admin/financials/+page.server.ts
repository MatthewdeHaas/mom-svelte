import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { getRecipesWithMarginsPaginated, getInvoiceData } from '$lib/server/queries/financials';
import { getCompanies } from '$lib/server/queries/companies';
import { parsePaginationParams } from '$lib/server/queries/pagination';
import { pool } from '$lib/server/db';

export const load: PageServerLoad = async ({ url }) => {
  const pagination = parsePaginationParams(url, { perPage: 25 });

  const [marginsResult, companies] = await Promise.all([
    getRecipesWithMarginsPaginated(pool, pagination),
    getCompanies(pool, { includeInactive: false }),
  ]);

  return {
    margins: marginsResult.data,
    pagination: {
      page: marginsResult.page,
      perPage: marginsResult.perPage,
      total: marginsResult.total,
      totalPages: marginsResult.totalPages,
    },
    companies,
  };
};

export const actions: Actions = {
  invoice: async ({ request }) => {
    const form = await request.formData();
    const company_id = form.get('company_id') as string;
    const from = form.get('from') as string;
    const to = form.get('to') as string;

    if (!company_id || !from || !to) {
      return fail(400, { action: 'invoice', error: 'Company and date range are required' });
    }

    const invoice = await getInvoiceData(pool, company_id, from, to);
    if (!invoice) {
      return fail(404, { action: 'invoice', error: 'Company not found' });
    }

    return { action: 'invoice', success: true, invoice, from, to };
  },
};
