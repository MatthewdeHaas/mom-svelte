-- 017_use_rust_backend_invites.sql
--
-- Per-domain flag for the invites query layer (Track 3 Step 3,
-- domain #7). When ON, the SvelteKit `src/lib/server/queries/invites.ts`
-- dispatcher forwards to `/api/invites/*` on the Rust service; when
-- OFF (default), the existing pg SQL path runs. Flip
-- `enabled_global = TRUE` for a fleet-wide rollout, or insert a row
-- in `feature_flag_company` to roll out one client at a time.
--
-- Invites are admin-only across the board on the Rust side — the one
-- unauthenticated TS consumer (`src/routes/invite/[token]/+page.server.ts`,
-- the token-acceptance landing page) keeps calling the SQL path in this
-- commit. Routes are untouched; threading `caller` through the public
-- invite-acceptance route is a follow-up with its own scope decision.

INSERT INTO feature_flag (name, description, enabled_global) VALUES
    ('use_rust_backend_invites', 'Route invites read/write queries through the Rust service.', FALSE)
ON CONFLICT (name) DO NOTHING;
