//! Ratatui render loop.
//!
//! Owns the terminal from `run()` until the user quits or writes. The
//! loop is blocking crossterm I/O — the enclosing tokio runtime is
//! single-threaded (`main` uses `flavor = "current_thread"`) so there's
//! no benefit to making this async.
//!
//! Rendering model: one `Table` widget. Columns = GLOBAL + one per
//! company. Rows = compile-time catalog. Each cell is a glyph from
//! `CellState::glyph`. The row under the cursor is highlighted; the
//! column under the cursor gets a `>` prefix in its header. A status
//! bar at the bottom shows the current coordinate + change count +
//! keyboard reference.

use std::io;

use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Terminal,
};

use crate::model::{CellState, Grid};

/// What the user did when the loop returned.
pub enum Outcome {
    QuitWithoutWriting,
    WriteRequested(Grid),
}

/// Enter raw mode, run the loop, and always restore the terminal on
/// the way out — even on panic — so a crashed TUI doesn't leave the
/// user's shell in alt-screen mode.
pub fn run(mut grid: Grid) -> Result<Outcome> {
    let mut stdout = io::stdout();
    enable_raw_mode().context("enable raw mode")?;
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("init terminal")?;

    let result = loop_inner(&mut terminal, &mut grid);

    // Always restore, then surface loop errors.
    disable_raw_mode().ok();
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .ok();
    terminal.show_cursor().ok();

    result
}

/// Inner loop — pulled out so the restore block above runs even on
/// an error path.
fn loop_inner<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    grid: &mut Grid,
) -> Result<Outcome> {
    let mut show_help = false;
    loop {
        terminal.draw(|f| draw(f, grid, show_help)).context("draw")?;

        // Blocking read is fine — we don't animate or auto-refresh.
        let ev = event::read().context("read event")?;
        let Event::Key(key) = ev else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                if show_help {
                    show_help = false;
                } else {
                    return Ok(Outcome::QuitWithoutWriting);
                }
            }
            KeyCode::Char('w') => {
                return Ok(Outcome::WriteRequested(std::mem::replace(
                    grid,
                    // Dummy — we won't touch `grid` after this; the
                    // real one has moved into the Outcome.
                    Grid {
                        rows: Vec::new(),
                        cols: Vec::new(),
                        cells: Default::default(),
                        original: Default::default(),
                        snapshot_flags: Default::default(),
                        cursor_row: 0,
                        cursor_col: 0,
                    },
                )));
            }
            KeyCode::Char('?') => {
                show_help = !show_help;
            }
            KeyCode::Char(' ') => {
                grid.cycle_current();
            }
            KeyCode::Char('g') => {
                grid.toggle_global_of_current_row();
            }
            KeyCode::Up => {
                if grid.cursor_row > 0 {
                    grid.cursor_row -= 1;
                }
            }
            KeyCode::Down => {
                if grid.cursor_row + 1 < grid.rows.len() {
                    grid.cursor_row += 1;
                }
            }
            KeyCode::Left => {
                if grid.cursor_col > 0 {
                    grid.cursor_col -= 1;
                }
            }
            KeyCode::Right => {
                if grid.cursor_col + 1 < grid.cols.len() {
                    grid.cursor_col += 1;
                }
            }
            _ => {}
        }
    }
}

/// One frame.
fn draw(f: &mut ratatui::Frame, grid: &Grid, show_help: bool) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(3)])
        .split(f.area());

    // --- Table ---
    let header_cells: Vec<Cell> = grid
        .cols
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let label = if i == grid.cursor_col {
                format!("> {}", c.label())
            } else {
                c.label().to_string()
            };
            Cell::from(label).style(Style::default().add_modifier(Modifier::BOLD))
        })
        .collect();
    // Prepend a label column for the flag name.
    let mut header: Vec<Cell> = Vec::with_capacity(grid.cols.len() + 1);
    header.push(Cell::from("FLAG").style(Style::default().add_modifier(Modifier::BOLD)));
    header.extend(header_cells);

    let rows: Vec<Row> = grid
        .rows
        .iter()
        .enumerate()
        .map(|(row_i, d)| {
            let mut cells: Vec<Cell> = Vec::with_capacity(grid.cols.len() + 1);
            let name_style = if row_i == grid.cursor_row {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            cells.push(Cell::from(d.name.clone()).style(name_style));

            for (col_i, _col) in grid.cols.iter().enumerate() {
                let state = grid
                    .cells
                    .get(&(d.name.clone(), col_i))
                    .copied()
                    .unwrap_or(CellState::Unset);
                let orig = grid
                    .original
                    .get(&(d.name.clone(), col_i))
                    .copied()
                    .unwrap_or(CellState::Unset);
                let changed = state != orig;

                let base = match state {
                    CellState::True => Style::default().fg(Color::Green),
                    CellState::False => Style::default().fg(Color::Red),
                    CellState::Unset => Style::default().fg(Color::DarkGray),
                };
                let styled = if changed {
                    base.add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                } else {
                    base
                };
                let cursored =
                    if row_i == grid.cursor_row && col_i == grid.cursor_col {
                        styled.bg(Color::DarkGray)
                    } else {
                        styled
                    };
                cells.push(Cell::from(state.glyph()).style(cursored));
            }
            Row::new(cells)
        })
        .collect();

    // Column widths: label column takes 24 chars, every data column 12.
    let mut widths: Vec<Constraint> = Vec::with_capacity(grid.cols.len() + 1);
    widths.push(Constraint::Length(24));
    for _ in &grid.cols {
        widths.push(Constraint::Length(12));
    }

    let table = Table::new(rows, widths)
        .header(Row::new(header).bottom_margin(1))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" flags — ↑↓←→ move · space cycle · g toggle global · w write · q quit · ? help "),
        );
    f.render_widget(table, chunks[0]);

    // --- Status bar ---
    let flag_name = grid
        .rows
        .get(grid.cursor_row)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| "—".to_string());
    let col_name = grid
        .cols
        .get(grid.cursor_col)
        .map(|c| c.label().to_string())
        .unwrap_or_else(|| "—".to_string());
    let changes = grid.change_count();
    let status = Line::from(vec![
        Span::styled(" cursor: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{flag_name} × {col_name}"),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw("    "),
        Span::styled("changes: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            changes.to_string(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw("    "),
        Span::styled(
            format!("value: {}", grid.current().glyph()),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    let bar = Paragraph::new(status).block(Block::default().borders(Borders::ALL));
    f.render_widget(bar, chunks[1]);

    // --- Help overlay ---
    if show_help {
        let area = chunks[0];
        let help = Paragraph::new(vec![
            Line::from("Keyboard"),
            Line::from(""),
            Line::from("  ↑ ↓ ← →   move cursor"),
            Line::from("  space     cycle cell  ·  →  ✓  →  ✗  →  ·"),
            Line::from("  g         toggle GLOBAL for the current flag row"),
            Line::from("  w         write migration + exit"),
            Line::from("  q / Esc   quit without writing"),
            Line::from("  ?         toggle this help"),
            Line::from(""),
            Line::from("Legend"),
            Line::from("  ·  unset — falls through to GLOBAL or compile-time default"),
            Line::from("  ✓  explicitly enabled"),
            Line::from("  ✗  explicitly disabled"),
            Line::from("  bold/underlined cells have been edited since snapshot"),
        ])
        .block(Block::default().borders(Borders::ALL).title(" help "));
        f.render_widget(help, area);
    }
}
