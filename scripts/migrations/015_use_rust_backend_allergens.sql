-- 015_use_rust_backend_allergens.sql
--
-- Per-domain flag for the allergens query layer (Track 3 Step 3,
-- domain #5). When ON, the SvelteKit `src/lib/server/queries/allergens.ts`
-- dispatcher forwards to `/api/allergens/*` on the Rust service; when
-- OFF (default), the existing pg SQL path runs. Flip
-- `enabled_global = TRUE` for a fleet-wide rollout, or insert a row
-- in `feature_flag_company` to roll out one client at a time.
--
-- Allergens are admin-only across the board — the Rust handlers reject
-- clients with 403 uniformly. The one client-facing read path today
-- (`src/routes/client/roster/[id]/+page.server.ts` → `getAllergens(pool)`)
-- stays on the SQL path in this commit — routes are untouched; threading
-- `caller` through that route is a follow-up with its own scope decision.

INSERT INTO feature_flag (name, description, enabled_global) VALUES
    ('use_rust_backend_allergens', 'Route allergens read/write queries through the Rust service.', FALSE)
ON CONFLICT (name) DO NOTHING;
