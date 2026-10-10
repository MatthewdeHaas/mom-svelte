-- =============================================================
-- Migration 006: Pricing fields for cost tracking & invoicing
--
-- Adds nullable cost/price columns to support financial features.
-- Seeded separately via scripts/seed-prices.ts.
-- =============================================================

ALTER TABLE ingredient ADD COLUMN IF NOT EXISTS cost_per_unit NUMERIC(10,4);
ALTER TABLE recipe ADD COLUMN IF NOT EXISTS price_per_meal NUMERIC(10,2);

COMMENT ON COLUMN ingredient.cost_per_unit IS 'Cost per unit of measurement (what MOM pays)';
COMMENT ON COLUMN recipe.price_per_meal IS 'Price charged to client per meal serving';
