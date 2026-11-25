//! `qubitick-ctx` — symbol context (L2).
//!
//! A scope-aware snapshot of the symbol table. Answers "is this symbol visible in
//! this node's scope, and what is its D-block / event type?" Built from the corpus
//! DB via `qubitick-db`. Stateless lookups once constructed; the API layer holds a
//! `Ctx` instance that engines receive as `&Ctx`.

pub mod build;
pub mod context;
pub mod resolve;
pub mod scope;

pub use build::build_ctx;
pub use context::{Ctx, create_ctx_from_rows};
pub use resolve::{form_of, row_of, symbol_by_id};