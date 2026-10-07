// Vitest stub for `$env/dynamic/private`.
//
// SvelteKit's `$env/dynamic/private` is a Vite virtual module that proxies
// `process.env` at runtime. Vitest's resolver doesn't know about SvelteKit's
// virtual modules, so any transitive import of this specifier blows up with
// `ERR_MODULE_NOT_FOUND`. This stub re-exports the same shape backed directly
// by `process.env` — functionally identical in Node tests.
//
// Aliased in `vitest.config.ts`.
export const env: Record<string, string | undefined> = new Proxy(
	{},
	{
		get(_, key: string) {
			return process.env[key];
		},
		has(_, key: string) {
			return key in process.env;
		},
	},
) as Record<string, string | undefined>;
