-- =============================================================
-- Migration 005: Invite tokens for client onboarding
--
-- Admin sends a tokenised invite link to a client user's email.
-- When the client clicks the link and authenticates, the token
-- is validated and their user.company_id is stamped automatically.
-- =============================================================

CREATE TABLE IF NOT EXISTS invite (
  id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
  company_id  UUID        NOT NULL REFERENCES company(id) ON DELETE CASCADE,
  email       TEXT        NOT NULL,
  token       TEXT        NOT NULL UNIQUE,
  expires_at  TIMESTAMPTZ NOT NULL,
  used_at     TIMESTAMPTZ,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_invite_token      ON invite(token);
CREATE INDEX IF NOT EXISTS idx_invite_company_id ON invite(company_id);

COMMENT ON TABLE invite IS 'One-time invite tokens sent by admins to onboard client users';
