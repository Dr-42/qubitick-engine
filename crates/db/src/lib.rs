//! `qubitick-db` — SQLite-backed decomposed AST storage (L1).
//!
//! Owns the connection, schema, RAM hash-consed cache, and read/write/query
//! APIs. The only crate that touches SQLite. Depends only on `model`.

pub mod cache;
pub mod conn;
pub mod hash;
pub mod queries;
pub mod read;
pub mod schema;
pub mod write;

pub use cache::ExprCache;
pub use conn::{open, open_in_memory};
pub use hash::{hash_atomic, hash_expr, hash_pattern, hash_symbol};
pub use write::Db;