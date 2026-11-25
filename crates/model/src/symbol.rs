//! The D-block IR and symbol-kind axis (DataSchema §6).
//!
//! A `Definition`'s `d_block` carries the structured D-block content the parser
//! consults at every symbol use: kind, signature, form body, mod-role table,
//! elaboration rule, and application form.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::expr::{AtomicComposite, Expr, Pattern};
use crate::ids::SymbolId;

/// Proof-status axis of a symbol (DataSchema §6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SymbolKind {
    Formation,
    Ctor,
    Type,
    Notation,
    Macro,
    Primitive,
    BracketFn,
}

impl SymbolKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SymbolKind::Formation => "formation",
            SymbolKind::Ctor => "ctor",
            SymbolKind::Type => "type",
            SymbolKind::Notation => "notation",
            SymbolKind::Macro => "macro",
            SymbolKind::Primitive => "primitive",
            SymbolKind::BracketFn => "bracketfn",
        }
    }
}

/// The surface role a declaration states for its glyph (`Role:` in a corpus node).
///
/// The glyph line says where the operands sit; a `Role:` says what the glyph means
/// to the parser and the rewriter, and no shape can tell that: `a \cup b` and
/// `a \lor b` are both `x op y` with an iff-headed `Sema:`, yet one is an operation
/// on elements and the other is a connective. The engine carries no classification
/// of a mathematical glyph of its own: an operator the parser may use is one a
/// declaration states, and this is where a corpus states it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeclaredRole {
    /// Binary, `a op b`. Also derivable from the glyph line.
    Infix,
    /// Unary prefix, `op a`. Also derivable from the glyph line.
    Prefix,
    /// A bracket pair written around its operand (`\Vert x \Vert`).
    Collector,
    /// Binds looser than `=`: the propositional connectives.
    Connective,
    /// A relation between two terms, binding looser than the arithmetic it compares.
    Relation,
    /// The carrier's empty element.
    Null,
    /// The nullity operator the proof rewriter anchors an equality on.
    Nullity,
}

impl DeclaredRole {
    pub fn as_str(self) -> &'static str {
        match self {
            DeclaredRole::Infix => "infix",
            DeclaredRole::Prefix => "prefix",
            DeclaredRole::Collector => "collector",
            DeclaredRole::Connective => "connective",
            DeclaredRole::Relation => "relation",
            DeclaredRole::Null => "null",
            DeclaredRole::Nullity => "nullity",
        }
    }

    /// The role a `Role:` line states, or `None` for a word the surface does not
    /// know (the caller reports the word and the vocabulary it may have meant).
    pub fn parse(word: &str) -> Option<DeclaredRole> {
        Some(match word.trim().to_ascii_lowercase().as_str() {
            "infix" => DeclaredRole::Infix,
            "prefix" => DeclaredRole::Prefix,
            "collector" => DeclaredRole::Collector,
            "connective" => DeclaredRole::Connective,
            "relation" => DeclaredRole::Relation,
            "null" => DeclaredRole::Null,
            "nullity" => DeclaredRole::Nullity,
            _ => return None,
        })
    }

    /// The fixed vocabulary, for an error message that names what was expected.
    pub const VOCABULARY: [&'static str; 7] =
        ["infix", "prefix", "collector", "connective", "relation", "null", "nullity"];
}

impl std::fmt::Display for DeclaredRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Constructor declaration of a formation's `FormBody::Formation` (DataSchema §6.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CtorDecl {
    pub constructor: AtomicComposite,
    /// Argument types, by `SymbolId`.
    pub args: Vec<SymbolId>,
}

/// Definitional body of a macro / formation / bracket-fn (DataSchema §6.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormBody {
    /// `a := <expr>`
    MacroExpr(Expr),
    /// `τ := zero | next τ` — formation with constructors.
    Formation(Vec<CtorDecl>),
    /// `ρ x | pattern ↦ body | pattern ↦ body`
    Recursive(Box<Expr>, Vec<(Pattern, Expr)>),
    /// `⟨⟩`, `‖‖`, `⌜⌝`
    Bracket(String, String),
}

/// The role of a mod slot on an atomic composite (DataSchema §6.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ModRoleKind {
    Exponent,
    Index,
    Annotation,
    DerivativeOrder,
    Transpose,
    Inverse,
    BinderVar,
    Bound,
    SuperscriptMark,
}

/// A set of permitted roles for one mod slot (DataSchema §6.3).
///
/// The empty set means the slot is disallowed; a non-empty set means the parser
/// chooses a role from the structural shape of the mod.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleSet(pub BTreeSet<ModRoleKind>);

impl RoleSet {
    pub fn new(roles: &[ModRoleKind]) -> Self {
        RoleSet(roles.iter().copied().collect())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn allows(&self, role: ModRoleKind) -> bool {
        self.0.contains(&role)
    }
}

/// Declares which roles each mod slot may take for a symbol (DataSchema §6.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModRoleTable {
    pub l_sup: RoleSet,
    pub r_sup: RoleSet,
    pub l_sub: RoleSet,
    pub r_sub: RoleSet,
}

impl Default for ModRoleTable {
    fn default() -> Self {
        // Default (DataSchema §6.3): r_sup = {Exponent, Annotation},
        // r_sub = {Index, Annotation}, l_sup = l_sub = ∅.
        let r_sup = RoleSet::new(&[ModRoleKind::Exponent, ModRoleKind::Annotation]);
        let r_sub = RoleSet::new(&[ModRoleKind::Index, ModRoleKind::Annotation]);
        ModRoleTable {
            l_sup: RoleSet::default(),
            r_sup,
            l_sub: RoleSet::default(),
            r_sub,
        }
    }
}

/// How the parser emits an `Expr` node for a symbol at use-site (DataSchema §6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ElabRule {
    Identity,
    Apply,
    CurriedApply,
    Binder,
    Matrix,
    Vector,
    Set,
    Wrap,
}

impl ElabRule {
    pub fn as_str(self) -> &'static str {
        match self {
            ElabRule::Identity => "Identity",
            ElabRule::Apply => "Apply",
            ElabRule::CurriedApply => "CurriedApply",
            ElabRule::Binder => "Binder",
            ElabRule::Matrix => "Matrix",
            ElabRule::Vector => "Vector",
            ElabRule::Set => "Set",
            ElabRule::Wrap => "Wrap",
        }
    }
}

/// Application shape, consulted by `ElabRule::CurriedApply` (DataSchema §6.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ApplyForm {
    Curried,
    Uncurried,
    Dependent,
}

impl ApplyForm {
    pub fn as_str(self) -> &'static str {
        match self {
            ApplyForm::Curried => "Curried",
            ApplyForm::Uncurried => "Uncurried",
            ApplyForm::Dependent => "Dependent",
        }
    }
}

/// The nested definitional content of a D-node (DataSchema §6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DBlockIR {
    pub kind: SymbolKind,
    pub sig: Option<Expr>,
    pub form: Option<FormBody>,
    pub mod_roles: ModRoleTable,
    pub elab_rule: Option<ElabRule>,
    pub apply_form: Option<ApplyForm>,
    /// `Role:` — the classification the declaration states for its glyph.
    ///
    /// Absent means the declaration did not classify it, and the engine falls back
    /// to the shape of the glyph line (infix/prefix/collector) or to no operator
    /// role at all.
    pub role: Option<DeclaredRole>,
}

impl Default for DBlockIR {
    fn default() -> Self {
        DBlockIR {
            kind: SymbolKind::Notation,
            sig: None,
            form: None,
            mod_roles: ModRoleTable::default(),
            elab_rule: None,
            apply_form: None,
            role: None,
        }
    }
}

/// A symbol declaration carried as an extra symbol of a node (e.g. a formation's
/// constructor) (DataSchema §5.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymDecl {
    pub symbol: AtomicComposite,
    pub d_block: DBlockIR,
}