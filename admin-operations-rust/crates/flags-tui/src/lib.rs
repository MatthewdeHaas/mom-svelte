//! Library surface so integration tests under `tests/` can import the
//! modules without duplicating them. `main.rs` consumes the same
//! modules through this crate name.

pub mod emit;
pub mod model;
pub mod snapshot;
pub mod ui;
