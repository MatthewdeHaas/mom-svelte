// Help page is static — layout's auth guard
// (admin-operations-svelte/src/routes/admin/+layout.server.ts) already
// enforces the admin role on every /admin/** route. This file exists
// for shape-parity with other admin pages and so a future load() has
// somewhere to go without a new +page.server.ts drive-by.
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async () => {
  return {};
};
