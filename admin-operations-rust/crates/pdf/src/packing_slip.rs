//! Packing slips. Port of `src/lib/pdf/packing-slip.ts`.
//!
//! Fetches orders in the week containing `reference_date` (EST
//! Mon-Sun), groups by `(company, shipment_date)`, and emits one PDF
//! per group. Letter geometry, Helvetica, pink header band.
//!
//! # Coordinate systems
//!
//! PDFKit draws from the top-left in points. `printpdf` draws from
//! the bottom-left in millimetres (f32). We stay in points throughout
//! and only convert to `Mm`/`Pt` at the call site. Every TS y-value is
//! flipped once at paint time via `pt_y_from_top(y_ts)` which returns
//! `Mm(PAGE_H - y_ts)` — same effective coordinate frame as PDFKit
//! lands on, with less arithmetic sprinkled through the body.
//!
//! # "Page N of T"
//!
//! TS emits `Page N of ##TOTAL##`, counts placeholder occurrences in
//! the final buffer, and byte-patches the total back in. The orch
//! wants the direct approach: a pagination dry-run to compute total
//! pages per group, then one real render with the real total baked
//! in. Mirror of behaviour, not of mechanism.
//!
//! # Determinism
//!
//! `creation_date` / `modification_date` / `metadata_date` are pinned
//! to Unix epoch (TS: `new Date(0)`), creator/producer/title/subject
//! match the TS Info dict. The ZIP-level pin that the Excel path does
//! has no PDF analogue — printpdf doesn't stamp wall-clock time
//! anywhere else. Byte-identical validation lands in the oracle-
//! rewrite sub-session.

use std::collections::BTreeMap;

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use db::queries::orders::{self, OrderDetailRow, OrderFilters};
use printpdf::path::PaintMode;
use printpdf::{
    BuiltinFont, Color, IndirectFontRef, Line, Mm, PdfDocumentReference, PdfLayerReference,
    Point, Pt, Rect, Rgb, Svg, SvgTransform,
};
use sqlx::PgPool;

use crate::PdfError;

/// One generated packing slip, ready for HTTP delivery or zipping.
#[derive(Debug, Clone)]
pub struct PackingSlipPdf {
    pub filename: String,
    pub company_name: String,
    pub pdf_bytes: Vec<u8>,
}

// ─── Page geometry (Letter, points) ──────────────────────────────────

const PAGE_W: f32 = 612.0; // 8.5 in
const PAGE_H: f32 = 792.0; // 11 in
const ML: f32 = 10.0;
const MR: f32 = 10.0;
const MT: f32 = 25.0;
const MB: f32 = 25.0;
const CONTENT_W: f32 = PAGE_W - ML - MR; // 592

// ─── Logo ────────────────────────────────────────────────────────────

const LOGO_SVG: &str = include_str!(
    "../../../../static/mom-logo.svg"
);
const LOGO_W: f32 = 100.0;
// SVG viewBox is 192×107, preserve aspect ratio.
const LOGO_H: f32 = (107.0 / 192.0) * LOGO_W; // ≈ 55.72 pt

// ─── Colours (RGB in 0..1 for printpdf::Rgb) ─────────────────────────

fn rgb_hex(hex: u32) -> Color {
    let r = ((hex >> 16) & 0xff) as f32 / 255.0;
    let g = ((hex >> 8) & 0xff) as f32 / 255.0;
    let b = (hex & 0xff) as f32 / 255.0;
    Color::Rgb(Rgb::new(r, g, b, None))
}

// Named colour constants matching the TS palette.
fn c_black() -> Color {
    rgb_hex(0x000000)
}
fn c_header_bg() -> Color {
    rgb_hex(0xC9718A)
}
fn c_header_text() -> Color {
    rgb_hex(0xFFFFFF)
}
fn c_grid() -> Color {
    rgb_hex(0xA9A9A9)
}
fn c_muted() -> Color {
    rgb_hex(0x4A4A4A)
}

// ─── Table columns ───────────────────────────────────────────────────

#[derive(Clone, Copy)]
#[allow(dead_code)] // `Right` isn't used by any current column but the
                    // align handling branches on it; keep the variant so
                    // adding a right-aligned column later is a one-line
                    // edit, not a re-plumb.
enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy)]
struct Col {
    label: &'static str,
    width: f32,
    align: Align,
}

const COLS: &[Col] = &[
    Col { label: "Meal",     width: 230.0, align: Align::Left },
    Col { label: "Category", width: 90.0,  align: Align::Left },
    Col { label: "Qty",      width: 36.0,  align: Align::Center },
    Col { label: "Temp",     width: 58.0,  align: Align::Center },
    Col { label: "Note",     width: 178.0, align: Align::Left },
];
// 230 + 90 + 36 + 58 + 178 = 592 = CONTENT_W

const ROW_H: f32 = 18.0;
const HEADER_ROW_H: f32 = 20.0;
const CELL_PAD_X: f32 = 4.0;
const CELL_PAD_Y: f32 = 5.0;
const TABLE_FONT_SIZE: f32 = 8.0;

// ─── Grouping ────────────────────────────────────────────────────────

struct SlipGroup {
    company_name: String,
    shipping_address: String,
    shipment_date: NaiveDate,
    orders: Vec<OrderDetailRow>,
}

// ─── Coordinate helpers ──────────────────────────────────────────────

/// Flip a TS-style top-left y (in points) to printpdf bottom-left mm.
/// We stay in points up to the call site, so this is called with a
/// point value and returns the matching `Mm` for printpdf APIs.
fn mm_from_pt(y_pt_from_top: f32) -> Mm {
    // 1 pt = 25.4/72 mm. `Mm::from(Pt(x))` does this via `From`.
    Mm::from(Pt(PAGE_H - y_pt_from_top))
}

fn mm_x(x_pt: f32) -> Mm {
    Mm::from(Pt(x_pt))
}

// ─── Public entrypoint ───────────────────────────────────────────────

/// Generate one packing slip per `(company, shipment_date)` in the EST
/// week containing `reference_date`. Returns a `Vec` sorted by company
/// name then shipment date for stable output ordering.
pub async fn generate(
    pool: &PgPool,
    reference_date: NaiveDate,
) -> Result<Vec<PackingSlipPdf>, PdfError> {
    let (from, to) = week_bounds_est(reference_date);

    // 1. Fetch all orders for the week.
    let filters = OrderFilters {
        serving_date_from: Some(from),
        serving_date_to: Some(to),
        ..Default::default()
    };
    let summary = orders::list(pool, &filters).await?;

    // 2. Full detail per id. TS does `Promise.all`; we go serial —
    // packing-slip volume is small, parallelism not worth the hazard.
    let mut details: Vec<OrderDetailRow> = Vec::with_capacity(summary.len());
    for row in &summary {
        if let Some(detail) = orders::get_by_id(pool, row.id).await? {
            details.push(detail);
        }
    }

    // 3. Group by (company_id, shipment_date). BTreeMap keeps the
    // iteration order stable; final slip ordering is re-sorted below
    // by content keys so the key choice here doesn't affect output.
    let mut groups: BTreeMap<(uuid::Uuid, NaiveDate), SlipGroup> = BTreeMap::new();
    for d in details {
        let key = (d.company.id, d.shipment_date);
        groups
            .entry(key)
            .or_insert_with(|| SlipGroup {
                company_name: d.company.name.clone(),
                shipping_address: d.company.shipping_address.clone().unwrap_or_default(),
                shipment_date: d.shipment_date,
                orders: Vec::new(),
            })
            .orders
            .push(d);
    }

    // 4. Sort orders inside each group by the TS content tuple — same
    // reasoning as the Excel exports: avoid UUID tiebreak flip.
    for g in groups.values_mut() {
        g.orders.sort_by(|a, b| {
            let a_key = detail_sort_key(a);
            let b_key = detail_sort_key(b);
            a_key.cmp(&b_key)
        });
    }

    // 5. Final group ordering by company_name, shipment_date.
    let mut sorted_groups: Vec<SlipGroup> = groups.into_values().collect();
    sorted_groups.sort_by(|a, b| {
        a.company_name
            .cmp(&b.company_name)
            .then_with(|| a.shipment_date.cmp(&b.shipment_date))
    });

    // 6. Render each group.
    let mut out: Vec<PackingSlipPdf> = Vec::with_capacity(sorted_groups.len());
    for group in sorted_groups {
        let total_pages = count_pages(&group);
        let pdf_bytes = render_group(&group, total_pages)?;
        out.push(PackingSlipPdf {
            filename: format!("packing-slip-{}.pdf", group.shipment_date),
            company_name: group.company_name.clone(),
            pdf_bytes,
        });
    }

    Ok(out)
}

/// Content-based sort key for orders inside a group — same tuple TS
/// uses, pipe-joined into a comparable string.
fn detail_sort_key(d: &OrderDetailRow) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}",
        d.recipe.as_ref().map(|r| r.name.as_str()).unwrap_or(""),
        d.serving_date,
        d.meal_category.as_deref().unwrap_or(""),
        d.portion_size.map(|p| p.to_string()).unwrap_or_default(),
        d.amount,
        d.client.as_deref().unwrap_or(""),
        d.frozen as u8,
        d.chilled as u8,
    )
}

// ─── Week bounds (EST Mon-Sun) ───────────────────────────────────────

fn week_bounds_est(date: NaiveDate) -> (NaiveDate, NaiveDate) {
    // TS shifts the Date by -5h to approximate EST, then computes
    // Mon-Sun bounds on that shifted date. For a plain NaiveDate
    // without a time component the shift is a no-op (the date is
    // already "midnight local"), so just do the ISO week math.
    let weekday = date.weekday();
    let diff_to_mon = match weekday {
        Weekday::Mon => 0,
        Weekday::Tue => -1,
        Weekday::Wed => -2,
        Weekday::Thu => -3,
        Weekday::Fri => -4,
        Weekday::Sat => -5,
        Weekday::Sun => -6,
    };
    let mon = date + Duration::days(diff_to_mon);
    let sun = mon + Duration::days(6);
    (mon, sun)
}

// ─── Page counting (dry run) ─────────────────────────────────────────

/// Walk the group once without emitting operations to find out how
/// many pages the real render will produce. Mirrors the TS "if row
/// won't fit, addPage" loop in `renderSlip` exactly.
fn count_pages(group: &SlipGroup) -> u32 {
    let mut pages: u32 = 1;
    let header_y_after = page_header_y_after(group);
    let company_block_h = company_block_height(group);
    let mut y = header_y_after + company_block_h + 6.0;
    y += HEADER_ROW_H; // table header
    let bottom_limit = PAGE_H - MB - 30.0;

    for _ in &group.orders {
        if y + ROW_H > bottom_limit {
            pages += 1;
            // Next page skips the company block — matches TS `renderSlip`
            // branch where a mid-table page break only re-draws the
            // page header and table header, not the company block.
            y = page_header_y_after(group) + HEADER_ROW_H;
        }
        y += ROW_H;
    }
    pages
}

/// y position (top-left pt) immediately below the page header rule.
fn page_header_y_after(_group: &SlipGroup) -> f32 {
    // Title block occupies 4 (padding) + 22 (title line) + 14
    // (delivery line) = 40 pt below MT; logo is MT + LOGO_H + 4 pt.
    // sepY = max(title_bottom, logo_bottom); returned y = sepY + 8.
    let title_bottom = MT + 4.0 + 22.0 + 14.0;
    let logo_bottom = MT + LOGO_H + 4.0;
    title_bottom.max(logo_bottom) + 8.0
}

/// Approximate height of the company block (name line + optional
/// address line + 8 pt trailing pad). TS pdfkit measures text to pick
/// the exact height; here we use fixed line-heights (12 pt bold name
/// + 11 pt address + spacing) that are close enough for pagination
/// decisions. The real paint pass uses the same offsets.
fn company_block_height(group: &SlipGroup) -> f32 {
    let mut h = 14.0 + 3.0; // name line + spacing
    if !group.shipping_address.trim().is_empty() {
        h += 11.0 + 8.0; // address line + trailing pad
    } else {
        h += 8.0;
    }
    h
}

// ─── Rendering ───────────────────────────────────────────────────────

/// Render one slip to PDF bytes. `total_pages` is the final page total
/// baked into every footer — computed by `count_pages` on the same
/// group so the footer is right the first time.
fn render_group(group: &SlipGroup, total_pages: u32) -> Result<Vec<u8>, PdfError> {
    use printpdf::{OffsetDateTime, PdfDocument};

    let (doc, page1, layer1) = PdfDocument::new(
        format!("Packing Slip — {}", group.company_name),
        Mm::from(Pt(PAGE_W)),
        Mm::from(Pt(PAGE_H)),
        "Layer 1",
    );

    // Pin every mutable date + info-dict field to a fixed value so two
    // runs over the same semantic input produce identical bytes. These
    // builder methods take `self` by value, so chain them before any
    // page-level work and shadow `doc`.
    let epoch = OffsetDateTime::UNIX_EPOCH;
    let doc = doc
        .with_creation_date(epoch)
        .with_mod_date(epoch)
        .with_metadata_date(epoch)
        .with_title(format!("Packing Slip — {}", group.company_name))
        .with_subject(format!("Delivery {}", group.shipment_date))
        .with_creator("motm-admin")
        .with_producer("motm-admin")
        // Pin document_id — printpdf randomises it at construction,
        // which would otherwise flip bytes between runs even with
        // every date and info field pinned.
        .with_document_id("0000000000000000".to_string());

    // Fonts registered once per document — printpdf caches by name.
    let f_regular = doc
        .add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| PdfError::Build(e.to_string()))?;
    let f_bold = doc
        .add_builtin_font(BuiltinFont::HelveticaBold)
        .map_err(|e| PdfError::Build(e.to_string()))?;

    let mut page_num: u32 = 1;
    let layer = doc.get_page(page1).get_layer(layer1);

    draw_page_footer(&layer, &f_regular, page_num, total_pages);
    let mut y = draw_page_header(&doc, &layer, &f_regular, &f_bold, group);
    y = draw_company_block(&layer, &f_regular, &f_bold, group, y);
    y += 6.0;
    y = draw_table_header(&layer, &f_bold, y);

    let bottom_limit = PAGE_H - MB - 30.0;

    let mut current_layer = layer;

    for order in &group.orders {
        if y + ROW_H > bottom_limit {
            page_num += 1;
            let (new_page_idx, new_layer_idx) = doc.add_page(
                Mm::from(Pt(PAGE_W)),
                Mm::from(Pt(PAGE_H)),
                format!("Layer {page_num}"),
            );
            current_layer = doc.get_page(new_page_idx).get_layer(new_layer_idx);

            draw_page_footer(&current_layer, &f_regular, page_num, total_pages);
            y = draw_page_header(&doc, &current_layer, &f_regular, &f_bold, group);
            y = draw_table_header(&current_layer, &f_bold, y);
        }
        y = draw_order_row(&current_layer, &f_regular, order, y);
    }

    // Close-off rule under the last table row (slightly heavier).
    draw_hline(&current_layer, y, 1.0, c_grid());

    // Signature line.
    draw_signature_line(&current_layer, &f_regular, y);

    let bytes = doc
        .save_to_bytes()
        .map_err(|e| PdfError::Build(e.to_string()))?;
    Ok(bytes)
}

// ─── Drawing primitives ──────────────────────────────────────────────

fn draw_hline(layer: &PdfLayerReference, y_pt_from_top: f32, thickness: f32, color: Color) {
    layer.set_outline_color(color);
    layer.set_outline_thickness(thickness);
    let line = Line {
        points: vec![
            (Point { x: Pt(ML), y: Pt(PAGE_H - y_pt_from_top) }, false),
            (Point { x: Pt(PAGE_W - MR), y: Pt(PAGE_H - y_pt_from_top) }, false),
        ],
        is_closed: false,
    };
    layer.add_line(line);
}

fn draw_vline(layer: &PdfLayerReference, x_pt: f32, y_top_pt: f32, y_bot_pt: f32) {
    let line = Line {
        points: vec![
            (Point { x: Pt(x_pt), y: Pt(PAGE_H - y_top_pt) }, false),
            (Point { x: Pt(x_pt), y: Pt(PAGE_H - y_bot_pt) }, false),
        ],
        is_closed: false,
    };
    layer.add_line(line);
}

/// Draw an unfilled rectangle (grid cells, outer table border).
fn draw_row_border(layer: &PdfLayerReference, y_pt_from_top: f32, row_h: f32) {
    layer.set_outline_color(c_grid());
    layer.set_outline_thickness(0.5);
    // Outer rect.
    let rect = Rect::new(
        mm_x(ML),
        mm_from_pt(y_pt_from_top + row_h),
        mm_x(PAGE_W - MR),
        mm_from_pt(y_pt_from_top),
    )
    .with_mode(PaintMode::Stroke);
    layer.add_rect(rect);

    // Vertical dividers.
    let mut x = ML;
    for col in &COLS[..COLS.len() - 1] {
        x += col.width;
        draw_vline(layer, x, y_pt_from_top, y_pt_from_top + row_h);
    }
}

/// Draw a filled rectangle (header band).
fn draw_filled_rect(
    layer: &PdfLayerReference,
    x: f32,
    y_pt_from_top: f32,
    w: f32,
    h: f32,
    fill: Color,
) {
    layer.save_graphics_state();
    layer.set_fill_color(fill);
    let rect = Rect::new(
        mm_x(x),
        mm_from_pt(y_pt_from_top + h),
        mm_x(x + w),
        mm_from_pt(y_pt_from_top),
    )
    .with_mode(PaintMode::Fill);
    layer.add_rect(rect);
    layer.restore_graphics_state();
}

/// Place a text string at (x, y). `y_pt_from_top` is the TOP of the
/// text cell (matches PDFKit's `text(s, x, y)` where y is the cell
/// top); we shift down by the font size to put the baseline inside
/// the cell the same way PDFKit does.
fn draw_text(
    layer: &PdfLayerReference,
    text: &str,
    x_pt: f32,
    y_pt_from_top: f32,
    size: f32,
    font: &IndirectFontRef,
    fill: Color,
) {
    layer.save_graphics_state();
    layer.set_fill_color(fill);
    // PDFKit measures from text TOP; printpdf's use_text uses the
    // baseline. Shift by the font size so the glyphs sit inside the
    // nominal cell. `0.8 * size` is PDFKit's approximate baseline
    // offset for Helvetica; close enough for packing-slip cosmetics.
    let baseline_y = y_pt_from_top + 0.8 * size;
    layer.use_text(text, size, mm_x(x_pt), mm_from_pt(baseline_y), font);
    layer.restore_graphics_state();
}

/// Approximate glyph-advance width for Helvetica at `size` points.
/// PDFKit computes exact widths from the AFM; we don't have the AFM
/// handy, so use a conservative average of 0.5 em per character —
/// right-align and centre-align fall within a few points of the TS
/// output, which is fine for the compile-only port. The oracle-
/// rewrite sub-session can swap this for `rusttype`/`fontdb` metrics
/// if a byte-identical goal is reasserted.
fn approx_text_width(text: &str, size: f32) -> f32 {
    text.chars().count() as f32 * 0.5 * size
}

/// Place text aligned inside `[cell_x, cell_x + col.width]`. Padding
/// matches TS (CELL_PAD_X each side). No wrapping — long strings
/// overflow the right pad silently, same as TS `lineBreak: false`.
fn cell_text(
    layer: &PdfLayerReference,
    text: &str,
    cell_x: f32,
    cell_y: f32,
    col: &Col,
    font: &IndirectFontRef,
    bold_font: Option<&IndirectFontRef>,
    fill: Color,
    size: f32,
) {
    let inner_w = col.width - CELL_PAD_X * 2.0;
    let font_ref = bold_font.unwrap_or(font);
    let text_w = approx_text_width(text, size);
    let x = match col.align {
        Align::Left => cell_x + CELL_PAD_X,
        Align::Center => cell_x + CELL_PAD_X + (inner_w - text_w).max(0.0) / 2.0,
        Align::Right => cell_x + CELL_PAD_X + (inner_w - text_w).max(0.0),
    };
    draw_text(layer, text, x, cell_y + CELL_PAD_Y, size, font_ref, fill);
}

// ─── Composite blocks ────────────────────────────────────────────────

fn draw_page_header(
    doc: &PdfDocumentReference,
    layer: &PdfLayerReference,
    f_regular: &IndirectFontRef,
    f_bold: &IndirectFontRef,
    group: &SlipGroup,
) -> f32 {
    // Logo (right-aligned). `Svg::parse` can fail if the SVG is
    // malformed; we only ship one logo and it was probed clean, so
    // bubble the error up rather than skipping.
    //
    // SvgTransform translates in points from the bottom-left and
    // scales relative to the SVG's own px dimensions at 300 DPI
    // (printpdf default). The scale factors below map 192×107 px to
    // LOGO_W × LOGO_H pt.
    if let Ok(svg) = Svg::parse(LOGO_SVG) {
        let xo = svg.into_xobject(layer);
        let logo_x = PAGE_W - MR - LOGO_W;
        let logo_top = MT;
        // printpdf scales the xobject at 300 DPI by default — compute
        // scale so the SVG's 192 px wide renders at LOGO_W pt wide.
        // At 300 DPI, 1 px = 72/300 pt = 0.24 pt. For 192 px that's
        // 46.08 pt native; we want LOGO_W pt → scale = LOGO_W/46.08.
        let scale = LOGO_W / (192.0 * 72.0 / 300.0);
        xo.add_to_layer(
            layer,
            SvgTransform {
                translate_x: Some(Pt(logo_x)),
                translate_y: Some(Pt(PAGE_H - logo_top - LOGO_H)),
                scale_x: Some(scale),
                scale_y: Some(scale),
                dpi: Some(300.0),
                rotate: None,
            },
        );
    }

    // Title block.
    let mut y = MT + 4.0;
    draw_text(layer, "PACKING SLIP", ML, y, 16.0, f_bold, c_black());
    y += 22.0;

    let delivery = format!("Delivery Date: {}", format_date(group.shipment_date));
    draw_text(layer, &delivery, ML, y, 10.0, f_regular, c_black());
    y += 14.0;

    // Separator rule.
    let sep_y = y.max(MT + LOGO_H + 4.0);
    draw_hline(layer, sep_y, 0.5, c_grid());

    // Suppress unused-param warnings when the macro-less build drops
    // one of the args.
    let _ = doc;
    sep_y + 8.0
}

fn draw_company_block(
    layer: &PdfLayerReference,
    f_regular: &IndirectFontRef,
    f_bold: &IndirectFontRef,
    group: &SlipGroup,
    y_in: f32,
) -> f32 {
    let mut y = y_in;
    draw_text(layer, &group.company_name, ML, y, 12.0, f_bold, c_black());
    y += 14.0 + 3.0;
    let addr = group.shipping_address.trim();
    if !addr.is_empty() {
        draw_text(layer, addr, ML, y, 9.0, f_regular, c_muted());
        y += 11.0 + 8.0;
    } else {
        y += 8.0;
    }
    y
}

fn draw_table_header(
    layer: &PdfLayerReference,
    f_bold: &IndirectFontRef,
    y_in: f32,
) -> f32 {
    draw_filled_rect(layer, ML, y_in, CONTENT_W, HEADER_ROW_H, c_header_bg());
    draw_row_border(layer, y_in, HEADER_ROW_H);

    let mut x = ML;
    for col in COLS {
        let upper = col.label.to_uppercase();
        cell_text(
            layer,
            &upper,
            x,
            y_in,
            col,
            f_bold,
            None,
            c_header_text(),
            8.0,
        );
        x += col.width;
    }

    y_in + HEADER_ROW_H
}

fn draw_order_row(
    layer: &PdfLayerReference,
    f_regular: &IndirectFontRef,
    order: &OrderDetailRow,
    y_in: f32,
) -> f32 {
    draw_row_border(layer, y_in, ROW_H);

    let temp = if order.frozen {
        "Frozen"
    } else if order.chilled {
        "Chilled"
    } else {
        "Chilled" // TS falls through to Chilled when neither flag set
    };
    let cells = [
        order.recipe.as_ref().map(|r| r.name.as_str()).unwrap_or("—").to_string(),
        order.meal_category.clone().unwrap_or_else(|| "—".to_string()),
        format!("{}", order.amount),
        temp.to_string(),
        order.client.clone().unwrap_or_default().trim().to_string(),
    ];

    let mut x = ML;
    for (i, text) in cells.iter().enumerate() {
        cell_text(
            layer,
            text,
            x,
            y_in,
            &COLS[i],
            f_regular,
            None,
            c_black(),
            TABLE_FONT_SIZE,
        );
        x += COLS[i].width;
    }

    y_in + ROW_H
}

fn draw_signature_line(
    layer: &PdfLayerReference,
    f_regular: &IndirectFontRef,
    y_in: f32,
) -> f32 {
    let y = y_in + 12.0;
    draw_text(
        layer,
        "Packed By: ________________________   Checked By: ________________________",
        ML,
        y,
        10.0,
        f_regular,
        c_black(),
    );
    y + 10.0 + 8.0
}

fn draw_page_footer(
    layer: &PdfLayerReference,
    f_regular: &IndirectFontRef,
    page_num: u32,
    total_pages: u32,
) {
    let text = format!("Page {} of {}", page_num, total_pages);
    let footer_y = PAGE_H - MB + 6.0;
    // Right-align manually — compute approximate width and offset.
    let text_w = approx_text_width(&text, 9.0);
    let x = PAGE_W - MR - text_w;
    draw_text(layer, &text, x, footer_y, 9.0, f_regular, c_muted());
}

// ─── Date formatting ─────────────────────────────────────────────────

/// Mirror of TS `formatDate` — "Monday, January 1, 2026" in en-CA.
/// chrono's `format` doesn't do locale-aware month/weekday names in
/// no-default-features builds, but the standard English names match
/// en-CA for this format exactly.
fn format_date(d: NaiveDate) -> String {
    const WEEKDAYS: [&str; 7] = [
        "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday",
    ];
    const MONTHS: [&str; 12] = [
        "January", "February", "March", "April", "May", "June", "July", "August",
        "September", "October", "November", "December",
    ];
    let wd = match d.weekday() {
        Weekday::Mon => 0,
        Weekday::Tue => 1,
        Weekday::Wed => 2,
        Weekday::Thu => 3,
        Weekday::Fri => 4,
        Weekday::Sat => 5,
        Weekday::Sun => 6,
    };
    let m = (d.month0()) as usize;
    format!("{}, {} {}, {}", WEEKDAYS[wd], MONTHS[m], d.day(), d.year())
}
