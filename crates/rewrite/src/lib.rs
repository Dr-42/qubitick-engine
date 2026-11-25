//! `qubitick-rewrite` — the proof compiler / rewrite engine (L4).
//!
//! Compiles a `Vec<ProofStep>` against a stated goal into a Lean proof term.
//! Depends on `model`, `pattern`, `ctx`, `lean`.

pub mod chain;
pub mod compile;
pub mod conclude;
pub mod diff;
pub mod engine;
pub mod justify;
pub mod discriminate;
pub mod env;
pub mod induction;
pub mod rewrite;
pub mod rules;
pub mod termctx;
pub use compile::compile_steps;
pub use diff::{RewriteRule, prove_eq_term};
pub use env::Env;
pub use rewrite::try_diff_apply;
pub use rules::{strip_binders, AxRule, RuleTable};