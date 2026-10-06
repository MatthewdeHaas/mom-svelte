//! Recipe-import orchestration. Mirrors
//! `src/lib/server/queries/recipe-imports.ts` 1:1 — same ordered steps
//! (collect uniques → upsert recipe/ingredient/allergen → group by recipe
//! → full-replace recipe_ingredient → full-replace ingredient_allergen),
//! same transaction boundary, same return shape.
//!
//! The row input type `RecipeExcelRowParsed` lives in the pure-types
//! `domain::recipes` crate — NOT in `excel::recipes` — because
//! `excel → db` already runs the other way (orders importer), and
//! `db → excel` would cycle. See `crates/domain/src/recipes.rs`.
//!
//! Semantics worth pinning down (TS parity):
//! - A row with ingredient=None is a "declare recipe exists" row;
//!   it participates in the recipe upsert but contributes nothing
//!   to the recipe_ingredient / ingredient_allergen replace.
//! - DELETE+INSERT per recipe is the full-replace model — a second
//!   import of the same recipe replaces its ingredient list wholesale.
//! - DELETE+INSERT per ingredient is also full-replace: whatever
//!   allergens appear in the input for a given ingredient fully
//!   determines that ingredient's allergen set after commit.
//! - `ingredients_created` is the count of DISTINCT ingredient names
//!   in the input — matches the TS approximation at
//!   `recipe-imports.ts:208` (true "newly inserted" would need a
//!   before/after diff and no caller does).
//! - `(xmax = 0) AS created` is the canonical pg trick for
//!   "RETURNING knows whether the row was inserted vs updated":
//!   on INSERT xmax is zero; on UPDATE the xmax is the current
//!   txid. The TS code uses the exact same expression.
//! - UNIQUE-name is enforced on recipe/ingredient/allergen; the
//!   orchestrator relies on that for ON CONFLICT. No handler-layer
//!   23505 translation is needed because every write here is an
//!   explicit upsert.

use std::collections::{HashMap, HashSet};

use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use domain::recipes::{RecipeExcelRowParsed, RecipeImportResult};

/// Full recipe import pipeline. Expects pre-validated rows from
/// `excel::recipes::parse_recipe_xlsx`. All writes happen inside a
/// single transaction — on error the whole batch rolls back.
///
/// Mirrors `importRecipesFromRows` at
/// `src/lib/server/queries/recipe-imports.ts:129` byte-for-byte on
/// visible side effects. `_created_by` is accepted for TS surface
/// parity (audit-log hook); unused today.
pub async fn import_recipes_from_rows(
    pool: &PgPool,
    rows: &[RecipeExcelRowParsed],
    _created_by: &str,
) -> Result<RecipeImportResult, sqlx::Error> {
    // ── 1. Collect unique entity names ─────────────────────────────────────
    //
    // Preserve first-seen order so test output is deterministic and the
    // batch INSERT parameter order matches between TS and Rust. HashSet
    // for dedup, Vec for order; one pass through rows.
    let recipe_names = unique_in_order(rows.iter().map(|r| r.recipe.as_str()));

    let ingredient_names = unique_in_order(
        rows.iter()
            .filter_map(|r| r.ingredient.as_deref())
            .filter(|s| !s.is_empty()),
    );

    // Allergen names come from the comma-separated `allergens` cell on
    // any row (even recipe-only rows). TS splits+trims+filter(Boolean);
    // we do the same.
    let allergen_names = {
        let flat: Vec<String> = rows
            .iter()
            .filter_map(|r| r.allergens.as_deref())
            .flat_map(|s| s.split(',').map(|p| p.trim().to_string()))
            .filter(|s| !s.is_empty())
            .collect();
        unique_in_order(flat.iter().map(|s| s.as_str()))
    };

    let mut tx = pool.begin().await?;

    // ── 2. Batch upserts → name→id maps ────────────────────────────────────
    let RecipeUpsert {
        name_to_id: recipe_map,
        recipes_created,
        recipes_updated,
    } = upsert_recipes(&mut tx, &recipe_names).await?;
    let ingredient_map = upsert_ingredients(&mut tx, &ingredient_names).await?;
    let allergen_map = upsert_allergens(&mut tx, &allergen_names).await?;

    // ── 3. Group rows by recipe ───────────────────────────────────────────
    //
    // Preserve first-seen order of recipes, and within each recipe the
    // original row order (TS iterates `rowsByRecipe` in insertion order).
    let mut recipe_order: Vec<String> = Vec::new();
    let mut rows_by_recipe: HashMap<String, Vec<&RecipeExcelRowParsed>> = HashMap::new();
    for row in rows {
        let entry = rows_by_recipe.entry(row.recipe.clone()).or_insert_with(|| {
            recipe_order.push(row.recipe.clone());
            Vec::new()
        });
        entry.push(row);
    }

    // ── 4. Replace recipe_ingredient per recipe ───────────────────────────
    //
    // Within a recipe we dedup by ingredient_id — first occurrence wins
    // (matches the TS `seen.add(ingredient_id)` guard at
    // `recipe-imports.ts:167-176`). This is important: a user Excel file
    // can list the same ingredient twice for one recipe; we take the
    // first row's amount/unit, not the last.
    for recipe_name in &recipe_order {
        let Some(&recipe_id) = recipe_map.get(recipe_name) else {
            continue;
        };
        let recipe_rows = &rows_by_recipe[recipe_name];

        let mut seen: HashSet<Uuid> = HashSet::new();
        let mut lines: Vec<(Uuid, f64, String)> = Vec::new();
        for r in recipe_rows {
            let Some(iname) = r.ingredient.as_deref() else {
                continue;
            };
            let Some(&iid) = ingredient_map.get(iname) else {
                continue;
            };
            if !seen.insert(iid) {
                continue;
            }
            // Parser guarantees `amount` and `unit` are Some when
            // `ingredient` is Some — enforced in excel/src/recipes.rs
            // row validation. Unwrap is safe post-parse.
            let amount = r.amount.expect("amount Some when ingredient Some");
            let unit = r.unit.clone().expect("unit Some when ingredient Some");
            lines.push((iid, amount, unit));
        }

        set_recipe_ingredients(&mut tx, recipe_id, &lines).await?;
    }

    // ── 5. Replace ingredient_allergen per ingredient ─────────────────────
    //
    // Build ingredient_name → Vec<allergen_id> from rows. Last row wins
    // if the same ingredient appears on multiple rows with different
    // allergen lists — matches TS `allergensByIngredient.set(...)` which
    // overwrites each iteration.
    let mut allergens_by_ingredient: HashMap<String, Vec<Uuid>> = HashMap::new();
    let mut ingredient_order_for_allergens: Vec<String> = Vec::new();
    for row in rows {
        let (Some(iname), Some(a_raw)) = (row.ingredient.as_deref(), row.allergens.as_deref())
        else {
            continue;
        };
        let ids: Vec<Uuid> = a_raw
            .split(',')
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
            .filter_map(|p| allergen_map.get(p).copied())
            .collect();
        if !allergens_by_ingredient.contains_key(iname) {
            ingredient_order_for_allergens.push(iname.to_string());
        }
        allergens_by_ingredient.insert(iname.to_string(), ids);
    }

    for iname in &ingredient_order_for_allergens {
        let Some(&iid) = ingredient_map.get(iname.as_str()) else {
            continue;
        };
        let aids = &allergens_by_ingredient[iname];
        set_ingredient_allergens(&mut tx, iid, aids).await?;
    }

    tx.commit().await?;

    Ok(RecipeImportResult {
        recipes_created,
        recipes_updated,
        ingredients_created: ingredient_names.len() as i64,
        rows_processed: rows.len() as i64,
        errors: Vec::new(),
    })
}

// ─────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────

/// First-seen-order dedup. `HashSet::insert` returns false on repeat,
/// which gates the `push`. Preserves input order so parameter order on
/// batch INSERTs is deterministic.
fn unique_in_order<'a, I>(iter: I) -> Vec<String>
where
    I: Iterator<Item = &'a str>,
{
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for s in iter {
        if seen.insert(s.to_string()) {
            out.push(s.to_string());
        }
    }
    out
}

/// Output of the recipe upsert: the id lookup plus insert/update counts
/// pulled from the `(xmax = 0)` sentinel.
struct RecipeUpsert {
    name_to_id: HashMap<String, Uuid>,
    recipes_created: i64,
    recipes_updated: i64,
}

/// Upsert recipes by name. ON CONFLICT re-activates (sets active=TRUE)
/// so a previously-deactivated recipe re-imported becomes live again —
/// matches TS at `recipe-imports.ts:28-32`.
///
/// `(xmax = 0) AS created` is pg's canonical "was this an INSERT" sentinel.
async fn upsert_recipes(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    names: &[String],
) -> Result<RecipeUpsert, sqlx::Error> {
    if names.is_empty() {
        return Ok(RecipeUpsert {
            name_to_id: HashMap::new(),
            recipes_created: 0,
            recipes_updated: 0,
        });
    }

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new("INSERT INTO recipe (name) ");
    qb.push_values(names.iter(), |mut b, n| {
        b.push_bind(n);
    });
    qb.push(
        " ON CONFLICT (name) DO UPDATE SET active = TRUE \
         RETURNING id, name, (xmax = 0) AS created",
    );

    let rows: Vec<(Uuid, String, bool)> = qb.build_query_as().fetch_all(&mut **tx).await?;

    let mut name_to_id: HashMap<String, Uuid> = HashMap::with_capacity(rows.len());
    let mut recipes_created: i64 = 0;
    for (id, name, created) in &rows {
        name_to_id.insert(name.clone(), *id);
        if *created {
            recipes_created += 1;
        }
    }
    let recipes_updated = rows.len() as i64 - recipes_created;

    Ok(RecipeUpsert {
        name_to_id,
        recipes_created,
        recipes_updated,
    })
}

/// Upsert ingredients by name. ON CONFLICT DO NOTHING — ingredient has
/// no mutable fields in this importer (cost_per_unit is admin-only via
/// the ingredient page). DO NOTHING skips RETURNING for conflicts, so
/// we follow up with a SELECT to resolve the ids of pre-existing names.
async fn upsert_ingredients(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    names: &[String],
) -> Result<HashMap<String, Uuid>, sqlx::Error> {
    if names.is_empty() {
        return Ok(HashMap::new());
    }

    // First pass: insert what we can; skip collisions.
    let mut ins_qb: QueryBuilder<Postgres> = QueryBuilder::new("INSERT INTO ingredient (name) ");
    ins_qb.push_values(names.iter(), |mut b, n| {
        b.push_bind(n);
    });
    ins_qb.push(" ON CONFLICT (name) DO NOTHING RETURNING id, name");
    let inserted: Vec<(Uuid, String)> = ins_qb.build_query_as().fetch_all(&mut **tx).await?;

    let mut map: HashMap<String, Uuid> = HashMap::with_capacity(names.len());
    for (id, name) in inserted {
        map.insert(name, id);
    }

    // Second pass: pull ids for any names that collided (not returned by DO NOTHING).
    let missing: Vec<&str> = names
        .iter()
        .filter(|n| !map.contains_key(n.as_str()))
        .map(|n| n.as_str())
        .collect();
    if !missing.is_empty() {
        let existing: Vec<(Uuid, String)> =
            sqlx::query_as("SELECT id, name FROM ingredient WHERE name = ANY($1)")
                .bind(&missing)
                .fetch_all(&mut **tx)
                .await?;
        for (id, name) in existing {
            map.insert(name, id);
        }
    }

    Ok(map)
}

/// Upsert allergens by name. Same ON CONFLICT DO NOTHING + fetch-missing
/// pattern as ingredients.
async fn upsert_allergens(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    names: &[String],
) -> Result<HashMap<String, Uuid>, sqlx::Error> {
    if names.is_empty() {
        return Ok(HashMap::new());
    }

    let mut ins_qb: QueryBuilder<Postgres> = QueryBuilder::new("INSERT INTO allergen (name) ");
    ins_qb.push_values(names.iter(), |mut b, n| {
        b.push_bind(n);
    });
    ins_qb.push(" ON CONFLICT (name) DO NOTHING RETURNING id, name");
    let inserted: Vec<(Uuid, String)> = ins_qb.build_query_as().fetch_all(&mut **tx).await?;

    let mut map: HashMap<String, Uuid> = HashMap::with_capacity(names.len());
    for (id, name) in inserted {
        map.insert(name, id);
    }

    let missing: Vec<&str> = names
        .iter()
        .filter(|n| !map.contains_key(n.as_str()))
        .map(|n| n.as_str())
        .collect();
    if !missing.is_empty() {
        let existing: Vec<(Uuid, String)> =
            sqlx::query_as("SELECT id, name FROM allergen WHERE name = ANY($1)")
                .bind(&missing)
                .fetch_all(&mut **tx)
                .await?;
        for (id, name) in existing {
            map.insert(name, id);
        }
    }

    Ok(map)
}

/// Full-replace a recipe's ingredient list: DELETE then bulk INSERT.
///
/// `recipe::set_ingredients` exists but owns its own tx and takes a
/// `&PgPool`. We need to compose with the orchestration tx, so this
/// one is tx-local and private.
///
/// `amount` goes through `::float8::numeric` to match the write
/// discipline used by `recipes::add_ingredient`.
async fn set_recipe_ingredients(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    recipe_id: Uuid,
    lines: &[(Uuid, f64, String)],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"DELETE FROM recipe_ingredient WHERE recipe_id = $1"#,
        recipe_id,
    )
    .execute(&mut **tx)
    .await?;

    if lines.is_empty() {
        return Ok(());
    }

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "INSERT INTO recipe_ingredient (recipe_id, ingredient_id, amount, unit) ",
    );
    // `push_values` auto-comma-separates `push_bind` calls within the
    // tuple — don't insert `push()` fragments between them or we lose
    // the separator and emit `($1 $2 $3 $4)` which pg rejects at scan.
    // sqlx handles the f64 → NUMERIC coercion natively (same as
    // `recipes::set_ingredients`), so no `::float8::numeric` cast here.
    qb.push_values(lines.iter(), |mut b, (iid, amount, unit)| {
        b.push_bind(recipe_id)
            .push_bind(iid)
            .push_bind(amount)
            .push_bind(unit);
    });
    qb.build().execute(&mut **tx).await?;

    Ok(())
}

/// Full-replace an ingredient's allergen set.
async fn set_ingredient_allergens(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    ingredient_id: Uuid,
    allergen_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"DELETE FROM ingredient_allergen WHERE ingredient_id = $1"#,
        ingredient_id,
    )
    .execute(&mut **tx)
    .await?;

    if allergen_ids.is_empty() {
        return Ok(());
    }

    let mut qb: QueryBuilder<Postgres> =
        QueryBuilder::new("INSERT INTO ingredient_allergen (ingredient_id, allergen_id) ");
    qb.push_values(allergen_ids.iter(), |mut b, aid| {
        b.push_bind(ingredient_id).push_bind(aid);
    });
    qb.build().execute(&mut **tx).await?;

    Ok(())
}
