-- =============================================================
-- Meals on the Move — PostgreSQL Schema
-- =============================================================

-- Enable UUID generation
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- =============================================================
-- COMPANIES
-- Separate from users. A company can have multiple users.
-- Attributes like address/phone live here, not on the user.
-- =============================================================
CREATE TABLE companies (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name            TEXT UNIQUE NOT NULL,
    contact_email   TEXT,
    phone           TEXT,
    shipping_address TEXT,
    active          BOOLEAN NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- =============================================================
-- USERS
-- Owned by Better Auth — do not manage passwords here.
-- We extend with company_id and role.
-- =============================================================
CREATE TABLE users (
    id              TEXT PRIMARY KEY,         -- Better Auth issues string UUIDs
    company_id      UUID REFERENCES companies(id) ON DELETE SET NULL,
    email           TEXT UNIQUE NOT NULL,
    role            TEXT NOT NULL DEFAULT 'admin' CHECK (role IN ('admin', 'client')),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- =============================================================
-- ALLERGENS
-- Master list of allergens (gluten, dairy, nuts, etc.)
-- =============================================================
CREATE TABLE allergens (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name            TEXT UNIQUE NOT NULL
);

-- =============================================================
-- INGREDIENTS
-- =============================================================
CREATE TABLE ingredients (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name            TEXT UNIQUE NOT NULL,
    active          BOOLEAN NOT NULL DEFAULT TRUE
);

-- Many-to-many: ingredients <-> allergens
CREATE TABLE ingredient_allergens (
    ingredient_id   UUID NOT NULL REFERENCES ingredients(id) ON DELETE CASCADE,
    allergen_id     UUID NOT NULL REFERENCES allergens(id) ON DELETE CASCADE,
    PRIMARY KEY (ingredient_id, allergen_id)
);

-- =============================================================
-- RECIPES
-- Soft-delete via active flag — never hard delete a recipe
-- that may be referenced by historical orders.
-- =============================================================
CREATE TABLE recipes (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name            TEXT UNIQUE NOT NULL,
    active          BOOLEAN NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Many-to-many: recipes <-> ingredients (with amount/unit)
CREATE TABLE recipe_ingredients (
    recipe_id       UUID NOT NULL REFERENCES recipes(id) ON DELETE CASCADE,
    ingredient_id   UUID NOT NULL REFERENCES ingredients(id) ON DELETE RESTRICT,
    amount          NUMERIC(10, 3) NOT NULL CHECK (amount > 0),
    unit            TEXT NOT NULL,
    PRIMARY KEY (recipe_id, ingredient_id)
);

-- =============================================================
-- MENUS
-- A menu is active for a date range and cycles every 4 weeks.
-- Updated roughly every 6 months.
-- =============================================================
CREATE TABLE menus (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name            TEXT UNIQUE NOT NULL,
    is_active       BOOLEAN NOT NULL DEFAULT TRUE,
    start_date      DATE NOT NULL,
    end_date        DATE NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT valid_date_range CHECK (end_date > start_date)
);

-- Menu <-> Recipe assignments across the 4-week cycle
-- cycle_week: 1–4
-- day_of_week: 'monday'–'friday'
-- meal_category: e.g. 'breakfast', 'lunch', 'dinner'
CREATE TABLE menu_recipes (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    menu_id         UUID NOT NULL REFERENCES menus(id) ON DELETE CASCADE,
    recipe_id       UUID NOT NULL REFERENCES recipes(id) ON DELETE RESTRICT,
    cycle_week      SMALLINT NOT NULL CHECK (cycle_week BETWEEN 1 AND 4),
    day_of_week     TEXT NOT NULL CHECK (day_of_week IN ('monday','tuesday','wednesday','thursday','friday','saturday','sunday')),
    meal_category   TEXT NOT NULL,
    -- One recipe per slot per menu
    UNIQUE (menu_id, cycle_week, day_of_week, meal_category)
);

-- =============================================================
-- PERSONS
-- Individual people tracked per company (for headcount/allergens).
-- =============================================================
CREATE TABLE persons (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id      UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    active          BOOLEAN NOT NULL DEFAULT TRUE
);

-- Many-to-many: persons <-> allergens
CREATE TABLE person_allergens (
    person_id       UUID NOT NULL REFERENCES persons(id) ON DELETE CASCADE,
    allergen_id     UUID NOT NULL REFERENCES allergens(id) ON DELETE CASCADE,
    PRIMARY KEY (person_id, allergen_id)
);

-- Attendance per person per serving date
CREATE TABLE attendance (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    person_id       UUID NOT NULL REFERENCES persons(id) ON DELETE CASCADE,
    serving_date    DATE NOT NULL,
    attending       BOOLEAN NOT NULL DEFAULT FALSE,
    UNIQUE (person_id, serving_date)
);

-- =============================================================
-- ORDERS
-- One row per recipe per company per serving date.
-- order_source tracks how the order was created.
-- recipe_id is RESTRICT — soft-delete recipes instead.
-- =============================================================
CREATE TABLE orders (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id          UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    recipe_id           UUID REFERENCES recipes(id) ON DELETE RESTRICT,
    created_by          TEXT REFERENCES users(id) ON DELETE SET NULL,
    order_source        TEXT NOT NULL DEFAULT 'admin_manual'
                            CHECK (order_source IN ('admin_manual', 'excel_import', 'client_portal')),
    status              TEXT NOT NULL DEFAULT 'pending'
                            CHECK (status IN ('pending', 'confirmed', 'fulfilled', 'cancelled')),
    meal_category       TEXT,
    amount              NUMERIC(10, 2) NOT NULL CHECK (amount > 0),
    serving_date        DATE NOT NULL,
    shipment_date       DATE NOT NULL,
    frozen              BOOLEAN NOT NULL DEFAULT FALSE,
    chilled             BOOLEAN NOT NULL DEFAULT FALSE,
    recurring           BOOLEAN NOT NULL DEFAULT FALSE,
    client_note         TEXT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT shipment_before_serving CHECK (shipment_date <= serving_date)
);

-- =============================================================
-- INDEXES
-- =============================================================
CREATE INDEX idx_orders_company_id     ON orders(company_id);
CREATE INDEX idx_orders_serving_date   ON orders(serving_date);
CREATE INDEX idx_orders_shipment_date  ON orders(shipment_date);
CREATE INDEX idx_orders_recipe_id      ON orders(recipe_id);
CREATE INDEX idx_menu_recipes_menu_id  ON menu_recipes(menu_id);
CREATE INDEX idx_attendance_date       ON attendance(serving_date);
CREATE INDEX idx_persons_company_id    ON persons(company_id);
