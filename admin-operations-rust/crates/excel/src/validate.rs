//! Per-row validation. Mirrors `ExcelRowSchema` + `validateRow` in
//! `src/lib/server/queries/imports.ts:31-146`.
//!
//! Error messages are kept byte-identical to the Zod messages — the
//! real-client test and the admin UI yellow strip both key off them,
//! so even wording tweaks are a parity break.

use chrono::NaiveDate;
use domain::ImportRowError;
use serde_json::Value;

use crate::headers::{
    F_AMOUNT, F_CLIENT, F_COMPANY, F_DAY, F_MEAL_CATEGORY, F_PORTION_SIZE, F_RECIPE,
    F_SERVING_DATE, F_SHIPMENT_DATE, F_STORAGE,
};
use crate::meals::MEAL_CATEGORIES;
use crate::normalise::NormalisedRow;

/// A fully validated row ready for reference resolution. Dates are
/// `NaiveDate` so the sqlx insert can bind them directly.
#[derive(Debug, Clone)]
pub struct ParsedRow {
    pub company: String,
    pub recipe: String,
    pub serving_date: NaiveDate,
    pub shipment_date: NaiveDate,
    pub meal_category: String,
    pub amount: f64,
    pub portion_size: Option<f64>,
    pub day: Option<i32>,
    /// `'f' | 'c' | None`.
    pub storage: Option<char>,
    pub client: Option<String>,
}

/// Validate one normalised row. Returns the typed row OR the full list
/// of field-level errors — never a partial result.
pub fn validate_row(
    row: &NormalisedRow,
    row_number: i64,
) -> Result<ParsedRow, Vec<ImportRowError>> {
    let mut errors: Vec<ImportRowError> = Vec::new();

    let company = require_nonempty_string(row, F_COMPANY, "Company", row_number, &mut errors);
    let recipe = require_nonempty_string(row, F_RECIPE, "Recipe", row_number, &mut errors);
    let serving_date =
        require_iso_date(row, F_SERVING_DATE, "Serving date", row_number, &mut errors);
    let shipment_date = require_iso_date(
        row,
        F_SHIPMENT_DATE,
        "Shipment date",
        row_number,
        &mut errors,
    );
    let meal_category = require_meal_category(row, row_number, &mut errors);
    let amount = require_positive_number(row, F_AMOUNT, "Amount", row_number, &mut errors);

    let portion_size =
        optional_positive_number(row, F_PORTION_SIZE, "Portion size", row_number, &mut errors);
    let day = optional_day(row, row_number, &mut errors);
    let storage = optional_storage(row, row_number, &mut errors);
    let client = optional_string(row, F_CLIENT);

    // superRefine: shipment_date <= serving_date.
    if let (Some(serving), Some(shipment)) = (serving_date, shipment_date) {
        if shipment > serving {
            errors.push(ImportRowError {
                row: row_number,
                field: Some(F_SHIPMENT_DATE.to_string()),
                message: "Shipment date cannot be after serving date".into(),
                raw_value: None,
                severity: None,
                suggestion: None,
            });
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(ParsedRow {
        company: company.unwrap(),
        recipe: recipe.unwrap(),
        serving_date: serving_date.unwrap(),
        shipment_date: shipment_date.unwrap(),
        meal_category: meal_category.unwrap(),
        amount: amount.unwrap(),
        portion_size,
        day,
        storage,
        client,
    })
}

/// Field is required + must be a non-empty string.
fn require_nonempty_string(
    row: &NormalisedRow,
    field: &'static str,
    label: &str,
    row_number: i64,
    errors: &mut Vec<ImportRowError>,
) -> Option<String> {
    match row.get(field) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::String(_)) => {
            errors.push(ImportRowError {
                row: row_number,
                field: Some(field.to_string()),
                message: format!("{} cannot be empty", label),
                raw_value: None,
                severity: None,
                suggestion: None,
            });
            None
        }
        _ => {
            errors.push(ImportRowError {
                row: row_number,
                field: Some(field.to_string()),
                message: format!("{} is required", label),
                raw_value: None,
                severity: None,
                suggestion: None,
            });
            None
        }
    }
}

/// Field must be an ISO date string (YYYY-MM-DD).
fn require_iso_date(
    row: &NormalisedRow,
    field: &'static str,
    label: &str,
    row_number: i64,
    errors: &mut Vec<ImportRowError>,
) -> Option<NaiveDate> {
    match row.get(field) {
        Some(Value::String(s)) if !s.is_empty() => match NaiveDate::parse_from_str(s, "%Y-%m-%d") {
            Ok(d) => Some(d),
            Err(_) => {
                errors.push(ImportRowError {
                    row: row_number,
                    field: Some(field.to_string()),
                    message: format!("{} must be in YYYY-MM-DD format", label),
                    raw_value: Some(s.clone()),
                    severity: None,
                    suggestion: None,
                });
                None
            }
        },
        _ => {
            errors.push(ImportRowError {
                row: row_number,
                field: Some(field.to_string()),
                message: format!("{} is required", label),
                raw_value: None,
                severity: None,
                suggestion: None,
            });
            None
        }
    }
}

/// `meal_category` must be one of `MEAL_CATEGORIES` after normalisation.
fn require_meal_category(
    row: &NormalisedRow,
    row_number: i64,
    errors: &mut Vec<ImportRowError>,
) -> Option<String> {
    match row.get(F_MEAL_CATEGORY) {
        Some(Value::String(s)) => {
            if MEAL_CATEGORIES.iter().any(|c| *c == s) {
                Some(s.clone())
            } else {
                errors.push(ImportRowError {
                    row: row_number,
                    field: Some(F_MEAL_CATEGORY.to_string()),
                    message: format!(
                        "Meal category must be one of: {}",
                        MEAL_CATEGORIES.join(", ")
                    ),
                    raw_value: Some(s.clone()),
                    severity: None,
                    suggestion: None,
                });
                None
            }
        }
        _ => {
            errors.push(ImportRowError {
                row: row_number,
                field: Some(F_MEAL_CATEGORY.to_string()),
                message: format!(
                    "Meal category must be one of: {}",
                    MEAL_CATEGORIES.join(", ")
                ),
                raw_value: None,
                severity: None,
                suggestion: None,
            });
            None
        }
    }
}

/// Required positive number.
fn require_positive_number(
    row: &NormalisedRow,
    field: &'static str,
    label: &str,
    row_number: i64,
    errors: &mut Vec<ImportRowError>,
) -> Option<f64> {
    match row.get(field) {
        Some(Value::Number(n)) => {
            let f = n.as_f64().unwrap_or(f64::NAN);
            if f.is_finite() && f > 0.0 {
                Some(f)
            } else {
                errors.push(ImportRowError {
                    row: row_number,
                    field: Some(field.to_string()),
                    message: format!("{} must be greater than zero", label),
                    raw_value: Some(n.to_string()),
                    severity: None,
                    suggestion: None,
                });
                None
            }
        }
        _ => {
            errors.push(ImportRowError {
                row: row_number,
                field: Some(field.to_string()),
                message: format!("{} is required", label),
                raw_value: None,
                severity: None,
                suggestion: None,
            });
            None
        }
    }
}

/// Optional positive number. `null`/missing → `None`; present and
/// non-positive → error.
fn optional_positive_number(
    row: &NormalisedRow,
    field: &'static str,
    label: &str,
    row_number: i64,
    errors: &mut Vec<ImportRowError>,
) -> Option<f64> {
    match row.get(field) {
        Some(Value::Number(n)) => {
            let f = n.as_f64().unwrap_or(f64::NAN);
            if f.is_finite() && f > 0.0 {
                Some(f)
            } else {
                errors.push(ImportRowError {
                    row: row_number,
                    field: Some(field.to_string()),
                    message: format!("{} must be greater than zero", label),
                    raw_value: Some(n.to_string()),
                    severity: None,
                    suggestion: None,
                });
                None
            }
        }
        _ => None,
    }
}

/// Optional day (1..=7, whole number).
fn optional_day(
    row: &NormalisedRow,
    row_number: i64,
    errors: &mut Vec<ImportRowError>,
) -> Option<i32> {
    match row.get(F_DAY) {
        Some(Value::Number(n)) => {
            let f = n.as_f64().unwrap_or(f64::NAN);
            if !f.is_finite() || f.fract() != 0.0 {
                errors.push(ImportRowError {
                    row: row_number,
                    field: Some(F_DAY.to_string()),
                    message: "Day must be a whole number".into(),
                    raw_value: Some(n.to_string()),
                    severity: None,
                    suggestion: None,
                });
                return None;
            }
            let i = f as i32;
            if !(1..=7).contains(&i) {
                errors.push(ImportRowError {
                    row: row_number,
                    field: Some(F_DAY.to_string()),
                    message: "Day must be between 1 and 7".into(),
                    raw_value: Some(n.to_string()),
                    severity: None,
                    suggestion: None,
                });
                return None;
            }
            Some(i)
        }
        _ => None,
    }
}

/// Optional storage. Normaliser already restricts the value to
/// `'f' | 'c' | null`, so a non-null string here is guaranteed to be
/// one of those two.
fn optional_storage(
    row: &NormalisedRow,
    _row_number: i64,
    _errors: &mut Vec<ImportRowError>,
) -> Option<char> {
    match row.get(F_STORAGE) {
        Some(Value::String(s)) if s == "f" => Some('f'),
        Some(Value::String(s)) if s == "c" => Some('c'),
        _ => None,
    }
}

/// Optional free-form string. Missing/null → `None`.
fn optional_string(row: &NormalisedRow, field: &'static str) -> Option<String> {
    match row.get(field) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mkrow(pairs: &[(&'static str, Value)]) -> NormalisedRow {
        pairs.iter().cloned().collect()
    }

    fn ok_row() -> NormalisedRow {
        mkrow(&[
            (F_COMPANY, json!("Acme")),
            (F_RECIPE, json!("Chicken Curry")),
            (F_SERVING_DATE, json!("2026-06-08")),
            (F_SHIPMENT_DATE, json!("2026-06-07")),
            (F_MEAL_CATEGORY, json!("Lunch")),
            (F_AMOUNT, json!(10.0)),
        ])
    }

    #[test]
    fn minimal_valid_row() {
        let r = validate_row(&ok_row(), 2).expect("valid row");
        assert_eq!(r.company, "Acme");
        assert_eq!(r.amount, 10.0);
        assert_eq!(r.serving_date, NaiveDate::from_ymd_opt(2026, 6, 8).unwrap());
        assert_eq!(r.portion_size, None);
        assert_eq!(r.storage, None);
    }

    #[test]
    fn missing_company_errors() {
        let mut row = ok_row();
        row.remove(F_COMPANY);
        let errs = validate_row(&row, 3).unwrap_err();
        assert_eq!(errs[0].field.as_deref(), Some(F_COMPANY));
        assert_eq!(errs[0].message, "Company is required");
    }

    #[test]
    fn bad_meal_category_errors() {
        let mut row = ok_row();
        row.insert(F_MEAL_CATEGORY, json!("Brunch"));
        let errs = validate_row(&row, 2).unwrap_err();
        assert_eq!(errs[0].field.as_deref(), Some(F_MEAL_CATEGORY));
        assert!(errs[0].message.starts_with("Meal category must be one of"));
    }

    #[test]
    fn shipment_after_serving_errors() {
        let mut row = ok_row();
        row.insert(F_SHIPMENT_DATE, json!("2026-06-10"));
        let errs = validate_row(&row, 2).unwrap_err();
        assert_eq!(errs[0].field.as_deref(), Some(F_SHIPMENT_DATE));
        assert_eq!(
            errs[0].message,
            "Shipment date cannot be after serving date"
        );
    }

    #[test]
    fn day_out_of_range_errors() {
        let mut row = ok_row();
        row.insert(F_DAY, json!(8));
        let errs = validate_row(&row, 2).unwrap_err();
        assert_eq!(errs[0].message, "Day must be between 1 and 7");
    }

    #[test]
    fn optional_storage_c_passes() {
        let mut row = ok_row();
        row.insert(F_STORAGE, json!("c"));
        let r = validate_row(&row, 2).unwrap();
        assert_eq!(r.storage, Some('c'));
    }
}
