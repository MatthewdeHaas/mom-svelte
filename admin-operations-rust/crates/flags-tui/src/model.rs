//! In-memory grid state + pending-edits tracker.
//!
//! The TUI builds a `Grid` once from the snapshot, mutates `edits` as
//! the user flips cells, and `emit` walks the delta against the
//! original to produce the migration file. Nothing here does I/O.
//!
//! Cell semantics:
//! - `Unset` — no row in `feature_flag_company` for that (flag, company).
//!   Resolves to the global default (or compile-time default if the
//!   flag isn't in `feature_flag` yet). Rendered as `·`.
//! - `True` / `False` — explicit per-company override. Rendered as
//!   `✓` / `✗`. In the GLOBAL column these edit `feature_flag.enabled_global`
//!   and `Unset` is impossible — fall-through to the compile-time
//!   default is a schema state we only see for brand-new catalog flags
//!   that haven't been inserted yet.

use std::collections::HashMap;

use uuid::Uuid;

use crate::snapshot::{FlagDescriptor, FlagRow, FlagSnapshot, OverrideRow, SnapshotCompanyRow};

/// Tri-state cell. The GLOBAL column only uses `True` / `False` (plus
/// the brand-new-catalog-flag case documented on `Grid::global_of`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellState {
    Unset,
    True,
    False,
}

impl CellState {
    /// Space-bar cycle: `Unset → True → False → Unset`.
    pub fn cycle(self) -> Self {
        match self {
            CellState::Unset => CellState::True,
            CellState::True => CellState::False,
            CellState::False => CellState::Unset,
        }
    }

    /// Glyph for the grid cell.
    pub fn glyph(self) -> &'static str {
        match self {
            CellState::Unset => "·",
            CellState::True => "✓",
            CellState::False => "✗",
        }
    }
}

/// A column in the grid. `Global` edits `feature_flag.enabled_global`
/// for the row's flag; `Company` edits `feature_flag_company`.
#[derive(Debug, Clone)]
pub enum Col {
    Global,
    Company {
        id: Uuid,
        name: String,
        /// Carried for parity with the server's `SnapshotCompanyRow`
        /// shape — the UI could grey out inactive companies, but
        /// doesn't today.
        #[allow(dead_code)]
        active: bool,
    },
}

impl Col {
    pub fn label(&self) -> &str {
        match self {
            Col::Global => "GLOBAL",
            Col::Company { name, .. } => name.as_str(),
        }
    }
}

/// Grid state. `rows` is the compile-time catalog (so flags added to
/// `crates/flags/src/lib.rs` but not yet in the DB show up); `cols`
/// is `[Global, …companies]`. `original` holds the snapshot's values
/// so emit can diff.
#[derive(Debug)]
pub struct Grid {
    pub rows: Vec<FlagDescriptor>,
    pub cols: Vec<Col>,
    /// Current (possibly edited) cell values keyed by (flag_name, col index).
    pub cells: HashMap<(String, usize), CellState>,
    /// Snapshot values at load time — never mutated after `from_snapshot`.
    pub original: HashMap<(String, usize), CellState>,
    /// Snapshot's `feature_flag` rows keyed by name — used by `emit`
    /// to decide which flags need an `INSERT ... ON CONFLICT DO NOTHING`
    /// seed row (catalog entries not present here are new).
    pub snapshot_flags: HashMap<String, FlagRow>,
    /// Cursor — index into `rows` × `cols`.
    pub cursor_row: usize,
    pub cursor_col: usize,
}

impl Grid {
    /// Build a grid from a snapshot. Catalog rows missing from the
    /// snapshot's `flags` get a `False` global cell (matching the
    /// compile-time default of every real flag today) — emit will
    /// insert the catalog row with that default on write.
    pub fn from_snapshot(snap: FlagSnapshot) -> Self {
        let mut cols = Vec::with_capacity(1 + snap.companies.len());
        cols.push(Col::Global);
        for SnapshotCompanyRow { id, name, active } in snap.companies {
            cols.push(Col::Company { id, name, active });
        }

        let snapshot_flags: HashMap<String, FlagRow> =
            snap.flags.into_iter().map(|f| (f.name.clone(), f)).collect();

        // Index overrides by (flag_name, company_id).
        let overrides_idx: HashMap<(String, Uuid), bool> = snap
            .overrides
            .into_iter()
            .map(|OverrideRow { flag_name, company_id, enabled, .. }| {
                ((flag_name, company_id), enabled)
            })
            .collect();

        let mut cells = HashMap::new();
        for (row_i, flag) in snap.catalog.iter().enumerate() {
            let flag_name = flag.name.to_string();

            // Column 0 — Global.
            let global = snapshot_flags
                .get(&flag_name)
                .map(|f| if f.enabled_global { CellState::True } else { CellState::False })
                // Brand-new catalog flag — no DB row yet. The compile-time
                // default lives on `FlagDescriptor::default`; mirror it so
                // the user sees the state they'll land in once the emitted
                // migration runs.
                .unwrap_or(if flag.default { CellState::True } else { CellState::False });
            cells.insert((flag_name.clone(), 0), global);

            // Columns 1.. — companies.
            for (col_i, col) in cols.iter().enumerate().skip(1) {
                let Col::Company { id, .. } = col else { unreachable!() };
                let state = match overrides_idx.get(&(flag_name.clone(), *id)) {
                    Some(true) => CellState::True,
                    Some(false) => CellState::False,
                    None => CellState::Unset,
                };
                cells.insert((flag_name.clone(), col_i), state);
            }
            let _ = row_i;
        }

        let original = cells.clone();

        Self {
            rows: snap.catalog,
            cols,
            cells,
            original,
            snapshot_flags,
            cursor_row: 0,
            cursor_col: 0,
        }
    }

    /// Number of (flag, col) cells whose state differs from the snapshot.
    pub fn change_count(&self) -> usize {
        self.cells
            .iter()
            .filter(|(k, v)| self.original.get(*k) != Some(*v))
            .count()
    }

    /// Convenience accessor for the cell under the cursor.
    pub fn current(&self) -> CellState {
        let flag = self.rows[self.cursor_row].name.to_string();
        *self.cells.get(&(flag, self.cursor_col)).unwrap_or(&CellState::Unset)
    }

    /// Cycle the cell under the cursor and store it. Returns the new
    /// state so the caller can log it.
    pub fn cycle_current(&mut self) -> CellState {
        let flag = self.rows[self.cursor_row].name.to_string();
        let key = (flag, self.cursor_col);
        let next = self.cells.get(&key).copied().unwrap_or(CellState::Unset).cycle();

        // GLOBAL column clamps `Unset` → `False` on cycle. A global
        // can't be "unset" — the schema has NOT NULL enabled_global.
        let next = if self.cursor_col == 0 && next == CellState::Unset {
            CellState::True
        } else {
            next
        };

        self.cells.insert(key, next);
        next
    }

    /// Toggle the GLOBAL column for the current row (hotkey `g`).
    pub fn toggle_global_of_current_row(&mut self) -> CellState {
        let flag = self.rows[self.cursor_row].name.to_string();
        let key = (flag, 0);
        let next = match self.cells.get(&key).copied().unwrap_or(CellState::False) {
            CellState::True => CellState::False,
            _ => CellState::True,
        };
        self.cells.insert(key, next);
        next
    }

    /// Resolve the global cell for a flag, used by `emit`.
    pub fn global_of(&self, flag: &str) -> CellState {
        self.cells
            .get(&(flag.to_string(), 0))
            .copied()
            .unwrap_or(CellState::False)
    }
}
