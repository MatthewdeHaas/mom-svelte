//! Unit tests for the migration emitter.
//!
//! Pure-function tests on `render_body` — no tempdir needed. The
//! file-naming side (`next_migration_number` + `fs::write`) is kept
//! unexercised here; it's a thin wrapper over `fs::read_dir` and
//! would just be testing the stdlib.
//!
//! Cases, one per `#[test]`:
//! - no changes → body contains only the header, no SQL
//! - global flipped true→false → one UPDATE
//! - per-company override added → one upsert INSERT ... ON CONFLICT DO UPDATE
//! - per-company override cleared → one DELETE
//! - new catalog flag not in snapshot → one INSERT ... ON CONFLICT DO NOTHING

use std::collections::HashMap;

use chrono::{TimeZone, Utc};
use flags_tui::emit::render_body;
use flags_tui::model::{CellState, Col, Grid};
use flags_tui::snapshot::{FlagDescriptor, FlagRow};
use uuid::Uuid;

/// Build a `FlagRow` with sensible defaults — only the fields emit cares
/// about are meaningful here.
fn row(name: &str, enabled_global: bool) -> FlagRow {
    FlagRow {
        name: name.to_string(),
        description: String::new(),
        enabled_global,
        created_at: Utc.timestamp_opt(0, 0).unwrap(),
        updated_at: Utc.timestamp_opt(0, 0).unwrap(),
    }
}

/// Minimal grid builder — one catalog flag, GLOBAL column, no companies.
/// Caller mutates `cells` to simulate edits; `original` is the "snapshot".
fn grid_for(flag_name: &str, snapshot_global: bool, catalog_default: bool) -> Grid {
    let desc = FlagDescriptor {
        name: flag_name.to_string(),
        description: "test flag".to_string(),
        default: catalog_default,
    };
    let snapshot_flags = {
        let mut m = HashMap::new();
        m.insert(flag_name.to_string(), row(flag_name, snapshot_global));
        m
    };
    let global_cell = if snapshot_global {
        CellState::True
    } else {
        CellState::False
    };
    let mut cells = HashMap::new();
    cells.insert((flag_name.to_string(), 0), global_cell);
    Grid {
        rows: vec![desc],
        cols: vec![Col::Global],
        cells: cells.clone(),
        original: cells,
        snapshot_flags,
        cursor_row: 0,
        cursor_col: 0,
    }
}

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 9, 12, 0, 0).unwrap()
}

#[test]
fn no_changes_renders_header_only() {
    let grid = grid_for("insights", false, false);
    let body = render_body(&grid, now(), "http://example", "op-123");
    // Header present.
    assert!(body.contains("flags migration"));
    // No SQL sections.
    assert!(!body.contains("INSERT"), "unexpected INSERT: {body}");
    assert!(!body.contains("UPDATE"), "unexpected UPDATE: {body}");
    assert!(!body.contains("DELETE"), "unexpected DELETE: {body}");
}

#[test]
fn global_flip_emits_single_update() {
    let mut grid = grid_for("insights", false, false);
    // Snapshot is False; user flipped to True.
    grid.cells
        .insert(("insights".to_string(), 0), CellState::True);

    let body = render_body(&grid, now(), "http://example", "op-123");
    assert!(body.contains("-- Global default changes:"));
    assert!(body.contains("UPDATE feature_flag SET enabled_global = TRUE WHERE name = 'insights';"));
    // No per-company section.
    assert!(
        !body.contains("feature_flag_company"),
        "unexpected overrides: {body}"
    );
}

#[test]
fn override_add_emits_single_upsert() {
    let company_id = Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap();

    // Grid: one flag, GLOBAL + one company column; snapshot has no override.
    let desc = FlagDescriptor {
        name: "insights".to_string(),
        description: "x".to_string(),
        default: false,
    };
    let snapshot_flags = {
        let mut m = HashMap::new();
        m.insert("insights".to_string(), row("insights", false));
        m
    };
    let mut cells = HashMap::new();
    cells.insert(("insights".to_string(), 0), CellState::False);
    cells.insert(("insights".to_string(), 1), CellState::Unset);
    let original = cells.clone();
    // User explicitly enables for this company.
    cells.insert(("insights".to_string(), 1), CellState::True);

    let grid = Grid {
        rows: vec![desc],
        cols: vec![
            Col::Global,
            Col::Company {
                id: company_id,
                name: "Acme".to_string(),
                active: true,
            },
        ],
        cells,
        original,
        snapshot_flags,
        cursor_row: 0,
        cursor_col: 0,
    };

    let body = render_body(&grid, now(), "http://example", "op-123");
    assert!(body.contains("-- Per-company overrides (upserts + deletes):"));
    assert!(body.contains("INSERT INTO feature_flag_company (flag_name, company_id, enabled)"));
    assert!(body.contains("'11111111-2222-3333-4444-555555555555'::uuid"));
    assert!(body
        .contains("ON CONFLICT (flag_name, company_id) DO UPDATE SET enabled = EXCLUDED.enabled"));
    assert!(!body.contains("DELETE FROM"), "unexpected DELETE: {body}");
}

#[test]
fn override_clear_emits_single_delete() {
    let company_id = Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap();
    let desc = FlagDescriptor {
        name: "insights".to_string(),
        description: "x".to_string(),
        default: false,
    };
    let snapshot_flags = {
        let mut m = HashMap::new();
        m.insert("insights".to_string(), row("insights", false));
        m
    };
    let mut cells = HashMap::new();
    cells.insert(("insights".to_string(), 0), CellState::False);
    // Snapshot had an explicit True for this company.
    cells.insert(("insights".to_string(), 1), CellState::True);
    let original = cells.clone();
    // User cleared it.
    cells.insert(("insights".to_string(), 1), CellState::Unset);

    let grid = Grid {
        rows: vec![desc],
        cols: vec![
            Col::Global,
            Col::Company {
                id: company_id,
                name: "Acme".to_string(),
                active: true,
            },
        ],
        cells,
        original,
        snapshot_flags,
        cursor_row: 0,
        cursor_col: 0,
    };

    let body = render_body(&grid, now(), "http://example", "op-123");
    assert!(body.contains("DELETE FROM feature_flag_company"));
    assert!(body.contains("flag_name = 'insights'"));
    assert!(body.contains("company_id = '11111111-2222-3333-4444-555555555555'::uuid"));
    // No upsert line for this flag/company.
    assert!(
        !body.contains("ON CONFLICT (flag_name, company_id)"),
        "unexpected upsert: {body}"
    );
}

#[test]
fn new_catalog_flag_emits_insert_with_on_conflict_do_nothing() {
    // Catalog has `new_flag` but the snapshot's `flags` map doesn't —
    // mirrors the "flag was just added to crates/flags/src/lib.rs"
    // scenario. Emitter must seed it with an ON CONFLICT DO NOTHING.
    let desc = FlagDescriptor {
        name: "new_flag".to_string(),
        description: "A brand new flag.".to_string(),
        default: false,
    };
    let snapshot_flags: HashMap<String, FlagRow> = HashMap::new();
    let mut cells = HashMap::new();
    // No snapshot row → `from_snapshot` would have planted the
    // compile-time default (False) in the global cell; mirror that.
    cells.insert(("new_flag".to_string(), 0), CellState::False);
    let original = cells.clone();

    let grid = Grid {
        rows: vec![desc],
        cols: vec![Col::Global],
        cells,
        original,
        snapshot_flags,
        cursor_row: 0,
        cursor_col: 0,
    };

    let body = render_body(&grid, now(), "http://example", "op-123");
    assert!(body.contains("-- New flags discovered in catalog but not yet in DB:"));
    assert!(body.contains("INSERT INTO feature_flag (name, description, enabled_global)"));
    assert!(body.contains("VALUES ('new_flag', 'A brand new flag.', FALSE)"));
    assert!(body.contains("ON CONFLICT (name) DO NOTHING"));
    // User didn't flip anything → no Section 2 UPDATE needed.
    assert!(
        !body.contains("-- Global default changes:"),
        "unexpected UPDATE: {body}"
    );
}
