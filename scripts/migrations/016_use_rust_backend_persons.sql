-- 016_use_rust_backend_persons.sql
--
-- Per-domain flag for the persons + attendance query layer (Track 3
-- Step 3, domain #6). When ON, the SvelteKit
-- `src/lib/server/queries/persons.ts` dispatcher forwards to
-- `/api/persons/*` and `/api/attendance/*` on the Rust service; when
-- OFF (default), the existing pg SQL path runs. Flip
-- `enabled_global = TRUE` for a fleet-wide rollout, or insert a row
-- in `feature_flag_company` to roll out one client at a time.
--
-- Scoping differs from allergens/ingredients: persons are a client
-- surface (client/roster, client/attendance, client/menu), so the
-- Rust handlers enforce `person.company_id == auth.company_id` for
-- client callers and admins pass through. The "lie about existence"
-- rule applies — a client probing another company's person id gets
-- 404, not 403.

INSERT INTO feature_flag (name, description, enabled_global) VALUES
    ('use_rust_backend_persons', 'Route persons + attendance read/write queries through the Rust service.', FALSE)
ON CONFLICT (name) DO NOTHING;
