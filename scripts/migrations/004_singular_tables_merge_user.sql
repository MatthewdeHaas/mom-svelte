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
--
-- Idempotency: every rename is wrapped in a `to_regclass` guard so a
-- replay (nuke-and-replay, or dual-migrator re-run across the TS +
-- sqlx ledgers) is a no-op after the first apply. The lint doesn't
-- check renames, but the guards are the only way to make the file
-- actually safe to re-run.
-- =============================================================

-- -------------------------------------------------------------
-- STEP 1: Merge company_id into Better Auth's "user" table
-- -------------------------------------------------------------

ALTER TABLE "user"
  ADD COLUMN IF NOT EXISTS company_id UUID REFERENCES companies(id) ON DELETE SET NULL;

-- Copy any existing company associations across. Guarded on the source
-- table because step 3 drops it; a replay otherwise hits
-- `relation "users" does not exist`.
DO $$
BEGIN
  IF to_regclass('public.users') IS NOT NULL THEN
    UPDATE "user" u
    SET    company_id = d.company_id
    FROM   users d
    WHERE  d.id = u.id
      AND  d.company_id IS NOT NULL;
  END IF;
END $$;

-- -------------------------------------------------------------
-- STEP 2: Re-point orders.created_by from users → "user"
--
-- Guarded on the source table name — after step 4 renames `orders`
-- to `"order"`, a replay would otherwise fail on `relation "orders"
-- does not exist`.
-- -------------------------------------------------------------

DO $$
BEGIN
  IF to_regclass('public.orders') IS NOT NULL THEN
    ALTER TABLE orders DROP CONSTRAINT IF EXISTS orders_created_by_fkey;
    ALTER TABLE orders
      ADD CONSTRAINT orders_created_by_fkey
        FOREIGN KEY (created_by) REFERENCES "user"(id) ON DELETE SET NULL;
  END IF;
END $$;

-- -------------------------------------------------------------
-- STEP 3: Drop the redundant domain users table
-- -------------------------------------------------------------

DROP TABLE IF EXISTS users;

-- -------------------------------------------------------------
-- STEP 4: Rename plural tables to singular
--   Compound join tables first, then their component tables,
--   to avoid any transient constraint confusion.
--
-- Each rename is guarded: `to_regclass` on the source resolves to a
-- regclass OID only when the plural still exists AND the singular
-- doesn't. The combined check means a replay sees the renames as
-- already-applied and does nothing.
-- -------------------------------------------------------------

DO $$
BEGIN
  IF to_regclass('public.ingredient_allergens') IS NOT NULL
     AND to_regclass('public.ingredient_allergen') IS NULL THEN
    ALTER TABLE ingredient_allergens RENAME TO ingredient_allergen;
  END IF;

  IF to_regclass('public.person_allergens') IS NOT NULL
     AND to_regclass('public.person_allergen') IS NULL THEN
    ALTER TABLE person_allergens RENAME TO person_allergen;
  END IF;

  IF to_regclass('public.recipe_ingredients') IS NOT NULL
     AND to_regclass('public.recipe_ingredient') IS NULL THEN
    ALTER TABLE recipe_ingredients RENAME TO recipe_ingredient;
  END IF;

  IF to_regclass('public.menu_recipes') IS NOT NULL
     AND to_regclass('public.menu_recipe') IS NULL THEN
    ALTER TABLE menu_recipes RENAME TO menu_recipe;
  END IF;

  IF to_regclass('public.allergens') IS NOT NULL
     AND to_regclass('public.allergen') IS NULL THEN
    ALTER TABLE allergens RENAME TO allergen;
  END IF;

  IF to_regclass('public.ingredients') IS NOT NULL
     AND to_regclass('public.ingredient') IS NULL THEN
    ALTER TABLE ingredients RENAME TO ingredient;
  END IF;

  IF to_regclass('public.recipes') IS NOT NULL
     AND to_regclass('public.recipe') IS NULL THEN
    ALTER TABLE recipes RENAME TO recipe;
  END IF;

  IF to_regclass('public.persons') IS NOT NULL
     AND to_regclass('public.person') IS NULL THEN
    ALTER TABLE persons RENAME TO person;
  END IF;

  IF to_regclass('public.companies') IS NOT NULL
     AND to_regclass('public.company') IS NULL THEN
    ALTER TABLE companies RENAME TO company;
  END IF;

  IF to_regclass('public.menus') IS NOT NULL
     AND to_regclass('public.menu') IS NULL THEN
    ALTER TABLE menus RENAME TO menu;
  END IF;

  IF to_regclass('public.orders') IS NOT NULL
     AND to_regclass('public."order"') IS NULL THEN
    ALTER TABLE orders RENAME TO "order";
  END IF;
END $$;

-- -------------------------------------------------------------
-- STEP 5: Rename indexes to match new table names
--
-- `ALTER INDEX IF EXISTS` is already idempotent in Postgres — a
-- missing index is a silent no-op, so these need no further guard.
-- -------------------------------------------------------------

ALTER INDEX IF EXISTS idx_orders_company_id    RENAME TO idx_order_company_id;
ALTER INDEX IF EXISTS idx_orders_serving_date  RENAME TO idx_order_serving_date;
ALTER INDEX IF EXISTS idx_orders_shipment_date RENAME TO idx_order_shipment_date;
ALTER INDEX IF EXISTS idx_orders_recipe_id     RENAME TO idx_order_recipe_id;
ALTER INDEX IF EXISTS idx_menu_recipes_menu_id RENAME TO idx_menu_recipe_menu_id;
ALTER INDEX IF EXISTS idx_persons_company_id   RENAME TO idx_person_company_id;
-- idx_attendance_date is already singular — no rename needed
