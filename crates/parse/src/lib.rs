//! `qubitick-parse` — Lexer, structural parser, and semantic elaboration for `.qbit` (L3).
//!
//! The only crate that consumes `.qbit` source. Produces fully-typed `model`
//! ASTs. Depends on `model`, `db`, `ctx`.

pub mod atomic;
pub mod lexer;
pub mod node;
pub mod parser;
pub mod semantic;

pub use lexer::{FieldKind, LexError, Span, Token, lex};
pub use node::{cite_name, definition_arity, parse_content};
pub use semantic::{Diagnostic, ParseError, Severity, elab, parse_expr, Phase};