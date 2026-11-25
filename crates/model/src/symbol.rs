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