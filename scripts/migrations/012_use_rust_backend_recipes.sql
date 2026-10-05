-- 012_use_rust_backend_recipes.sql
--
-- Per-domain flag for the recipes query layer (Track 3 Step 3,
-- domain #2). When ON, the SvelteKit `src/lib/server/queries/recipes.ts`
-- dispatcher forwards to `/api/recipes/*` on the Rust service; when
-- OFF (default), the existing pg SQL path runs. Flip
-- `enabled_global = TRUE` for a fleet-wide rollout, or insert a row
-- in `feature_flag_company` to roll out one client at a time.
--
-- The catch-all `use_rust_backend_queries` was already retired by
-- migration 011; this migration adds no DELETE clause.

INSERT INTO feature_flag (name, description, enabled_global) VALUES
    ('use_rust_backend_recipes', 'Route recipes read/write queries through the Rust service.', FALSE)
ON CONFLICT (name) DO NOTHING;
