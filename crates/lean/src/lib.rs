//! `qubitick-lean` — Expr → Lean 4 surface text (L2).
//!
//! Pure rendering with caller-provided symbol resolution. Depends only on `model`.

pub mod render;
pub mod target;

pub use render::render;
pub use target::LeanTarget;