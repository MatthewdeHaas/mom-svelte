//! Shopping-list sheet. Port of
//! `src/lib/excel/export/shopping-list.ts`.
//!
//! Pulls every order in the window, fetches recipe-with-ingredients
//! for the referenced recipe set, then aggregates ingredients by
//! `(name, unit)` — same name with a different unit stays on its own
//! row so the kitchen's shopping list isn't lying about g vs kg.
//!
//! Aggregation math is identical to TS:
//!     production_qty = portion_size * amount  (or just amount if NULL)
//!     contribution   = recipe_ingredient.amount * production_qty
//! Final `total_amount` is rounded to 3 decimals before emit so float
//! noise doesn't make two identical semantic inputs render as
//! `3.0000000004` and `3.0`.
//!
//! Byte-identical ZIP determinism is NOT handled here — see
//! `production_order` module docs for the rationale.

use std::collections::BTreeSet;

use chrono::NaiveDate;
use db::queries::orders::{self, OrderFilters};
use db::queries::recipes;
use indexmap::IndexMap;
use rust_xlsxwriter::{Color, Format, FormatAlign, Workbook};
use sqlx::PgPool;
use uuid::Uuid;

use super::ExportError;

/// Aggregated row for a `(ingredient_name, unit)` pair. Local to this
/// module — the sheet writer is the only consumer.
#[derive(Debug)]
struct Aggregated {
    name: String,
    total_amount: f64,
    unit: String,
    allergens: String,
}

/// Build the shopping-list xlsx. `from` / `to` are inclusive on
/// `serving_date` — same window semantics as the TS source.
pub async fn generate(
    pool: &PgPool,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<u8>, ExportError> {
    let filters = OrderFilters {
        serving_date_from: Some(from),
        serving_date_to: Some(to),
        ..Default::default()
    };
    let orders_vec = orders::list(pool, &filters).await?;

    // Unique recipe ids. BTreeSet keeps the input to the fetch sorted
    // so test fixtures don't flap on HashMap iteration order.
    let recipe_ids: Vec<Uuid> = {
        let mut set = BTreeSet::new();
        for o in &orders_vec {
            if let Some(rid) = o.recipe_id {
                set.insert(rid);
            }
        }
        set.into_iter().collect()
    };

    let recipes_vec = recipes::get_with_ingredients_by_ids(pool, &recipe_ids).await?;

    // id → recipe lookup; TS `getRecipesWithIngredientsByIds` returns a
    // Map, this helper returns a Vec, so index it ourselves.
    let recipe_map: std::collections::HashMap<Uuid, &recipes::RecipeWithIngredientsRow> =
        recipes_vec.iter().map(|r| (r.id, r)).collect();

    // Aggregate by `name::unit`. IndexMap preserves insertion order so
    // tests that round-trip the output can read it back in a stable
    // sequence.
    let mut agg: IndexMap<String, Aggregated> = IndexMap::new();

    for order in &orders_vec {
        let Some(rid) = order.recipe_id else { continue };
        let Some(recipe) = recipe_map.get(&rid) else {
            continue;
        };
        let production_qty = match order.portion_size {
            Some(p) => p * order.amount,
            None => order.amount,
        };

        for ing in &recipe.ingredients {
            let key = format!("{}::{}", ing.name, ing.unit);
            let entry = agg.entry(key).or_insert_with(|| {
                // Allergen names: dedupe + sort for stable output.
                // BTreeSet handles both in one shot.
                let names: BTreeSet<&str> =
                    ing.allergen.iter().map(|a| a.name.as_str()).collect();
                let allergens = names.into_iter().collect::<Vec<_>>().join(", ");
                Aggregated {
                    name: ing.name.clone(),
                    total_amount: 0.0,
                    unit: ing.unit.clone(),
                    allergens,
                }
            });
            entry.total_amount += ing.amount * production_qty;
        }
    }

    // Sort by ingredient name (case-insensitive). TS uses
    // `localeCompare(..., { sensitivity: 'base' })` — the ASCII-fold
    // here matches for the ingredient names we actually ship (no
    // non-ASCII in the dataset today). If that changes, swap to
    // `unicase` / `caseless`.
    let mut rows: Vec<Aggregated> = agg.into_values().collect();
    rows.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    build_workbook(&rows)
}

/// Column widths and headers — kept as a module-level const for the
/// same reason production_order does: easy diff against TS source.
const HEADERS: &[(&str, f64)] = &[
    ("Ingredient", 32.0),
    ("Total Amount", 14.0),
    ("Unit", 10.0),
    ("Allergens", 36.0),
];

fn build_workbook(rows: &[Aggregated]) -> Result<Vec<u8>, ExportError> {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name("Shopping List")?;

    // Header styling: brand green (distinct from production sheet
    // blue), bold white, centered.
    let header_fmt = Format::new()
        .set_bold()
        .set_font_color(Color::RGB(0xFFFFFF))
        .set_background_color(Color::RGB(0x375623))
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter);

    for (idx, (title, width)) in HEADERS.iter().enumerate() {
        let col = idx as u16;
        ws.write_with_format(0, col, *title, &header_fmt)?;
        ws.set_column_width(col, *width)?;
    }
    ws.set_row_height(0, 20.0)?;

    // Column number format for Total Amount — matches TS `#,##0.###`.
    let amount_fmt = Format::new().set_num_format("#,##0.###");
    ws.set_column_format(1, &amount_fmt)?;

    // Pre-built per-cell formats. Banded variants fold the stripe
    // fill into a single Format so `write_with_format` is one call
    // per cell.
    let center = Format::new().set_align(FormatAlign::Center);
    let right_amount = Format::new()
        .set_align(FormatAlign::Right)
        .set_num_format("#,##0.###");
    let stripe = Color::RGB(0x00F2_F8F0);
    let plain_band = Format::new().set_background_color(stripe);
    let right_band = Format::new()
        .set_align(FormatAlign::Right)
        .set_num_format("#,##0.###")
        .set_background_color(stripe);
    let center_band = Format::new()
        .set_align(FormatAlign::Center)
        .set_background_color(stripe);

    for (i, r) in rows.iter().enumerate() {
        let row = (i as u32) + 1;
        // Spreadsheet row = i + 2; even when (i+2) % 2 == 0 → i even.
        let banded = i % 2 == 0;

        // Match TS: `Math.round(x * 1000) / 1000` on the amount so the
        // display is a clean 3-decimal value regardless of float noise.
        let amount = (r.total_amount * 1000.0).round() / 1000.0;

        // Ingredient — plain text, stripe only when banded.
        if banded {
            ws.write_string_with_format(row, 0, &r.name, &plain_band)?;
        } else {
            ws.write_string(row, 0, &r.name)?;
        }

        // Total Amount — right aligned + 3-decimal number format.
        let amount_cell = if banded { &right_band } else { &right_amount };
        ws.write_number_with_format(row, 1, amount, amount_cell)?;

        // Unit — centered.
        let unit_cell = if banded { &center_band } else { &center };
        ws.write_string_with_format(row, 2, &r.unit, unit_cell)?;

        // Allergens — plain text, stripe only when banded.
        if banded {
            ws.write_string_with_format(row, 3, &r.allergens, &plain_band)?;
        } else {
            ws.write_string(row, 3, &r.allergens)?;
        }
    }

    ws.set_freeze_panes(1, 0)?;

    Ok(wb.save_to_buffer()?)
}
