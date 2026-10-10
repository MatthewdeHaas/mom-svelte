-- Better Auth admin plugin schema. Re-authored (not regenerated) to be
-- idempotent — ADD COLUMN IF NOT EXISTS so a nuke-and-replay or a
-- double-run across two migrators doesn't blow up. Shape matches what
-- `pnpm run auth:schema` emits for the admin plugin.

alter table "user" add column if not exists "role" text;

alter table "user" add column if not exists "banned" boolean;

alter table "user" add column if not exists "banReason" text;

alter table "user" add column if not exists "banExpires" timestamptz;

alter table "session" add column if not exists "impersonatedBy" text;
