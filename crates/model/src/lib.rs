//! `qubitick-model` — the canonical data types (L0).
//!
//! Pure data types with no logic and no dependencies beyond `serde`. This crate
//! is the single home of the corpus data model — the schema the DB, the emit
//! stages and the client all agree on. It is split by concern:
//!
//! - [`ids`] — `SymbolId`, `NodeId`, `Era`, `Strata`, `Scope`, `VarName`
//! - [`expr`] — `Expr`, `AtomicComposite`, `Mods`, `Binder`, `Pattern`
//! - [`symbol`] — `SymbolKind`, `DBlockIR`, `FormBody`, `ModRoleTable`,
//!   `ElabRule`, `ApplyForm`, `SymDecl`
//! - [`node`] — `Node`, `NodeBase`, `Axiom`, `Definition`, `Theorem`,
//!   `Calibration`, `Census`
//! - [`proof`] — `ProofStep`, `ChainRow`, `ChainJust`, `JustRef`, `CiteName`,
//!   `CaseArm`, `IndCase`
//! - [`tag`] — `PhysOrMath`, `CensusCharter`, `StepRel`
//! - [`diag`] — `Diagnostic`, `Severity`, `Phase`, `Span`: the one shape the
//!   CLI, the API and the client all report in

pub mod corpus;
pub mod diag;
pub mod expr;
pub mod ids;
pub mod node;
pub mod proof;
pub mod symbol;
pub mod tag;

pub use diag::{Diagnostic, Phase, Severity, Span};
pub use expr::{AtomicComposite, Binder, BinderKind, EQ_SYM, Expr, Mods, Pattern};
pub use ids::{Era, NodeId, Scope, Strata, SymbolId, VarName};
pub use node::{
    Axiom, Calibration, Census, ConsumptionBlock, Definition, Node, NodeBase, NodeKind, Theorem,
};
pub use proof::{CaseArm, ChainJust, ChainRow, CiteName, IndCase, JustRef, ProofStep};
pub use symbol::{
    ApplyForm, CtorDecl, DBlockIR, ElabRule, FormBody, ModRoleKind, ModRoleTable, RoleSet,
    SymDecl, SymbolKind,
};
pub use tag::{CensusCharter, PhysOrMath, StepRel};