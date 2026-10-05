-- 010_rust_backend_flags.sql
--
-- Seed the per-domain rollout flags for the Rust backend migration
-- (Track 3). Each flag gates the SvelteKit → Rust forward for one
-- slice of the business logic:
--
--   use_rust_backend_imports   → /admin/orders/import  (this commit)
--   use_rust_backend_queries   → read queries          (step 3)
--   use_rust_backend_exports   → PDF / xlsx exports    (step 4)
--   use_rust_backend_insights  → insights / KPIs       (step 5)
--
-- All default to FALSE. Flip `enabled_global = TRUE` to roll a slice
-- out to the whole fleet, or insert a row in `feature_flag_company`
-- to roll it out to one client at a time. The TS implementation stays
-- live for a parity window — don't delete it when the flag flips.

INSERT INTO feature_flag (name, description, enabled_global) VALUES
    ('use_rust_backend_imports',  'Route Excel imports through the Rust service.',            FALSE),
    ('use_rust_backend_queries',  'Route read queries through the Rust service.',             FALSE),
    ('use_rust_backend_exports',  'Route PDF / xlsx exports through the Rust service.',       FALSE),
    ('use_rust_backend_insights', 'Route insights / KPI aggregations through the Rust service.', FALSE)
ON CONFLICT (name) DO NOTHING;
