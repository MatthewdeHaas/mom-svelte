// =============================================================
// Meals on the Move — Domain Types
// Derived directly from the PostgreSQL schema.
//
// Conventions:
//   - Base types mirror table rows exactly (what pg returns)
//   - WithX suffixes add joined relations for query results
//   - Insert/Update types omit server-generated fields
// =============================================================

// -------------------------------------------------------------
// PRIMITIVES
// -------------------------------------------------------------

export type OrderSource = 'admin_manual' | 'excel_import' | 'client_portal';
export type OrderStatus = 'pending' | 'confirmed' | 'fulfilled' | 'cancelled';
export type DayOfWeek = 'monday' | 'tuesday' | 'wednesday' | 'thursday' | 'friday' | 'saturday' | 'sunday';
export type UserRole = 'admin' | 'client';

// -------------------------------------------------------------
// ALLERGEN
// -------------------------------------------------------------

export interface Allergen {
  id: string;
  name: string;
}

export type InsertAllergen = Omit<Allergen, 'id'>;

// -------------------------------------------------------------
// COMPANY
// -------------------------------------------------------------

export interface Company {
  id: string;
  name: string;
  contact_email: string | null;
  phone: string | null;
  shipping_address: string | null;
  active: boolean;
  created_at: Date;
}

export type InsertCompany = Omit<Company, 'id' | 'created_at'>;
export type UpdateCompany = Partial<InsertCompany>;

// -------------------------------------------------------------
// USER
// -------------------------------------------------------------

export interface User {
  id: string;           // Better Auth string UUID
  company_id: string | null;
  email: string;
  role: UserRole;
  created_at: Date;
}

// Users are owned by Better Auth — no InsertUser type.
// Mutations go through Better Auth APIs, not raw SQL.

export interface UserWithCompany extends User {
  company: Company | null;
}

// -------------------------------------------------------------
// INGREDIENT
// -------------------------------------------------------------

export interface Ingredient {
  id: string;
  name: string;
  active: boolean;
}

export type InsertIngredient = Omit<Ingredient, 'id'>;
export type UpdateIngredient = Partial<InsertIngredient>;

export interface IngredientWithAllergens extends Ingredient {
  allergens: Allergen[];
}

// -------------------------------------------------------------
// RECIPE
// -------------------------------------------------------------

export interface Recipe {
  id: string;
  name: string;
  active: boolean;
  created_at: Date;
}

export type InsertRecipe = Omit<Recipe, 'id' | 'created_at'>;
export type UpdateRecipe = Partial<InsertRecipe>;

// recipe_ingredients join row — used internally by query layer
export interface RecipeIngredientRow {
  recipe_id: string;
  ingredient_id: string;
  amount: number;
  unit: string;
}

// What the ingredient looks like when embedded in a recipe
export interface RecipeIngredient {
  ingredient_id: string;
  name: string;
  amount: number;
  unit: string;
  allergens: Allergen[];
}

export interface RecipeWithIngredients extends Recipe {
  ingredients: RecipeIngredient[];
}

// -------------------------------------------------------------
// MENU
// -------------------------------------------------------------

export interface Menu {
  id: string;
  name: string;
  is_active: boolean;
  start_date: Date;
  end_date: Date;
  created_at: Date;
}

export type InsertMenu = Omit<Menu, 'id' | 'created_at'>;
export type UpdateMenu = Partial<InsertMenu>;

export interface MenuRecipe {
  id: string;
  menu_id: string;
  recipe_id: string;
  cycle_week: 1 | 2 | 3 | 4;
  day_of_week: DayOfWeek;
  meal_category: string;
}

export type InsertMenuRecipe = Omit<MenuRecipe, 'id'>;

// Slot with recipe name denormalised — useful for display
export interface MenuRecipeWithName extends MenuRecipe {
  recipe_name: string;
  recipe_active: boolean;
}

export interface MenuWithRecipes extends Menu {
  recipes: MenuRecipeWithName[];
}

// -------------------------------------------------------------
// PERSON
// -------------------------------------------------------------

export interface Person {
  id: string;
  company_id: string;
  name: string;
  active: boolean;
}

export type InsertPerson = Omit<Person, 'id'>;
export type UpdatePerson = Partial<InsertPerson>;

export interface PersonWithAllergens extends Person {
  allergens: Allergen[];
}

// -------------------------------------------------------------
// ATTENDANCE
// -------------------------------------------------------------

export interface Attendance {
  id: string;
  person_id: string;
  serving_date: Date;
  attending: boolean;
}

export type UpsertAttendance = Omit<Attendance, 'id'>;

// Useful for building headcount views
export interface AttendanceWithPerson extends Attendance {
  person: PersonWithAllergens;
}

// -------------------------------------------------------------
// ORDER
// -------------------------------------------------------------

export interface Order {
  id: string;
  company_id: string;
  recipe_id: string | null;
  created_by: string | null;
  order_source: OrderSource;
  status: OrderStatus;
  meal_category: string | null;
  amount: number;
  serving_date: Date;
  shipment_date: Date;
  frozen: boolean;
  chilled: boolean;
  recurring: boolean;
  client_note: string | null;
  created_at: Date;
}

export type InsertOrder = Omit<Order, 'id' | 'created_at'>;
export type UpdateOrder = Partial<Omit<InsertOrder, 'order_source' | 'created_by'>>;

// For list views — joins just enough to avoid N+1 lookups
export interface OrderSummary extends Order {
  company_name: string;
  recipe_name: string | null;
}

// For detail view — full relations
export interface OrderDetail extends Order {
  company: Company;
  recipe: Recipe | null;
  created_by_email: string | null;
}

// -------------------------------------------------------------
// EXCEL IMPORT
// -------------------------------------------------------------

// Raw shape off the sheet after column mapping, before validation.
// All fields are unknown until Zod parses them.
export interface ExcelRowRaw {
  company: unknown;
  recipe: unknown;
  serving_date: unknown;
  shipment_date: unknown;
  meal_category: unknown;
  amount: unknown;
  storage: unknown;
  notes: unknown;
}

// Validated, parsed row — ready for DB reference lookups.
export interface ExcelRowParsed {
  company: string;
  recipe: string;
  serving_date: string;   // YYYY-MM-DD
  shipment_date: string;  // YYYY-MM-DD
  meal_category: string;
  amount: number;
  storage: 'frozen' | 'chilled' | null;
  notes: string | null;
}

// A row that has passed both Zod validation and DB reference checks.
// company/recipe resolved to their UUIDs — ready for INSERT.
export interface ExcelRowResolved {
  company_id: string;
  recipe_id: string;
  serving_date: string;
  shipment_date: string;
  meal_category: string;
  amount: number;
  frozen: boolean;
  chilled: boolean;
  client_note: string | null;
}

// Result returned to the caller after an import attempt
export interface ImportResult {
  inserted: number;
  skipped: number;
  errors: ImportRowError[];
}

export interface ImportRowError {
  row: number;          // 1-based row index from the sheet
  field: string | null; // null = row-level error (e.g. company not found)
  message: string;
}
