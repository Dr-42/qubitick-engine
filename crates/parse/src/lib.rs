//! `qubitick-parse` — Lexer, structural parser, and semantic elaboration for `.qbit` (L3).
//!
//! The only crate that consumes `.qbit` source. Produces fully-typed `model`
//! ASTs. Depends on `model`, `db`, `ctx`.

pub mod atomic;
pub mod lexer;
pub mod node;
pub use node::row_refusal;
pub mod parser;
pub mod semantic;

pub use lexer::{FieldKind, LexError, Span, Token, lex};
pub use node::{cite_name, definition_arity, parse_content};
pub use semantic::{Diagnostic, ParseError, Severity, elab, parse_expr, Phase};
thread_local! {
    /// True while a STRUCTURAL pass reads a corpus that has no declarations yet.
    ///
    /// Ingestion lists a corpus before a single glyph is declared (to build the ledger
    /// and register symbols), and reads it again while the declarations are being written.
    /// No notation can be read in either state, so a proof row that does not parse there
    /// is not prose — it is a row no context could read yet, and the same row is read
    /// strictly once the declarations exist.
    static TOLERATE_PROSE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Mark the start/end of a structural pass (see [`TOLERATE_PROSE`]).
pub fn tolerate_prose(tolerate: bool) {
    TOLERATE_PROSE.with(|c| c.set(tolerate));
}

/// Whether the current pass tolerates a proof row it cannot read.
pub(crate) fn pros_are_tolerated() -> bool {
    TOLERATE_PROSE.with(|c| c.get())
}
