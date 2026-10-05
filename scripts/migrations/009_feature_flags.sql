-- 009_feature_flags.sql
--
-- Feature-flag registry consumed by the Rust service (`crates/flags`).
-- Node/SvelteKit does not read this directly — it asks the Rust API
-- which flags are enabled for the current (user, company). Keeping the
-- storage in Postgres (vs. a config file) means ops can toggle a flag
-- for a single client without a deploy.
--
-- Precedence at resolve time (see `PgFlagResolver::resolve`):
--   feature_flag_company.enabled   (per-company override)
--     > feature_flag.enabled_global (fleet-wide default)
--       > Flag::DEFAULT             (compile-time fallback)

CREATE TABLE IF NOT EXISTS feature_flag (
    name            TEXT PRIMARY KEY,
    description     TEXT NOT NULL DEFAULT '',
    enabled_global  BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS feature_flag_company (
    flag_name   TEXT        NOT NULL REFERENCES feature_flag(name) ON DELETE CASCADE,
    company_id  UUID        NOT NULL REFERENCES companies(id)      ON DELETE CASCADE,
    enabled     BOOLEAN     NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (flag_name, company_id)
);

-- Lookup is always keyed by (flag_name, company_id) — the PK covers
-- it. No extra index needed. The reverse index (all overrides for a
-- company) is only used by the admin UI's company-detail page; add
-- `CREATE INDEX ... ON feature_flag_company (company_id)` if that
-- page shows up slow.

-- Seed the five known flags with their compile-time defaults so
-- `/api/flags` returns something meaningful on a fresh DB. Keeping
-- the row count small — ops can toggle `enabled_global` directly.
INSERT INTO feature_flag (name, description, enabled_global) VALUES
    ('financials',           'Enable the financials section (invoices, recipe margins) for a company.', FALSE),
    ('insights',             'Enable the insights dashboard (KPIs, charts).',                           FALSE),
    ('allergens',            'Expose the allergens admin UI and allergen columns on orders.',           FALSE),
    ('client_self_ordering', 'Let client users place their own orders through the portal.',             FALSE),
    ('magic_link_auth',      'Show the magic-link login option alongside password login.',              FALSE)
ON CONFLICT (name) DO NOTHING;
