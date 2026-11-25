//! Central, data-driven symbol classifier table (code-only config).
//!
//! The engines (`emit`, `build`, `rewrite`, `parse`) previously named specific
//! glyph tex inline whenever they had to treat a symbol specially (the generated
//! tick/ℤ sorts and their constructors, arrow glyphs, binary operators, the iff
//! connectives, collector brackets, the nullity op, …). That scattered the "which
//! glyph does what" knowledge and meant every new generated sort or operator
//! required editing several engine files.
//!
//! This module is the SINGLE place that knowledge lives. A glyph's *existence*
//! is still always authoritative from the SQL DB / `.qbit` corpus (that is the
//! only source of truth for symbols themselves). Here we only RECLASSIFY tex
//! that already occurred in the corpus into semantic [`Facet`]s the engines
//! consult generically via [`texes_with`] / [`is_class`].
//!
//! ## Extending it
//!
//! - Add a new generated sort / operator / connective: add one row to
//!   [`PUBLIC_SYMBOL_CLASSES`] (or set a facet bit on an existing row). No
//!   engine change is needed — the engines query facets dynamically.
//! - Add a brand-new kind of special symbol that engines must recognise: add a
//!   new [`Facet`] bit, tag the relevant rows, then (and only then) teach one
//!   engine to ask `is_class(tex, facet)` / iterate `texes_with(facet)`.

/// A semantic facet a corpus symbol may carry. Facets are independent flags, so
/// one glyph (e.g. `\Delta`) can play several roles at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Facet {
    /// `\tau` — the generated tick sort.
    SortTau,
    /// `\mathbb{Z}` — the generated integer sort.
    SortZ,
    /// Any sort / type-carrier glyph (`\tau`, `\mathbb{Z}`, `\mathbb{Q}`, …).
    Sort,
    /// `\tau_0` — the nullary tick base constructor.
    TickNull,
    /// `^+` — the tick's own unary successor (single family).
    Succ,
    /// `^+`/`\uparrow`/`\downarrow` — heads whose operands are τ-typed.
    TauConsume,
    /// The full τ-term vocabulary: base + successor + the `tick` spellings.
    TickTerm,
    /// Generated-sort constructor tags used to recognise a constructor-headed arg.
    GenTag,
    /// Braced ℤ element constructors (`\mathbb{Z}_{0}`, `\mathbb{Z}^{+}`, `\mathbb{Z}^{-}`).
    ZCtor,
    /// ℤ element constructors including the corpus's no-brace variants.
    ZCtorDual,
    /// `\leftrightarrow` — the iff connective (peel + define).
    LeftIff,
    /// `\iff` — alternate iff spelling (define only).
    RightIff,
    /// Arrow glyphs rendered as Lean function types.
    Arrow,
    /// Arrow glyphs the theorem rewriter unwinds (adds `\Longrightarrow`).
    RewriteArrow,
    /// Arrow glyphs the rewrite-compile goal-unpacker treats as implications.
    CompileImp,
    /// Binary operator heads (compound definiendum handling).
    BinOp,
    /// `\varnothing` — empty-set carrier.
    Null,
    /// A collector / bracketing glyph (`\Vert`, `\ulcorner`, `\urcorner`).
    Collector,
    /// `\Delta` — the XOR nullity-encoding op (proof rewriting).
    Nullity,
    /// A relation between two terms (`\le_{\mathbb{Z}}`, `\subseteq`, `\in`, …).
    ///
    /// A relation binds LOOSER than the arithmetic it compares and looser than `=`:
    /// without this the flat infix fold read `D.BT.76`'s
    /// `a \cdot_{\mathbb{Z}} d \le_{\mathbb{Z}} c \cdot_{\mathbb{Z}} b` as
    /// `((a \cdot_Z d) \le_Z c) \cdot_Z b`, so the definition of the rational
    /// order applied `\cdot_Z` to a proposition. The parser asks for this facet
    /// through `is_logic_connective`, the same way it asks for the boolean-ring
    /// connectives.
    Relation,
}

impl Facet {
    const fn bits(self) -> u32 {
        1u32 << (self as u8)
    }
}

/// `tex -> set of facet bits`. Add rows here to register a special symbol.
///
/// `None` rows carry no bits; this two-column shape is kept so a future corpus
/// that stops using a glyph can drop the row without touching the engines.
pub const PUBLIC_SYMBOL_CLASSES: &[(&str, u32)] = &[
    // -- generated tick sort --
    ("\\tau", bits(Facet::SortTau) | bits(Facet::Sort)),
    ("\\tau_0", bits(Facet::TickNull) | bits(Facet::TickTerm) | bits(Facet::GenTag)),
    ("^+", bits(Facet::Succ) | bits(Facet::TauConsume) | bits(Facet::TickTerm) | bits(Facet::GenTag)),
    ("tick", bits(Facet::TickTerm)),
    ("\\uparrow", bits(Facet::TauConsume) | bits(Facet::TickTerm)),
    ("\\downarrow", bits(Facet::TauConsume) | bits(Facet::TickTerm)),
    // -- generated ℤ sort --
    ("\\mathbb{Z}", bits(Facet::SortZ) | bits(Facet::Sort)),
    ("\\mathbb{Q}", bits(Facet::Sort)),
    ("\\mathfrak{E}", bits(Facet::Sort)),
    ("\\mathbb{V}", bits(Facet::Sort)),
    ("\\mathbb{E}", bits(Facet::Sort)),
    ("\\mathbb{Z}_{0}", bits(Facet::ZCtor) | bits(Facet::ZCtorDual)),
    ("\\mathbb{Z}^{+}", bits(Facet::ZCtor) | bits(Facet::ZCtorDual)),
    ("\\mathbb{Z}^{-}", bits(Facet::ZCtor) | bits(Facet::ZCtorDual)),
    ("\\mathbb{Z}_0", bits(Facet::ZCtorDual)),
    ("\\mathbb{Z}^+", bits(Facet::ZCtorDual)),
    ("\\mathbb{Z}^-", bits(Facet::ZCtorDual)),
    // -- arrows --
    ("\\to", bits(Facet::Arrow) | bits(Facet::RewriteArrow) | bits(Facet::CompileImp)),
    ("\\rightarrow", bits(Facet::Arrow) | bits(Facet::RewriteArrow) | bits(Facet::CompileImp)),
    ("\\longrightarrow", bits(Facet::Arrow) | bits(Facet::RewriteArrow)),
    ("\\Longrightarrow", bits(Facet::RewriteArrow) | bits(Facet::CompileImp)),
    // -- iff connectives --
    ("\\leftrightarrow", bits(Facet::LeftIff)),
    ("\\iff", bits(Facet::RightIff)),
    // -- collector / bracketing glyphs --
    ("\\Vert", bits(Facet::Collector)),
    ("\\ulcorner", bits(Facet::Collector)),
    ("\\urcorner", bits(Facet::Collector)),
    // -- nullity / empty carrier --
    ("\\varnothing", bits(Facet::Null)),
    ("\\Delta", bits(Facet::BinOp) | bits(Facet::Nullity)),
    // -- binary operator heads --
    ("\\cap", bits(Facet::BinOp)),
    ("\\cup", bits(Facet::BinOp)),
    ("\\land", bits(Facet::BinOp)),
    ("\\lor", bits(Facet::BinOp)),
    ("\\cdot", bits(Facet::BinOp)),
    ("\\subseteq", bits(Facet::BinOp) | bits(Facet::Relation)),
    ("\\in", bits(Facet::BinOp) | bits(Facet::Relation)),
    ("\\ne", bits(Facet::BinOp) | bits(Facet::Relation)),
    ("\\le", bits(Facet::BinOp) | bits(Facet::Relation)),
    ("\\le_{\\tau}", bits(Facet::Relation)),
    ("\\le_{\\mathbb{Z}}", bits(Facet::Relation)),
    ("\\le_{\\mathbb{Q}}", bits(Facet::Relation)),
    ("\\cong", bits(Facet::Relation)),
    ("\\hookrightarrow", bits(Facet::Relation)),
    ("\\times", bits(Facet::BinOp)),
];

const fn bits(f: Facet) -> u32 {
    f.bits()
}

/// Every registered tex carrying `facet`, in table order.
pub fn texes_with(facet: Facet) -> impl Iterator<Item = &'static str> {
    PUBLIC_SYMBOL_CLASSES
        .iter()
        .filter(move |(_, bits)| *bits != 0 && *bits & facet.bits() != 0)
        .map(|(tex, _)| *tex)
}

/// Whether `tex` carries `facet`.
pub fn is_class(tex: &str, facet: Facet) -> bool {
    PUBLIC_SYMBOL_CLASSES
        .iter()
        .any(|(t, bits)| *t == tex && *bits != 0 && *bits & facet.bits() != 0)
}

/// Whether a tex is any of the facets (not used yet, provided for callers that
pub fn is_any_class(tex: &str) -> bool {
    PUBLIC_SYMBOL_CLASSES
        .iter()
        .any(|(t, bits)| *t == tex && *bits != 0)
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
pub const BINDER_KIND_CLASSES: &[(&str, &str)] = &[
    ("\\sum", "Integral"),
    ("\\prod", "Integral"),
    ("\\int", "Integral"),
    ("\\oint", "Integral"),
    ("\\lim", "Limit"),
];

/// The BinderKind label for a binder-shaped `\command` tex, or None if unknown.
pub fn binder_kind_of(tex: &str) -> Option<&'static str> {
    let cmd = tex.strip_prefix('\\')?;
    let head: String = cmd
        .chars()
        .take_while(|c| c.is_ascii_alphabetic() || *c == '@')
        .collect();
    if head.is_empty() {
        return None;
    }
    let key = format!("\\{head}");
    BINDER_KIND_CLASSES
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
}

pub const Z_CTOR_ALIASES: &[(&str, &str)] = &[
    ("\\mathbb{Z}_{0}", "mathbb_Z.zero"),
    ("\\mathbb{Z}_0", "mathbb_Z.zero"),
    ("\\mathbb{Z}^{+}", "mathbb_Z.plus"),
    ("\\mathbb{Z}^{-}", "mathbb_Z.minus"),
];

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
    fn facet_lists_reproduce_engine_predicates() {
        // recursor extra: SortTau ∪ GenTag = { \tau, \tau_0, ^+ }
        let mut rec: Vec<&str> = texes_with(Facet::SortTau)
            .chain(texes_with(Facet::GenTag))
            .collect();
        rec.sort();
        rec.dedup();
        assert_eq!(rec, vec!["\\tau", "\\tau_0", "^+"]);

        // tau_term = TickTerm
        assert_eq!(
            texes_with(Facet::TickTerm).collect::<Vec<_>>(),
            vec!["\\tau_0", "^+", "tick", "\\uparrow", "\\downarrow"]
        );

        // tau_consuming / is_tick_body = TauConsume
        assert_eq!(
            texes_with(Facet::TauConsume).collect::<Vec<_>>(),
            vec!["^+", "\\uparrow", "\\downarrow"]
        );
        // tick's own successor = Succ (single family)
        assert_eq!(
            texes_with(Facet::Succ).collect::<Vec<_>>(),
            vec!["^+"]
        );

        // genids = GenTag
        assert_eq!(
            texes_with(Facet::GenTag).collect::<Vec<_>>(),
            vec!["\\tau_0", "^+"]
        );

        // exclude_ctor_dual = ZCtorDual (six variants)
        assert_eq!(texes_with(Facet::ZCtorDual).count(), 6);
        assert!(texes_with(Facet::ZCtorDual).all(|t| t.starts_with("\\mathbb{Z}")));

        // binop set
        let binops: Vec<&str> = texes_with(Facet::BinOp).collect();
        assert!(binops.contains(&"\\cap"));
        assert!(binops.contains(&"\\Delta"));
        assert!(binops.contains(&"\\times"));
        assert!(!binops.contains(&"\\to"));

        // single-sort access
        assert!(is_class("\\tau", Facet::SortTau));
        assert!(!is_class("\\tau", Facet::Arrow));
        assert!(is_class("\\Delta", Facet::Nullity));
        assert!(!is_class("\\whatever", Facet::Arrow));

        // sorts (type carriers)
        for t in ["\\tau", "\\mathbb{Z}", "\\mathbb{Q}", "\\mathfrak{E}", "\\mathbb{V}", "\\mathbb{E}"] {
            assert!(is_class(t, Facet::Sort), "{t} is a Sort");
        }
        assert!(!is_class("\\cap", Facet::Sort));

        // arrow subsets are distinct per engine
        assert_eq!(texes_with(Facet::Arrow).count(), 3);
        assert_eq!(texes_with(Facet::RewriteArrow).count(), 4);
        assert_eq!(texes_with(Facet::CompileImp).count(), 3);
        assert!(!is_class("\\Longrightarrow", Facet::Arrow));
        assert!(is_class("\\Longrightarrow", Facet::RewriteArrow));
        assert!(is_class("\\Longrightarrow", Facet::CompileImp));
        assert!(!is_class("\\longrightarrow", Facet::CompileImp));
    }
}