-- 011_use_rust_backend_orders.sql
--
-- Per-domain flag for the orders query layer (Track 3 Step 3,
-- domain #1). When ON, the SvelteKit `src/lib/server/queries/orders.ts`
-- dispatcher forwards to `/api/orders/*` on the Rust service; when
-- OFF (default), the existing pg SQL path runs. Flip
-- `enabled_global = TRUE` for a fleet-wide rollout, or insert a row
-- in `feature_flag_company` to roll out one client at a time.
--
-- Also retires the `use_rust_backend_queries` catch-all seeded in
-- migration 010 — per-domain flags supersede it. Follow-up migrations
-- will add `use_rust_backend_recipes`, `use_rust_backend_companies`,
-- etc. as each domain's port lands.

INSERT INTO feature_flag (name, description, enabled_global) VALUES
    ('use_rust_backend_orders', 'Route orders read/write queries through the Rust service.', FALSE)
ON CONFLICT (name) DO NOTHING;

DELETE FROM feature_flag WHERE name = 'use_rust_backend_queries';
