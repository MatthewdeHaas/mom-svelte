import { defineConfig } from 'vitest/config';
import { resolve } from 'path';

export default defineConfig({
  test: {
    include: [resolve(__dirname, 'tests/**/*.test.ts')],
    alias: {
      '$lib': resolve(__dirname, './src/lib'),
      // SvelteKit virtual module — vitest can't resolve it, so point at a
      // stub that proxies `process.env`. Needed by the `_rust-dispatch`
      // dispatcher's lazy load of `api-client.ts`, which imports this at
      // module-load time.
      '$env/dynamic/private': resolve(
        __dirname,
        './tests/_stubs/env-dynamic-private.ts',
      ),
    },
  },
});
