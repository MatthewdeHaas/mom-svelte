-- 021_use_rust_backend_insights.sql
--
-- Per-domain flag for the insights aggregations layer. When ON, the
-- SvelteKit `src/lib/server/queries/insights.ts` dispatcher forwards
-- to `/api/insights/*` on the Rust service; when OFF (default), the
-- existing pg SQL path runs. Flip `enabled_global = TRUE` for a
-- fleet-wide rollout.
--
-- Scoping — insights is admin-only across the board. The 7 consumers
-- (6 `/admin/insights/api/*` routes + the admin landing KPI tile)
-- are already admin-gated at the SvelteKit layer; the Rust handlers
-- enforce it defensively (`require_admin` → 403). No client-facing
-- surface, so there is no per-company override pathway needed.

INSERT INTO feature_flag (name, description, enabled_global) VALUES
    ('use_rust_backend_insights', 'Route admin-insights aggregation queries through the Rust service.', FALSE)
ON CONFLICT (name) DO NOTHING;
