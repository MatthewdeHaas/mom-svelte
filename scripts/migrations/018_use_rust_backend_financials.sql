-- 018_use_rust_backend_financials.sql
--
-- Per-domain flag for the financials query layer (Track 3 Step 3,
-- domain #8). When ON, the SvelteKit `src/lib/server/queries/financials.ts`
-- dispatcher forwards to `/api/financials/*` on the Rust service; when
-- OFF (default), the existing pg SQL path runs. Flip
-- `enabled_global = TRUE` for a fleet-wide rollout, or insert a row
-- in `feature_flag_company` to roll out one client at a time.
--
-- Financials are admin-only across the board — the one consumer
-- (`src/routes/admin/financials/+page.server.ts`) is behind the admin
-- guard, and every Rust handler hard-rejects client callers with 403.

INSERT INTO feature_flag (name, description, enabled_global) VALUES
    ('use_rust_backend_financials', 'Route financials read queries (recipe margins + invoices) through the Rust service.', FALSE)
ON CONFLICT (name) DO NOTHING;
