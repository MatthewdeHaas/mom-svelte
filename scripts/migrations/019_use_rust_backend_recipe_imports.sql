-- 019_use_rust_backend_recipe_imports.sql
--
-- Per-domain flag for the recipe-imports pipeline (Track 3 Step 3,
-- domain #9). When ON, the SvelteKit `/admin/recipes/new` import
-- action forwards the multipart upload to the Rust service's
-- `/api/recipe-imports/{preview,commit}` endpoints; when OFF (default),
-- the existing ExcelJS + pg SQL path runs. Flip `enabled_global = TRUE`
-- for a fleet-wide rollout, or insert a row in `feature_flag_company`
-- to roll out one admin company at a time.
--
-- Recipe imports are admin-only on both the TS route and the Rust
-- handler. There is no client surface — recipes are global master data.
-- Unlike the other eight domains, the dispatcher cannot forward from
-- inside the query module: the Rust `commit` endpoint consumes a
-- multipart body (the raw .xlsx), not a JSON row list. The route-level
-- `+page.server.ts` is the forwarding point.

INSERT INTO feature_flag (name, description, enabled_global) VALUES
    ('use_rust_backend_recipe_imports', 'Route the recipe xlsx import pipeline (parse + fuzzy + commit) through the Rust service.', FALSE)
ON CONFLICT (name) DO NOTHING;
