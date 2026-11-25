//! The expression AST (DataSchema §3, §4).
//!
//! Atomic composites are the decorated atom `↑_ℤ⁺` with up to four mod slots.
//! `Expr` is the recursive expression tree. Symbols are referenced exclusively by
//! `SymbolId`; bound variables by `VarName`.

use serde::{Deserialize, Serialize};

use crate::ids::{SymbolId, VarName};

/// Up to four modifier slots on an atomic composite (DataSchema §3.2).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mods {
    pub l_sup: Option<Box<AtomicComposite>>,
    pub r_sup: Option<Box<AtomicComposite>>,
    pub l_sub: Option<Box<AtomicComposite>>,
    pub r_sub: Option<Box<AtomicComposite>>,
}

/// A decorated atom: a primary `SymbolId` plus 0–4 nested mods (DataSchema §3.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtomicComposite {
    pub primary: SymbolId,
    pub mods: Mods,
}

impl AtomicComposite {
    pub fn new(primary: SymbolId) -> Self {
        AtomicComposite {
            primary,
            mods: Mods::default(),
        }
    }
}

/// Reserved `SymbolId` for the mathematical equality relation (`=`).
///
/// Equality is not a corpus glyph declared in the symbol table; it is the
/// built-in equality of the target logic. The lexer/parser bind the ASCII `=`
/// operator to this reserved id so that equations render as Lean's `=` (`Eq`)
/// rather than as an unresolved `Apply` with a dummy head.
pub const EQ_SYM: SymbolId = u32::MAX - 1;

/// `∀ a b : 𝔈, body` — a binder group over one or more vars with a shared type
/// (DataSchema §3.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binder {
    pub vars: Vec<VarName>,
    pub ty: Expr,
}

/// A constructor pattern in a `Recursor`/`Induction` case (DataSchema §3.6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pattern {
    pub constructor: AtomicComposite,
    pub binders: Vec<VarName>,
    pub sub: Vec<Pattern>,
}

/// Kind of a bound operator (DataSchema §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BinderKind {
    Forall,
    Exists,
    Lambda,
    Integral,
    Limit,
}

/// A single expression node (DataSchema §4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Expr {
    /// Corpus symbol used directly (atom or atomic composite with mods).
    Sym(AtomicComposite),
    /// Local variable bound by an enclosing `Binder` or `Pattern`.
    Var(VarName),
    /// Application: head + argument list.
    Apply(AtomicComposite, Vec<Expr>),
    /// Bracketed group ⟨a,b⟩, ‖x‖, ⌜A⌝; `BracketId` is a `SymbolId`.
    Bracket(SymbolId, Vec<Expr>),
    /// Quantifier or operator binding variables.
    Binder(BinderKind, Vec<Binder>, Box<Expr>),
    /// `ρ x | pattern ↦ body | pattern ↦ body`.
    Recursor(Box<Expr>, Vec<(Pattern, Expr)>),
    /// Structured literals.
    Matrix(Vec<Vec<Expr>>),
    Vector(Vec<Expr>),
    Set(Vec<Expr>),
}

impl Expr {
    /// A `Binder` that is itself a bound variable reference.
    pub fn var(name: impl Into<VarName>) -> Expr {
        Expr::Var(name.into())
    }

    /// Visit all immediate child expressions (not through mods of an atomic).
    pub fn children(&self) -> Vec<&Expr> {
        let mut out: Vec<&Expr> = Vec::new();
        self.push_children(&mut out);
        out
    }

    fn push_children<'e>(&'e self, out: &mut Vec<&'e Expr>) {
        match self {
            Expr::Sym(_) | Expr::Var(_) => {}
            Expr::Apply(_, args) => out.extend(args.iter()),
            Expr::Bracket(_, elems) => out.extend(elems.iter()),
            Expr::Binder(_, binders, body) => {
                for b in binders {
                    out.push(&b.ty);
                }
                out.push(body.as_ref());
            }
            Expr::Recursor(scrut, cases) => {
                out.push(scrut.as_ref());
                for (_, body) in cases {
                    out.push(body);
                }
            }
            Expr::Matrix(rows) => out.extend(rows.iter().flatten()),
            Expr::Vector(els) => out.extend(els.iter()),
            Expr::Set(els) => out.extend(els.iter()),
        }
    }
}
