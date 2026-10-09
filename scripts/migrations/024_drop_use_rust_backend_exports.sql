-- 024_drop_use_rust_backend_exports.sql
--
-- The exports fall-through on the SvelteKit side has been deleted;
-- all export generation (production-order xlsx, shopping-list xlsx,
-- packing-slips PDF/ZIP) now runs in Rust unconditionally. The flag
-- has no readers left, so drop the registry row.
--
-- `feature_flag_company` overrides for this flag cascade-delete via
-- the FK `ON DELETE CASCADE` in migration 009.
DELETE FROM feature_flag WHERE name = 'use_rust_backend_exports';
