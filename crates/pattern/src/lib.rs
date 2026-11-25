//! `qubitick-pattern` — structural pattern-matching primitives (L1).
//!
//! Context-free syntactic operations on `model::Expr`, shared by `rewrite` and
//! `autocite`. No dependency on `ctx`, `db`, or any engine.

pub mod bind;
pub mod find;
pub mod idents;
pub mod norm;
pub mod subst;
pub mod subterms;

pub use bind::{bind_pat, Bindings};
pub use find::{find_all, find_pos, Pos};
pub use idents::expr_idents;
pub use norm::{expr_norm_eq, expr_strip_parens};
pub use subst::{expr_subst, expr_subst_var};
pub use subterms::{expr_depth, expr_subterms, replace_at, walk};