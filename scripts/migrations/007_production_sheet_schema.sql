-- 007: Align order table with new production sheet format
--
-- Changes:
--   1. Rename client_note → client (field now holds client name + allergen info)
--   2. Add portion_size (grams/units per serving)
--   3. Add day (production cycle day number, 1-7)
--
-- Guarded for idempotency so a nuke-and-replay (or a dual-migrator
-- re-run across the TS + sqlx ledgers) doesn't fail on the rename.

DO $$
BEGIN
  IF EXISTS (
    SELECT 1
    FROM information_schema.columns
    WHERE table_name = 'order' AND column_name = 'client_note'
  ) AND NOT EXISTS (
    SELECT 1
    FROM information_schema.columns
    WHERE table_name = 'order' AND column_name = 'client'
  ) THEN
    EXECUTE 'ALTER TABLE "order" RENAME COLUMN client_note TO client';
  END IF;
END $$;

ALTER TABLE "order"
  ADD COLUMN IF NOT EXISTS portion_size NUMERIC(10, 2),
  ADD COLUMN IF NOT EXISTS day INTEGER CHECK (day >= 1 AND day <= 7);
