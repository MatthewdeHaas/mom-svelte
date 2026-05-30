-- =============================================================
-- Migration 004: Singular table names + merge domain users into "user"
--
-- Two goals:
--   1. The domain `users` table only added `company_id` on top of Better
--      Auth's `"user"` table. Add that column directly to `"user"` and
--      drop the redundant table.
--   2. Rename all plural tables to singular for consistency.
--      `orders` and `users` become "order" and "user" (reserved words,
--      require quoting in SQL).
-- =============================================================

-- -------------------------------------------------------------
-- STEP 1: Merge company_id into Better Auth's "user" table
-- -------------------------------------------------------------

ALTER TABLE "user"
  ADD COLUMN IF NOT EXISTS company_id UUID REFERENCES companies(id) ON DELETE SET NULL;

-- Copy any existing company associations across
UPDATE "user" u
SET    company_id = d.company_id
FROM   users d
WHERE  d.id = u.id
  AND  d.company_id IS NOT NULL;

-- -------------------------------------------------------------
-- STEP 2: Re-point orders.created_by from users → "user"
-- -------------------------------------------------------------

ALTER TABLE orders
  DROP CONSTRAINT IF EXISTS orders_created_by_fkey;

ALTER TABLE orders
  ADD CONSTRAINT orders_created_by_fkey
    FOREIGN KEY (created_by) REFERENCES "user"(id) ON DELETE SET NULL;

-- -------------------------------------------------------------
-- STEP 3: Drop the redundant domain users table
-- -------------------------------------------------------------

DROP TABLE IF EXISTS users;

-- -------------------------------------------------------------
-- STEP 4: Rename plural tables to singular
--   Compound join tables first, then their component tables,
--   to avoid any transient constraint confusion.
-- -------------------------------------------------------------

ALTER TABLE ingredient_allergens RENAME TO ingredient_allergen;
ALTER TABLE person_allergens     RENAME TO person_allergen;
ALTER TABLE recipe_ingredients   RENAME TO recipe_ingredient;
ALTER TABLE menu_recipes         RENAME TO menu_recipe;

ALTER TABLE allergens            RENAME TO allergen;
ALTER TABLE ingredients          RENAME TO ingredient;
ALTER TABLE recipes              RENAME TO recipe;
ALTER TABLE persons              RENAME TO person;
ALTER TABLE companies            RENAME TO company;
ALTER TABLE menus                RENAME TO menu;
ALTER TABLE orders               RENAME TO "order";

-- -------------------------------------------------------------
-- STEP 5: Rename indexes to match new table names
-- -------------------------------------------------------------

ALTER INDEX IF EXISTS idx_orders_company_id    RENAME TO idx_order_company_id;
ALTER INDEX IF EXISTS idx_orders_serving_date  RENAME TO idx_order_serving_date;
ALTER INDEX IF EXISTS idx_orders_shipment_date RENAME TO idx_order_shipment_date;
ALTER INDEX IF EXISTS idx_orders_recipe_id     RENAME TO idx_order_recipe_id;
ALTER INDEX IF EXISTS idx_menu_recipes_menu_id RENAME TO idx_menu_recipe_menu_id;
ALTER INDEX IF EXISTS idx_persons_company_id   RENAME TO idx_person_company_id;
-- idx_attendance_date is already singular — no rename needed
