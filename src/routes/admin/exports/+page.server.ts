import type { PageServerLoad } from './$types';

// No data needed — exports are pure download triggers.
// When backend routes are built, load any required filter
// options here (e.g. date pickers, company selectors).
export const load: PageServerLoad = async () => {
  return {};
};
