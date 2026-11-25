//! Node kinds and structures (DataSchema §5).
//!
//! All nodes share a `NodeBase` header. Five kinds: Axiom, Definition, Theorem,
//! Calibration, and Census. `Node` is a tagged summary over the five.

use serde::{Deserialize, Serialize};

use crate::expr::{AtomicComposite, Expr};
use crate::ids::{Era, NodeId, Strata};
use crate::proof::ProofStep;
use crate::symbol::{DBlockIR, SymDecl};
use crate::tag::{CensusCharter, PhysOrMath};

/// Which division/kind a node belongs to (DataSchema §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeKind {
    Axiom,
    Definition,
    Theorem,
    Calibration,
    Census,
}

impl NodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Axiom => "Axiom",
            NodeKind::Definition => "Definition",
            NodeKind::Theorem => "Theorem",
            NodeKind::Calibration => "Calibration",
            NodeKind::Census => "Census",
        }
    }
}

/// The shared header of every node (DataSchema §5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeBase {
    pub id: NodeId,
    pub kind: NodeKind,
    pub era: Era,
    pub strata: Strata,
    pub name: String,
    /// Declared dependencies (`req:` edges).
    pub req: Vec<NodeId>,
    /// Computed at build (`derived` edges).
    pub derived: Vec<NodeId>,
    /// The node's prose, as authored: the text of its `Math:` or `Phys:` line, or `None`.
    ///
    /// KEPT, not parsed. It is what the PDF prints (crates/build/src/pdf_doc.rs) and what the Lean
    /// emission comments (crates/emit/src/module.rs); it is never an expression. Which of the two
    /// readings it is travels in the kind's `tag: PhysOrMath` (DataSchema §5).
    pub note: Option<String>,
}

/// `A1 … A6` — statements fixed by axioms (DataSchema §5.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Axiom {
    pub base: NodeBase,
    pub statements: Vec<Expr>,
    pub tag: PhysOrMath,
    /// A6 only.
    pub falsify: Option<Expr>,
}

/// A definition node carrying a D-block (DataSchema §5.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Definition {
    pub base: NodeBase,
    /// The primary symbol being declared.
    pub symbol: AtomicComposite,
    /// Constructors of a formation; extra atoms.
    pub extra_symbols: Vec<SymDecl>,
    pub d_block: DBlockIR,
    pub tag: PhysOrMath,
}

/// A theorem statement plus its proof tree (DataSchema §5.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theorem {
    pub base: NodeBase,
    pub statement: Expr,
    pub proof: Vec<ProofStep>,
    pub tag: PhysOrMath,
}

/// Syzygy unit-calibration node (DataSchema §5.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Calibration {
    pub base: NodeBase,
    /// Free-form symbol label.
    pub symbol: String,
    /// 𝔮 notation.
    pub q_units: String,
    /// `[T]`, `[1]`, `[L]`, …
    pub si_dimensions: String,
    /// Rational or symbolic.
    pub conversion_rate: String,
    /// Backlinks, computed at build.
    pub referenced_by: Vec<NodeId>,
}

/// Consumption block carried by a Census node (DataSchema §5.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsumptionBlock {
    pub space: String,
    pub counts: String,
    pub baryonic: String,
}

/// Augury census node (DataSchema §5.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Census {
    pub base: NodeBase,
    pub role: String,
    pub charter: CensusCharter,
    /// The "Thm" field.
    pub statement: Expr,
    /// The "Pf" field.
    pub proof: Vec<ProofStep>,
    pub consumption: ConsumptionBlock,
    pub tag: PhysOrMath,
}

/// Tagged summary over all five node kinds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "node_kind")]
pub enum Node {
    Axiom(Axiom),
    Definition(Definition),
    Theorem(Theorem),
    Calibration(Calibration),
    Census(Census),
}

impl Node {
    pub fn base(&self) -> &NodeBase {
        match self {
            Node::Axiom(a) => &a.base,
            Node::Definition(d) => &d.base,
            Node::Theorem(t) => &t.base,
            Node::Calibration(c) => &c.base,
            Node::Census(c) => &c.base,
        }
    }
}