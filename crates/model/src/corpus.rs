//! The classification a DECLARATION may state about its glyph.
//!
//! A corpus declares its own mathematics, and the engine carries the declarations
//! faithfully. [`Facet`] is the small vocabulary a `Role:` line speaks — `relation`,
//! `connective`, `collector`, `null`, `nullity` — plus the binary-operator bit that
//! the glyph line's fixity implies. Ingestion writes those bits per symbol and the
//! engines read them from the DB (`Ctx::has_facet`, `Ctx::declared_with_facet`).
//!
//! There is no glyph list here. Nothing in the engine says `\cap` is an operator or
//! `\mathbb{Z}` is a sort: that sentence belongs to the corpus, and a corpus that
//! writes a glyph declares it first. The tokens a declaration is written WITH — the
//! `=`, the iff that links a `Sema:`, the arrow of a `Type:` signature, the
//! quantifiers — are the declaration FORMAT's and live in [`crate::format`].
//!
//! ## Extending it
//!
//! - A classification a corpus may state: add a [`crate::DeclaredRole`] variant in
//!   `symbol.rs`, read it in the parser, persist its facet in ingestion, and ask for
//!   it through `Ctx::has_facet`.
//! - A glyph: never here. A corpus declares it.

/// A classification a declaration states about its glyph. Facets are independent
/// flags, so one glyph can play several roles at once (a binary operator that is also
/// the nullity operator).
///
/// Every bit is set by ingestion from the corpus's own declaration; nothing else can
/// set one, and no glyph is classified without one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Facet {
    /// A binary operator head: set from the fixity a declaration states (`infix`).
    BinOp,
    /// A propositional connective: a glyph whose own declaration says
    /// `Role: connective`, binding looser than `=` and below the relations.
    Connective,
    /// A relation between two terms — a glyph whose own declaration says
    /// `Role: relation`.
    ///
    /// A relation binds looser than the arithmetic it compares and looser than `=`,
    /// which is what keeps `a \cdot_Z d \le_Z c \cdot_Z b` from folding left.
    Relation,
    /// A collector / bracketing glyph (`Role: collector`).
    Collector,
    /// The carrier's empty element (`Role: null`).
    Null,
    /// The nullity operator a proof rewrites through (`Role: nullity`).
    Nullity,
}

impl Facet {
    /// The single-bit mask for this facet. Public so callers outside the crate
    /// (e.g. `Ctx::has_facet`) can test the bitmasks a declaration writes.
    pub const fn bits(self) -> u32 {
        1u32 << (self as u8)
    }
}

/// The punctuation alphabet that can be an OPERATOR in the corpus surface.
///
/// Structural punctuation — brackets `()[]{}`, comma `,`, period `.`, semicolon `;`,
/// quotes `'"`, and backslash `\` — is EXCLUDED because it is GRAMMAR, not notation.
/// Registering a bracket as a symbol would let the parser treat grouping syntax as
/// a declarable operator, which breaks expression structure.
pub const OPERATOR_PUNCT: &[char] = &['+', '-', '*', '/', '=', '<', '>', '!', '%', '&', '|', '~', '^', ':', '×'];

/// The punctuation a corpus may write as a glyph: an OPERATOR.
///
/// The ASCII table above is the fast path and the spelling of the common cases; every other
/// script's operator is the same question asked of a different codepoint — `⊙`, `⊕`, `∘`, `⊢`
/// are notation exactly as `+` is — so a character that is neither a letter-ish nor
/// whitespace is operator-shaped, with the grammar left out (brackets, quotes, separators).
///
/// This is not a glyph table: it says which characters can BE a glyph, never which glyph means
/// what. The engine still knows no mathematical symbol (invariant 1).
#[inline]
pub fn is_operator_punct(c: char) -> bool {
    if OPERATOR_PUNCT.contains(&c) {
        return true;
    }
    if c.is_ascii() {
        // ASCII punctuation the table does not name is grammar or prose punctuation.
        return false;
    }
    if c.is_alphanumeric() || c.is_whitespace() || c.is_control() {
        return false;
    }
    !matches!(
        c,
        '\u{2018}' | '\u{2019}' | '\u{201C}' | '\u{201D}' | '\u{00AB}' | '\u{00BB}'
            | '\u{2308}' | '\u{2309}' | '\u{230A}' | '\u{230B}'   // ceilings
            | '\u{27E8}' | '\u{27E9}' | '\u{27EA}' | '\u{27EB}'   // angle brackets
            | '\u{27E6}' | '\u{27E7}' | '\u{301A}' | '\u{301B}'   // brackets
            | '\u{FF08}' | '\u{FF09}' | '\u{FF0C}' | '\u{FF1B}'   // fullwidth punctuation
    )
}

/// Count how many balanced `{ … }` groups immediately follow `i` (positioned just
/// after a `\command` word) WITHOUT consuming them.
///
/// This is the single source of truth for what a command's braced groups mean:
///
/// - 0: a bare command (`\tau`);
/// - 1: a single decoration group made into the symbol tex (`\mathbb{Z}`, `\hat{B}`) — the cmd +
///   `{…}` is ONE symbol;
/// - ≥2: a braced-ARGUMENT macro (`\frac{n}{d}`) whose `{…}` groups are APPLICATION arguments, so
///   the bare command heads the application.
///
/// The lexer, the ingest symbol-registrar, and the parser ALL consult this same
/// count, so the loader stays data-driven and the three never disagree on what a
/// command's braces mean.
pub fn braced_group_count(src: &[u8], mut i: usize, n: usize) -> usize {
    let mut count = 0usize;
    while i < n && src[i] == b'{' {
        let mut depth = 0usize;
        while i < n {
            let c = src[i];
            if c == b'{' {
                depth += 1;
            } else if c == b'}' {
                depth -= 1;
                if depth == 0 {
                    i += 1;
                    break;
                }
            }
            i += 1;
        }
        count += 1;
    }
    count
}

/// ℤ-element glyph tex mapped to the Lean constructor it aliases. This is the
/// ℤ-inductive's constructor surface (a `plus`/`minus` takes one `tau` arg); a
/// future generated sort adds its own alias table here, never in the engine.
/// Binder KIND for a binder-shaped operator, keyed by its `\command` head. This is
/// a DATA TABLE (like `Z_CTOR_ALIASES`), not engine logic: a binder glyph's
/// kind is a semantic label the loader records for it. Anything absent is a
/// plain fold, which renders as Lean `fun v : T => body` (`Lambda`).
///
/// The corpus currently only uses `\bigcup` (a fold -> Lambda); `\sum`/`\prod`/
/// `\int` are folds over a bound variable too, but are labelled `Integral` so the
/// BinderKind axis is exercised; `\lim` is a `Limit`.
/// `\bigcup`/`\bigcap` are deliberately ABSENT: a set-fold renders as the valid
/// Lean `fun v : T => body` (Lambda), whereas the `Integral` kind is the ASCII
/// placeholder LeanEmission §7.1 reserves for summation/integration glyphs.
/// The glyph a definition's `Sema:` defines.
///
/// A `Sema:` states the defined glyph in one of two shapes, and both peel the
/// same way:
///
/// - an equation `f(a, b) = …`, whose LEFT side IS the definiendum; or
/// - a relation `a \le_{\tau} b \leftrightarrow …`, where the connective only
///   links the glyph to its meaning, so the glyph is again the head of the
///   relation's left side.
///
/// The DB loader and the parser must peel the same way, or one definition has
/// two symbol ids: the one the DB rebuilds from the symbol table (the glyph as
/// it is WRITTEN, `\le_{\tau}`) and the one the parser derives from the glyph
/// line (the base control word, `\le`). A module that unfolds a theorem's
/// relational glyph then looks the definition up under an id nothing
/// registered, and every proof row that rewrites by that relation fails with
/// `citation does not match` — which is exactly what an era emitted as one
/// module hit, at every scope width.
///
/// `is_left_iff` recognises the linking connective in the caller's symbol table
/// (`\leftrightarrow`); the model crate carries no `Ctx` to ask.
pub fn definiendum_head(e: &crate::Expr, is_left_iff: impl Fn(u32) -> bool) -> u32 {
    let mut target = match e {
        crate::Expr::Apply(h, args) if h.primary == crate::EQ_SYM && args.len() == 2 => &args[0],
        other => other,
    };
    while let crate::Expr::Apply(op, args) = target {
        // Peel a top-level `=`/`\leftrightarrow` that merely links the compound
        // definiendum to its body (the glyph is the head of `args[0]`).
        if (op.primary == crate::EQ_SYM || is_left_iff(op.primary))
            && args.len() == 2
            && matches!(&args[0], crate::Expr::Apply(_, _))
        {
            target = &args[0];
        } else {
            break;
        }
    }
    match target {
        crate::Expr::Apply(h, _) => h.primary,
        crate::Expr::Sym(ac) => ac.primary,
        _ => 0,
    }
}

/// Whether a definition's `Sema:` is a postulate about OPERANDS rather than a
/// definition of the operator that heads its defining equation.
///
/// `D.BT.43 FrameLeg1` declares `e_1` with the Sema `e_1 \cap e_2 = \varnothing`.
/// Its defining equation is headed by `cap`, a corpus operator — and no reading of
/// it defines `cap`: the node says something ABOUT two operands, so it declares
/// nothing at all.
///
/// Read the BASE equation (a recursive tick operator joins several with `\land`,
/// and only the first is the defining one), because the shape to catch is narrow:
/// `D.BT.56`'s `a \times_{\tau} \tau_0 = \tau_0` IS a definition of `\times_{\tau}`,
/// and `\tau`-family operators are not in [`Facet::BinOp`].
///
/// The alternative — letting the node claim the operator — is what broke the shared
/// ground: `D.43` claimed `cap`, the frame then left `cap` to it, and every class
/// axiom in the same ground failed with `Unknown constant CoeFun`.
pub fn is_operand_postulate(e: &crate::Expr, is_binop: impl Fn(u32) -> bool) -> bool {
    /// The left side of the base `=` of a (possibly `\land`-joined) Sema.
    fn base_eq(e: &crate::Expr) -> Option<&crate::Expr> {
        match e {
            crate::Expr::Apply(h, args) if args.len() == 2 => {
                if h.primary == crate::EQ_SYM {
                    Some(&args[0])
                } else {
                    base_eq(&args[0])
                }
            }
            _ => None,
        }
    }
    match base_eq(e) {
        Some(crate::Expr::Apply(h, _)) => is_binop(h.primary),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_facet_vocabulary_is_the_declarations_and_nothing_more() {
        // Six classifications, all of them things a `Role:` line can say (BinOp is
        // what `infix` implies). A seventh bit can only arrive with a new
        // `DeclaredRole`, i.e. with a surface a corpus can actually write.
        assert_eq!(
            [
                Facet::BinOp,
                Facet::Connective,
                Facet::Relation,
                Facet::Collector,
                Facet::Null,
                Facet::Nullity,
            ]
            .iter()
            .map(|f| f.bits())
            .collect::<Vec<_>>(),
            vec![1, 2, 4, 8, 16, 32]
        );
    }

    #[test]
    fn only_format_punctuation_can_head_a_declared_operator() {
        // What a corpus may use as an operator GLYPH is the format's business (a
        // bracket or a period is grammar); which of them a corpus actually declares
        // is the corpus's.
        assert!(is_operator_punct('+'));
        assert!(is_operator_punct('×'));
        assert!(is_operator_punct('*'));
        // A caret is punctuation the format accepts, which is why `^+` can be one
        // glyph; whether a corpus declares one is the corpus's sentence.
        assert!(is_operator_punct('^'));
        assert!(!is_operator_punct('('));
        assert!(!is_operator_punct(','));
    }
}
