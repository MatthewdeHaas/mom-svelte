-- 020_use_rust_backend_menus.sql
--
-- Per-domain flag for the menus + menu_recipe query layer. When ON,
-- the SvelteKit `src/lib/server/queries/menus.ts` dispatcher forwards
-- to `/api/menus/*` on the Rust service; when OFF (default), the
-- existing pg SQL path runs. Flip `enabled_global = TRUE` for a
-- fleet-wide rollout, or insert a row in `feature_flag_company` to
-- roll out one client at a time.
--
-- Scoping — menus are GLOBAL master data (no `menu.company_id`),
-- so unlike persons/orders there is no row-level company scoping.
-- Reads (`list`, `get_by_id`, `with_recipes`, `active_for_date`,
-- `get_menu_recipes`, `get_slot`) are open to admin + client. Every
-- mutation (create/update/deactivate + slot writes) is admin-only;
-- client callers get 403. The Rust handlers enforce this split.

INSERT INTO feature_flag (name, description, enabled_global) VALUES
    ('use_rust_backend_menus', 'Route menu + menu_recipe read/write queries through the Rust service.', FALSE)
ON CONFLICT (name) DO NOTHING;
