import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import {
  getRecipesWithMarginsPaginated,
  getInvoiceData,
  getCompanies,
} from '$lib/server/queries/_rust-dispatch';
import { callerFromLocals } from '$lib/server/api-client';
import { parsePaginationParams } from '$lib/server/queries/pagination';

export const load: PageServerLoad = async ({ url, locals }) => {
  const caller = callerFromLocals(locals);
  const pagination = parsePaginationParams(url, { perPage: 25 });

  const [marginsResult, companies] = await Promise.all([
    getRecipesWithMarginsPaginated(pagination, caller),
    getCompanies({ includeInactive: false }, caller),
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
  invoice: async ({ request, locals }) => {
    const caller = callerFromLocals(locals);
    const form = await request.formData();
    const company_id = form.get('company_id') as string;
    const from = form.get('from') as string;
    const to = form.get('to') as string;

    if (!company_id || !from || !to) {
      return fail(400, { action: 'invoice', error: 'Company and date range are required' });
    }

    const invoice = await getInvoiceData(company_id, from, to, caller);
    if (!invoice) {
      return fail(404, { action: 'invoice', error: 'Company not found' });
    }

    return { action: 'invoice', success: true, invoice, from, to };
  },
};
