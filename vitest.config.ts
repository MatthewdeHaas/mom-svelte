import { defineConfig } from 'vitest/config';
import { resolve } from 'path';

export default defineConfig({
  test: {
    include: [resolve(__dirname, 'tests/**/*.test.ts')],
    alias: {
      '$lib': resolve(__dirname, './src/lib'),
    },
  },
});
