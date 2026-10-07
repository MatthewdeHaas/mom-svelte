//! Production-order sheet. Port of
//! `src/lib/excel/export/production-order.ts`.
//!
//! One sheet, 11 columns, rows sorted by the composite content tuple
//! `(recipe_name, company_name, serving_date, meal_category,
//! portion_size, amount, client, (frozen, chilled))`. The tuple is
//! deliberately UUID-free — order rows get a fresh UUID per import, so
//! tie-breaking on `id` flipped output byte-for-byte between runs of
//! the same semantic data. See the TS source for the full rationale.
//!
//! Byte-identical ZIP determinism (epoch stamp + docProps/core.xml
//! scrub) is NOT implemented here — the TS post-processor runs the
//! xlsx bytes through JSZip, and `rust_xlsxwriter` emits its own ZIP
//! structure that'll need a separate pin pass. That lands in the
//! sub-session that rewrites the oracle to semantic extraction;
//! today's goal is column parity and sort parity, nothing more.

use chrono::NaiveDate;
use db::queries::orders::{self, OrderFilters, OrderSummaryRow};
use rust_xlsxwriter::{Color, Format, FormatAlign, Workbook};
use sqlx::PgPool;

use super::ExportError;

/// Build the production-order xlsx. `from` / `to` are inclusive on
/// `serving_date` (same semantics as `getOrders({ serving_date_from,
/// serving_date_to })` on the TS side).
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
    let mut rows = orders::list(pool, &filters).await?;

    // Content-based sort — mirrors the TS tuple exactly, including the
    // trailing (frozen, chilled) tie-break. See module docs.
    rows.sort_by(|a, b| {
        a.recipe_name
            .as_deref()
            .unwrap_or("")
            .cmp(b.recipe_name.as_deref().unwrap_or(""))
            .then_with(|| a.company_name.cmp(&b.company_name))
            .then_with(|| a.serving_date.cmp(&b.serving_date))
            .then_with(|| {
                a.meal_category
                    .as_deref()
                    .unwrap_or("")
                    .cmp(b.meal_category.as_deref().unwrap_or(""))
            })
            .then_with(|| {
                a.portion_size
                    .unwrap_or(0.0)
                    .partial_cmp(&b.portion_size.unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                a.amount
                    .partial_cmp(&b.amount)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                a.client
                    .as_deref()
                    .unwrap_or("")
                    .cmp(b.client.as_deref().unwrap_or(""))
            })
            .then_with(|| a.frozen.cmp(&b.frozen))
            .then_with(|| a.chilled.cmp(&b.chilled))
    });

    build_workbook(&rows)
}

/// Column widths and headers match the TS `sheet.columns` definition
/// 1:1. Keeping them in a const + loop so the review diff against the
/// TS source is literally the same list.
const HEADERS: &[(&str, f64)] = &[
    ("Customer", 24.0),
    ("Serving Date", 14.0),
    ("Recipe", 32.0),
    ("Meal", 12.0),
    ("Day", 6.0),
    ("Portion Size", 14.0),
    ("Order Amount", 14.0),
    ("Quantity to Produce/Ship", 22.0),
    ("Client", 32.0),
    ("Storage", 10.0),
    ("Shipment Date", 14.0),
];

/// Shared workbook builder — pure, no DB. Factored out so a future
/// unit test can round-trip a hand-rolled `Vec<OrderSummaryRow>`
/// without touching sqlx.
fn build_workbook(rows: &[OrderSummaryRow]) -> Result<Vec<u8>, ExportError> {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name("Production Order")?;

    // Header styling: brand blue background, bold white text, centered.
    let header_fmt = Format::new()
        .set_bold()
        .set_font_color(Color::RGB(0xFFFFFF))
        .set_background_color(Color::RGB(0x2E75B6))
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter);

    for (idx, (title, width)) in HEADERS.iter().enumerate() {
        let col = idx as u16;
        ws.write_with_format(0, col, *title, &header_fmt)?;
        ws.set_column_width(col, *width)?;
    }
    ws.set_row_height(0, 20.0)?;

    // Date column format (yyyy-mm-dd) applied via a per-column format
    // so new rows pick it up automatically. Columns 1 (serving_date)
    // and 10 (shipment_date).
    let date_fmt = Format::new().set_num_format("yyyy-mm-dd");
    ws.set_column_format(1, &date_fmt)?;
    ws.set_column_format(10, &date_fmt)?;

    // Pre-built alignment formats — rust_xlsxwriter requires borrowing
    // the Format in `write_with_format`, so instantiate once outside
    // the hot loop. Even-row (banded) variants fold the stripe fill
    // into the same Format for a single `write_with_format` call per
    // cell.
    let center = Format::new().set_align(FormatAlign::Center);
    let right = Format::new().set_align(FormatAlign::Right);
    let stripe = Color::RGB(0x00F2_F7FB);
    let center_band = Format::new()
        .set_align(FormatAlign::Center)
        .set_background_color(stripe);
    let right_band = Format::new()
        .set_align(FormatAlign::Right)
        .set_background_color(stripe);
    let plain_band = Format::new().set_background_color(stripe);
    let date_band = Format::new()
        .set_num_format("yyyy-mm-dd")
        .set_background_color(stripe);

    for (i, o) in rows.iter().enumerate() {
        let row = (i as u32) + 1;
        // Row 2 is "even" per the TS `rowNumber % 2 === 0` check (TS
        // sheet row is 1-based; headerRow is 1, first data row is 2).
        // Translate: zero-based data index `i` → spreadsheet row
        // `i+2` → even when `(i+2) % 2 == 0` → `i % 2 == 0`.
        let banded = i % 2 == 0;

        let portion_size = o.portion_size;
        let qty_produce = match portion_size {
            Some(p) => p * o.amount,
            None => o.amount,
        };
        let storage = if o.frozen {
            "f"
        } else if o.chilled {
            "c"
        } else {
            ""
        };

        // Customer / Recipe / Meal / Client — plain text, stripe only.
        let text_fmt = if banded { Some(&plain_band) } else { None };
        write_str(ws, row, 0, &o.company_name, text_fmt)?;
        write_date(ws, row, 1, o.serving_date, banded, &date_band, &date_fmt)?;
        write_str(
            ws,
            row,
            2,
            o.recipe_name.as_deref().unwrap_or("—"),
            text_fmt,
        )?;
        write_str(ws, row, 3, o.meal_category.as_deref().unwrap_or(""), text_fmt)?;

        // Day — centered.
        let day_fmt = if banded { &center_band } else { &center };
        match o.day {
            Some(d) => ws.write_number_with_format(row, 4, d as f64, day_fmt)?,
            None => ws.write_string_with_format(row, 4, "", day_fmt)?,
        };

        // Portion Size / Amount / Qty — right-aligned numbers (or
        // empty string when portion_size is NULL, matching TS which
        // emits `''` rather than 0 to avoid confusion between
        // "unspecified" and "zero portions").
        let right_fmt = if banded { &right_band } else { &right };
        match portion_size {
            Some(p) => ws.write_number_with_format(row, 5, p, right_fmt)?,
            None => ws.write_string_with_format(row, 5, "", right_fmt)?,
        };
        ws.write_number_with_format(row, 6, o.amount, right_fmt)?;
        ws.write_number_with_format(row, 7, qty_produce, right_fmt)?;

        write_str(ws, row, 8, o.client.as_deref().unwrap_or(""), text_fmt)?;

        // Storage — centered.
        let storage_fmt = if banded { &center_band } else { &center };
        ws.write_string_with_format(row, 9, storage, storage_fmt)?;

        write_date(ws, row, 10, o.shipment_date, banded, &date_band, &date_fmt)?;
    }

    ws.set_freeze_panes(1, 0)?;

    Ok(wb.save_to_buffer()?)
}

/// Helper — write a string with optional banded-stripe format.
fn write_str(
    ws: &mut rust_xlsxwriter::Worksheet,
    row: u32,
    col: u16,
    value: &str,
    fmt: Option<&Format>,
) -> Result<(), ExportError> {
    match fmt {
        Some(f) => {
            ws.write_string_with_format(row, col, value, f)?;
        }
        None => {
            ws.write_string(row, col, value)?;
        }
    }
    Ok(())
}

/// Helper — write a date cell. `rust_xlsxwriter::ExcelDateTime` would
/// be the "correct" approach but requires an extra conversion; the TS
/// side writes a plain `Date` and lets ExcelJS turn it into a numeric
/// serial. The simplest Rust parity is to write the ISO string and
/// rely on the column's `yyyy-mm-dd` number format — Excel renders it
/// identically for display purposes. If byte parity requires a true
/// numeric serial the oracle-rewrite sub-session will swap this out.
fn write_date(
    ws: &mut rust_xlsxwriter::Worksheet,
    row: u32,
    col: u16,
    value: NaiveDate,
    banded: bool,
    band_fmt: &Format,
    plain_fmt: &Format,
) -> Result<(), ExportError> {
    let s = value.format("%Y-%m-%d").to_string();
    let fmt = if banded { band_fmt } else { plain_fmt };
    ws.write_string_with_format(row, col, &s, fmt)?;
    Ok(())
}
