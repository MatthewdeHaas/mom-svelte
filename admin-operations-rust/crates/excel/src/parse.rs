//! `calamine` → `ParsedWorkbook` adapter. Mirrors
//! `parseExcelBufferAsyncWithMeta` in `src/lib/excel/parse.ts`.
//!
//! Only sheet 1 is read for data (the real-client weekly workbook has
//! 7 sheets; sheets 2..N are packing-slip templates). `extra_sheet_count`
//! lets the orchestrator emit the `extra_sheets_ignored` sheet-level
//! warning. Date cells coerce to `YYYY-MM-DD` strings using the UTC
//! calendar date — matches the TS `date.getUTC{FullYear,Month,Date}()`
//! path so timezone drift can't creep in.
//!
//! Formula cells: calamine returns the *computed* value (no wrapper like
//! ExcelJS's `{formula, result}`), so Portion Size `=100*1.3` arrives as
//! `Data::Float(130.0)` and Qty formulas arrive as numbers. Header-based
//! `qty_formula_ignored` detection (see `headers.rs`) handles the
//! redundant-column case; no per-cell introspection needed.

use std::collections::HashMap;
use std::io::Cursor;

use calamine::{Data, ExcelDateTime, Reader, Xlsx};

/// A cell value after coercion. `Date` is kept as a pre-formatted
/// `YYYY-MM-DD` string because that's exactly what the Zod schema +
/// `ISO_DATE` regex expect; keeping it as `chrono::NaiveDate` would
/// force a re-stringify at validation and risk introducing a different
/// format than the TS path.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Null,
    Number(f64),
    String(String),
    /// Pre-formatted `YYYY-MM-DD`.
    Date(String),
    Bool(bool),
}

impl Cell {
    /// Convenience for the normaliser — returns a `&str` view for cells
    /// that carry string-shaped data (string + pre-formatted date).
    pub fn as_str_like(&self) -> Option<&str> {
        match self {
            Cell::String(s) | Cell::Date(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Cell::Null)
    }
}

/// One parsed workbook. `rows` is one map per non-empty data row,
/// keyed by the ORIGINAL header string (preserving leading/trailing
/// whitespace and case), matching the TS `Record<string, unknown>`
/// shape. `headers` is kept separately because the orchestrator needs
/// an ordered view for header analysis.
#[derive(Debug, Clone)]
pub struct ParsedWorkbook {
    pub headers: Vec<String>,
    pub rows: Vec<HashMap<String, Cell>>,
    pub extra_sheet_count: usize,
}

/// Parse an xlsx buffer. Returns an empty workbook (not an error) when
/// sheet 1 is missing — matches the TS fallback `if (!sheet) return ...`.
pub fn parse_workbook(bytes: &[u8]) -> Result<ParsedWorkbook, ParseError> {
    let cursor = Cursor::new(bytes);
    let mut wb: Xlsx<_> = calamine::open_workbook_from_rs(cursor)?;

    let sheet_names = wb.sheet_names();
    if sheet_names.is_empty() {
        return Ok(ParsedWorkbook {
            headers: Vec::new(),
            rows: Vec::new(),
            extra_sheet_count: 0,
        });
    }

    let extra_sheet_count = sheet_names.len().saturating_sub(1);
    let first = &sheet_names[0];
    let range = wb
        .worksheet_range(first)
        .map_err(|e| ParseError::Sheet(e.to_string()))?;

    let mut rows_iter = range.rows();

    // Header row — mirror TS `headers[colNumber - 1] = coerceCell(...) ?? ''`
    // so index-to-header alignment is byte-identical with the TS parser.
    let header_row = match rows_iter.next() {
        Some(r) => r,
        None => {
            return Ok(ParsedWorkbook {
                headers: Vec::new(),
                rows: Vec::new(),
                extra_sheet_count,
            });
        }
    };

    let headers: Vec<String> = header_row
        .iter()
        .map(|c| match coerce_cell(c) {
            Cell::String(s) => s,
            Cell::Date(s) => s,
            Cell::Number(n) => format_number(n),
            Cell::Bool(b) => b.to_string(),
            Cell::Null => String::new(),
        })
        .collect();

    // Data rows
    let mut rows: Vec<HashMap<String, Cell>> = Vec::new();
    for raw in rows_iter {
        let mut has_value = false;
        let mut record: HashMap<String, Cell> = HashMap::with_capacity(headers.len());
        for (i, cell) in raw.iter().enumerate() {
            let Some(header) = headers.get(i) else {
                continue;
            };
            if header.is_empty() {
                continue;
            }
            let coerced = coerce_cell(cell);
            if !coerced.is_null() {
                has_value = true;
            }
            record.insert(header.clone(), coerced);
        }
        if has_value {
            rows.push(record);
        }
    }

    Ok(ParsedWorkbook {
        headers,
        rows,
        extra_sheet_count,
    })
}

/// Coerce one calamine `Data` into our `Cell`. Rules mirror
/// `coerceCell` in `parse.ts`:
///
/// - Empty / Error → `Null`
/// - DateTime → `YYYY-MM-DD` (UTC calendar date)
/// - Float / Int → `Number`
/// - Bool → `Bool`
/// - String → `String` (empty string → `Null` to match TS `value === ''`)
fn coerce_cell(cell: &Data) -> Cell {
    match cell {
        Data::Empty | Data::Error(_) => Cell::Null,
        Data::String(s) => {
            if s.is_empty() {
                Cell::Null
            } else {
                Cell::String(s.clone())
            }
        }
        Data::Float(f) => Cell::Number(*f),
        Data::Int(i) => Cell::Number(*i as f64),
        Data::Bool(b) => Cell::Bool(*b),
        Data::DateTime(dt) => match coerce_datetime(dt) {
            Some(s) => Cell::Date(s),
            None => Cell::Null,
        },
        Data::DateTimeIso(s) | Data::DurationIso(s) => {
            // ISO strings already in `YYYY-MM-DD...` shape — pass the
            // date prefix through so the Zod regex accepts it.
            if s.len() >= 10 && is_iso_date_prefix(&s[..10]) {
                Cell::Date(s[..10].to_string())
            } else if s.is_empty() {
                Cell::Null
            } else {
                Cell::String(s.clone())
            }
        }
    }
}

/// Format an `ExcelDateTime` as `YYYY-MM-DD` using the UTC calendar
/// date — matches the TS `date.getUTC{FullYear,Month,Date}()` path.
fn coerce_datetime(dt: &ExcelDateTime) -> Option<String> {
    // calamine 0.26 exposes `as_datetime()` on `ExcelDateTime` which
    // returns a `chrono::NaiveDateTime`. Older methods differed; this
    // is the stable one.
    let naive = dt.as_datetime()?;
    Some(naive.date().format("%Y-%m-%d").to_string())
}

/// Numbers in header cells get stringified without a decimal when they
/// happen to be integral. Avoids `"7.0"` as a header name — not a case
/// we expect, but cheap insurance.
fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.is_finite() {
        format!("{}", n as i64)
    } else {
        format!("{}", n)
    }
}

fn is_iso_date_prefix(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[0].is_ascii_digit()
        && b[1].is_ascii_digit()
        && b[2].is_ascii_digit()
        && b[3].is_ascii_digit()
        && b[4] == b'-'
        && b[5].is_ascii_digit()
        && b[6].is_ascii_digit()
        && b[7] == b'-'
        && b[8].is_ascii_digit()
        && b[9].is_ascii_digit()
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("xlsx open failed: {0}")]
    Open(#[from] calamine::XlsxError),
    #[error("sheet read failed: {0}")]
    Sheet(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coerces_primitives() {
        assert!(matches!(coerce_cell(&Data::Empty), Cell::Null));
        assert!(matches!(coerce_cell(&Data::String("".into())), Cell::Null));
        assert_eq!(
            coerce_cell(&Data::String("Acme".into())),
            Cell::String("Acme".into())
        );
        assert_eq!(coerce_cell(&Data::Int(5)), Cell::Number(5.0));
        assert_eq!(coerce_cell(&Data::Float(1.5)), Cell::Number(1.5));
        assert_eq!(coerce_cell(&Data::Bool(true)), Cell::Bool(true));
    }

    #[test]
    fn iso_date_prefix_check() {
        assert!(is_iso_date_prefix("2026-06-08"));
        assert!(!is_iso_date_prefix("2026/06/08"));
        assert!(!is_iso_date_prefix("short"));
    }

    #[test]
    fn format_number_strips_integral_decimal() {
        assert_eq!(format_number(7.0), "7");
        assert_eq!(format_number(1.5), "1.5");
    }
}
