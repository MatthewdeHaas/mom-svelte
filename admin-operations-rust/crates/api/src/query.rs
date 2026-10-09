//! Shared helpers for `axum::extract::Query` deserialization.
//!
//! `serde_urlencoded` (axum's `Query` backend) does not coerce the
//! strings `"true"` / `"false"` into Rust `bool`. The SvelteKit side
//! builds query strings via `URLSearchParams`, which stringifies every
//! value — so `?include_inactive=false` lands here as the string
//! `"false"` and the handler 400s with:
//!
//! > Failed to deserialize query string: invalid type: string "false",
//! > expected a boolean
//!
//! Rather than filter booleans TS-side, accept the standard HTTP form
//! on the Rust side. Attach [`bool_from_str`] via `deserialize_with`
//! on every bool query field (JSON bodies are untouched — serde_json
//! handles bool natively).

use serde::{Deserialize, Deserializer};

/// Parse a bool query parameter that arrives as a string.
///
/// Accepts `"true"` / `"false"` (case-insensitive) and `"1"` / `"0"`.
/// Missing fields should be paired with `#[serde(default)]` on the
/// struct field; this helper only runs when the parameter *is*
/// present. Any other value is a deserialize error so callers get a
/// 400 instead of silently defaulting.
pub fn bool_from_str<'de, D>(de: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(de)?;
    match s.to_ascii_lowercase().as_str() {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        other => Err(serde::de::Error::invalid_value(
            serde::de::Unexpected::Str(other),
            &"true/false/1/0",
        )),
    }
}
