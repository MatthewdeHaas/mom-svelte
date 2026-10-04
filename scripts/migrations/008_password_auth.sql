-- Track 1b: enable email+password auth alongside dormant magic-link.
--
-- Better Auth's initial schema (migration 001) already provisioned a
-- nullable `password` column on the `account` table. This migration is
-- an idempotent sanity check so a fresh DB always has the column
-- regardless of how 001 was applied, and so the intent is documented
-- in the migrations ledger.
--
-- Password hashes land in `account(provider_id='credential', password)`
-- when `emailAndPassword.enabled` is turned on in `auth.ts`.

ALTER TABLE "account" ADD COLUMN IF NOT EXISTS "password" text;
