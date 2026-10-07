//! Business-logic queries, one module per domain. The TS counterpart
//! lives under `src/lib/server/queries/<domain>.ts`; keep names aligned
//! so a Node reviewer can jump between the two without a map.
//!
//! `users` is the one exception — there's no `queries/users.ts` on the
//! TS side (pre-port state had inline SQL in route handlers). The Rust
//! module is the sole module; see its own header for the full story.

pub mod allergens;
pub mod companies;
pub mod financials;
pub mod ingredients;
pub mod insights;
pub mod invites;
pub mod menus;
pub mod orders;
pub mod pagination;
pub mod persons;
pub mod recipe_imports;
pub mod recipes;
pub mod users;
