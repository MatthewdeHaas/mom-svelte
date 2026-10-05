//! Business-logic queries, one module per domain. The TS counterpart
//! lives under `src/lib/server/queries/<domain>.ts`; keep names aligned
//! so a Node reviewer can jump between the two without a map.

pub mod companies;
pub mod ingredients;
pub mod orders;
pub mod pagination;
pub mod recipes;
