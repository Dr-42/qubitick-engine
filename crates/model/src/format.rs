//! The declaration FORMAT's alphabet.
//!
//! A corpus declares its own mathematics; these tokens are how it writes a
//! declaration, so the engine knows them and no corpus declares them:
//!
//! - `=` — a `Sema:` and a statement relate two sides with it (the reserved
//!   [`crate::EQ_SYM`]).
//! - the iff — a `Sema:` that defines a RELATION links the glyph to its body with
//!   `\leftrightarrow` (or its `\iff` spelling).
//! - the arrow — a `Type:` signature is written `\varphi \to \psi`, and a statement's
//!   implication is one of the arrow spellings.
//! - the binders — `\forall` and `\exists`, with the `:` that introduces their sort.
//!
//! Everything else a glyph can mean is a corpus's: an operator (with its fixity and
//! its precedence level), a relation, a collector, the empty element, the nullity
//! operator, a sort and its constructors all arrive through a declaration, and the
//! engine owns no list of glyphs that carry those meanings.

/// The iff a `Sema:` is linked with: the connective a relation-definition is written
/// around (`a \subseteq b \leftrightarrow a \cap b = a`).
pub const IFF: &[&str] = &["\\leftrightarrow", "\\iff"];

/// Arrow glyphs a `Type:` signature is written with — rendered as Lean function types.
pub const ARROW: &[&str] = &["\\to", "\\rightarrow", "\\longrightarrow"];

/// Arrow glyphs a proof rewriter unwinds as an implication: the [`ARROW`] set plus the
/// long double arrow a corpus writes a metatheoretic `implies` with.
pub const IMPLICATION: &[&str] = &[
    "\\to",
    "\\rightarrow",
    "\\longrightarrow",
    "\\Longrightarrow",
];

/// The quantifiers, with the field they are written before.
pub const BINDER: &[&str] = &["\\forall", "\\exists"];

/// The corner brackets a CITATION is written in: `[\ulcorner D.BT.01 b b a \urcorner]`.
/// They are the citation format's punctuation, like the square brackets around them.
pub const CITE_OPEN: &str = "\\ulcorner";
pub const CITE_CLOSE: &str = "\\urcorner";

/// Whether `tex` is the iff a `Sema:` is linked with.
pub fn is_iff(tex: &str) -> bool {
    IFF.contains(&tex)
}

/// Whether `tex` is an arrow a signature or a statement is written with.
pub fn is_arrow(tex: &str) -> bool {
    ARROW.contains(&tex) || IMPLICATION.contains(&tex)
}

/// Whether `tex` is an arrow the rewriter may unwind as an implication.
pub fn is_implication(tex: &str) -> bool {
    IMPLICATION.contains(&tex)
}

/// Whether `tex` is a quantifier.
pub fn is_binder(tex: &str) -> bool {
    BINDER.contains(&tex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_format_knows_its_own_tokens_and_nothing_else() {
        assert!(is_iff("\\leftrightarrow"));
        assert!(is_iff("\\iff"));
        assert!(is_arrow("\\to"));
        assert!(is_arrow("\\Longrightarrow"));
        assert!(is_implication("\\Longrightarrow"));
        // A corpus's own glyphs are NOT the format's, whatever they look like: the
        // engine has no opinion about them until a declaration says what they mean.
        for tex in [
            "\\cap", "\\cup", "\\Delta", "\\varnothing", "\\times", "\\le", "\\subseteq",
            "\\land", "\\lor", "\\cdot", "\\Vert", "\\tau", "^+", "\\mathbb{Z}", "×", "+",
        ] {
            assert!(!is_iff(tex), "{tex} is not the format's iff");
            assert!(!is_arrow(tex), "{tex} is not the format's arrow");
            assert!(!is_binder(tex), "{tex} is not a quantifier");
        }
    }
}