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
///
/// A NODE'S IDENTITY IS ITS TIER AND ITS NUMBER, NOT ITS NUMBER ALONE. A corpus that grows by
/// tier (`D.BT.0.1`, `D.QB.1.1`, `T.HD.14.3`) numbers within the tier, so two nodes of one kind
/// legitimately carry the same number — `[D.BT.0.7]` and `[D.QB.0.7]` are different nodes. The
/// number alone made them one citation, and the build resolved it to whichever node the walk
/// reached first. The `tier` is the id's own text without its last segment (`D.BT.0.7` → `BT.0`);
/// an EMPTY tier is a citation that does not name one (`[D.7]`), and it matches by number as it
/// always did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CiteName {
    /// `A<n>.<line>` — a statement of an axiom node.
    AxiomLine { node: NodeId, line: u32 },
    /// `T.<tier>.<n>` — a theorem, addressed by the tier it is numbered in.
    TheoremId { tier: String, n: NodeId },
    /// `D.<tier>.<n>` — a definition, addressed by the tier it is numbered in.
    DefId { tier: String, n: NodeId },
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