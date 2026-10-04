-- 007: Align order table with new production sheet format
--
-- Changes:
--   1. Rename client_note → client (field now holds client name + allergen info)
--   2. Add portion_size (grams/units per serving)
--   3. Add day (production cycle day number, 1-7)

ALTER TABLE "order" RENAME COLUMN client_note TO client;

ALTER TABLE "order"
  ADD COLUMN portion_size NUMERIC(10, 2),
  ADD COLUMN day INTEGER CHECK (day >= 1 AND day <= 7);
