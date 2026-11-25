//! The proof IR (DataSchema §7).
//!
//! Every theorem and census node carries a `proof: Vec<ProofStep>` tree. Covers
//! chains, concludes, case/induction/recursor, notes, rewrites, fix, and
//! discriminate.

use serde::{Deserialize, Serialize};

use crate::expr::{Expr, Pattern};
use crate::ids::{NodeId, VarName};
use crate::tag::StepRel;

/// A reference to the justification of a proof step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JustRef {
    pub rev: bool,
    pub name: CiteName,
    pub args: Vec<Expr>,
}

/// What a citation points at (DataSchema §7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CiteName {
    /// `A<n>.<line>` — a statement of an axiom node.
    AxiomLine { node: NodeId, line: u32 },
    TheoremId(NodeId),
    DefId(NodeId),
    /// Local hypothesis such as `h0`, `ih`.
    Local(VarName),
}

/// The justification attached to a single chain row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "by")]
pub enum ChainJust {
    Rewrite { refs: Vec<JustRef> },
    Sym(JustRef),
    Exact { term: Expr },
    Rfl,
}

/// One row of a `Chain`.
///
/// `lhs` is `None` only for continuation rows, where the previous row's `rhs` is
/// the implicit lhs (DataSchema §7 invariant).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainRow {
    /// 1-based row index within the proof block it was written in.
    ///
    /// The parser works on field text, not spans, so this is the finest position
    /// it can honestly carry — but "row 3" plus the row's own two sides is what an
    /// author needs to find the line that will not justify.
    pub line: u32,
    pub lhs: Option<Expr>,
    pub rhs: Expr,
    pub rel: StepRel,
    pub just: ChainJust,
}

/// One case arm of a `Case` step (proof-only; uses a construct-or `Pattern`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseArm {
    pub pattern: Pattern,
    pub body: Vec<ProofStep>,
}

/// One case of an `Induction` / `Recursor` step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndCase {
    pub pattern: Option<Pattern>,
    pub body: Vec<ProofStep>,
}

/// A single proof step (DataSchema §7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "step")]
pub enum ProofStep {
    Chain { rows: Vec<ChainRow> },
    Conclude { term: Expr },
    Case { scrutinee: Expr, arms: Vec<CaseArm> },
    Induction { var: VarName, cases: Vec<IndCase> },
    Recursor { scrutinee: Expr, cases: Vec<IndCase> },
    Note { name: VarName, prop: Expr, body: Vec<ProofStep> },
    Rewrite { refs: Vec<JustRef> },
    Fix { vars: Vec<VarName> },
    Discriminate { hyp: VarName },
}