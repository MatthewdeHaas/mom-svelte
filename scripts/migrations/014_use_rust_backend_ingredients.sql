-- 014_use_rust_backend_ingredients.sql
--
-- Per-domain flag for the ingredients query layer (Track 3 Step 3,
-- domain #4). When ON, the SvelteKit `src/lib/server/queries/ingredients.ts`
-- dispatcher forwards to `/api/ingredients/*` on the Rust service; when
-- OFF (default), the existing pg SQL path runs. Flip
-- `enabled_global = TRUE` for a fleet-wide rollout, or insert a row
-- in `feature_flag_company` to roll out one client at a time.
--
-- Ingredients are admin-only across the board — the Rust handlers
-- reject clients with 403 uniformly (unlike companies, which "lie
-- about existence" with 404 for cross-company reads).

INSERT INTO feature_flag (name, description, enabled_global) VALUES
    ('use_rust_backend_ingredients', 'Route ingredients read/write queries through the Rust service.', FALSE)
ON CONFLICT (name) DO NOTHING;
