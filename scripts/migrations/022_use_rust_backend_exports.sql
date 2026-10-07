-- 022_use_rust_backend_exports.sql
--
-- Per-domain flag for the exports layer (xlsx + pdf). When ON, the
-- SvelteKit route handlers in `src/routes/admin/exports/**` forward
-- generation to the Rust service; when OFF (default), the existing
-- TS pipeline (ExcelJS + pdfkit) runs locally. Flip
-- `enabled_global = TRUE` for a fleet-wide rollout.
--
-- Scoping — exports are admin-only (the SvelteKit routes already
-- gate on admin). The Rust handlers enforce it defensively
-- (`require_admin` → 403). Plural flag name because one toggle
-- covers all three exports (production-order xlsx, shopping-list
-- xlsx, packing-slips pdfs), matching the import side where
-- `use_rust_backend_imports` likewise covers preview + commit.

INSERT INTO feature_flag (name, description, enabled_global) VALUES
    ('use_rust_backend_exports', 'Route admin xlsx + pdf export generation through the Rust service.', FALSE)
ON CONFLICT (name) DO NOTHING;
