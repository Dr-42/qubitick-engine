//! The row check: does each row of a proof follow from the row above it?
//!
//! A Lean proof term that type-checks is right by construction. The corpus's rows are not terms —
//! they are `.qbit` lines with citation brackets — and an explicit reference can name the wrong
//! row. `cites.rs` catches a citation that names NOTHING; this catches a citation that does
//! nothing, which is the check a term-based system cannot offer at all.
//!
//! THE CHECK CONFIRMS; IT CANNOT REFUTE. Every verdict is about what the engine could DERIVE, and
//! the derivation is bounded and incomplete — so `unconfirmed` means "I could not confirm this",
//! not "this row is wrong". That distinction is not pedantry, it is measured: `T.BT.01`'s row
//! `= a \Delta (a \Delta (a \Delta a))  [\ulcornerA1.4 a\urcorner, A1.1]` is a correct step
//! (expand `∅` to `a Δ a`, then regroup) that no rewriting search can derive, because expanding
//! `∅` means instantiating the rule's metavariable `a` with a term only the AUTHOR knows.
//!
//! The five answers are therefore:
//!
//!   `confirmed`     the row's citation (or its composition, when it names several) reaches the
//!                   row's claim from the row above;
//!   `unconfirmed`   it does not, within the bound — with the states it DID reach, which is the
//!                   half of the answer an author can act on;
//!   `unknown-cite`  the citation names nothing this scope can see (the build reports that as an
//!                   error, and that one IS provable);
//!   `local`         `\equiv` and `^i`: the definitional step and the induction hypothesis, which
//!                   the corpus has no equation for;
//!   a row with no citation at all is `confirmed` when its two sides are one expression.
//!
//! ONE CITATION, TWO SPELLINGS. A citation reaches this module as a node and a number, because
//! that is what `qubitick_parse` keeps: `D.X.01` (the division the author wrote) and `D.1` (the
//! number the engine resolves it to) are the same reference, and `cite_label` reports the numeric
//! one. A client that shows the row shows the author's own text, which it has; what it needs from
//! here is the verdict.

use qubitick_ctx::Ctx;
use qubitick_db::Db;
use qubitick_model::{ChainJust, Expr, Node, ProofStep};

/// What the corpus makes of one written row.
#[derive(Debug, Clone)]
pub struct RowCheck {
    /// 1-based row within the proof block, the same number `ChainRow::line` carries.
    pub row: u32,
    /// The citation as the row writes it, when it writes one.
    pub cite: Option<String>,
    /// How many citations the row names (`[A1.4, A1.1]` is two).
    pub refs: usize,
    /// `confirmed` | `unconfirmed` | `local` | `unknown-cite`.
    ///
    /// `unconfirmed` is not "wrong" (module header): this check searches, and a failed search
    /// does not prove a row unjustified.
    pub verdict: &'static str,
    /// For `not-justified`: the states the citation does reach from the row above.
    pub reaches: Vec<Expr>,
    /// What the check compared: the state the row starts from (the row above's right-hand side)
    /// and the state it claims. A verdict nobody can reproduce is a verdict nobody can act on.
    pub from: Expr,
    pub to: Expr,
}

/// Check every row of every chain in `nodes`.
///
/// Induction cases are walked too: their bodies are chains, and a row inside a case is as
/// checkable as a row at the top level. The row numbers restart per chain, which is what
/// `ChainRow::line` means — a caller that wants a file line maps it the way the editor does (the
/// nth `= ` line of that block).
pub fn check_nodes(conn: &Db, ctx: &Ctx, nodes: &[Node]) -> Vec<RowCheck> {
    let mut out = Vec::new();
    for node in nodes {
        let Node::Theorem(theorem) = node else { continue };
        walk(conn, ctx, &theorem.proof, &mut out);
    }
    out
}

fn walk(conn: &Db, ctx: &Ctx, steps: &[ProofStep], out: &mut Vec<RowCheck>) {
    for step in steps {
        match step {
            ProofStep::Chain { rows } => {
                let mut prev: Option<Expr> = None;
                for row in rows {
                    let from = row
                        .lhs
                        .clone()
                        .or_else(|| prev.clone())
                        .unwrap_or_else(|| row.rhs.clone());
                    out.push(check_row(conn, ctx, row.line, &from, &row.rhs, &row.just));
                    prev = Some(row.rhs.clone());
                }
            }
            ProofStep::Case { arms, .. } => {
                for arm in arms {
                    walk(conn, ctx, &arm.body, out);
                }
            }
            ProofStep::Induction { cases, .. } | ProofStep::Recursor { cases, .. } => {
                for case in cases {
                    walk(conn, ctx, &case.body, out);
                }
            }
            /* The steps that carry no rows of their own: `Rewrite { refs }` is a step the
               compiler composes, and `Conclude`/`Note`/`Fix`/… are proof-level moves (a closing
               term, a remark, a variable binding) rather than rows an author justifies. */
            _ => {}
        }
    }
}

fn check_row(
    conn: &Db,
    ctx: &Ctx,
    row: u32,
    from: &Expr,
    to: &Expr,
    just: &ChainJust,
) -> RowCheck {
    /* THE LABEL A CITE PRINTS AS IS THE CITED NODE'S OWN — `D.A.02`, not `D.2`.
       The row check once rebuilt the label from the parsed name, which drops the division tag
       the corpus numbers with, so a report described a citation with a string the file does
       not contain. Resolution is by the number; printing is by the node. */
    let headers = qubitick_db::read::get_node_list(conn).unwrap_or_default();
    let printed = |name: &qubitick_model::CiteName| {
        crate::cite_target::printed_label(ctx, &headers, name)
    };
    let refs: Vec<String> = match just {
        ChainJust::Rewrite { refs } => refs.iter().map(|r| printed(&r.name)).collect(),
        ChainJust::Sym(reference) => vec![printed(&reference.name)],
        ChainJust::Exact { .. } | ChainJust::Rfl => Vec::new(),
    };
    if refs.is_empty() {
        /* NO CITATION: the row is justified by the two sides being one expression. */
        return RowCheck {
            row,
            cite: None,
            refs: 0,
            verdict: match qubitick_rewrite::justify::justified(from, to, None, &[]) {
                qubitick_rewrite::justify::Verdict::Yes => "confirmed",
                _ => "unconfirmed",
            },
            reaches: Vec::new(),
            from: from.clone(),
            to: to.clone(),
        };
    }

    /* A ROW MAY NAME SEVERAL EQUATIONS (`[\ulcornerA1.4 a\urcorner, A1.1]` is a row that
       regroups and then rewrites), and the compiler composes them into one step — so the check
       composes them too. Checking only the first and calling the row unjustified would be an
       over-claim about a row that is fine. */
    /* EVERY LAW THE ROW NAMES, not the first of them.
       A row may cite several (`[\ulcornerA1.4 a\urcorner, A1.1]` regroups and then rewrites),
       and the render tile prints this string beside the row — so a row that cites two laws
       showed one. Joined with `, `, which is the shape the client's citation renderer already
       normalises (web-next/js/app/math.js::citeTex, which splits on the comma and typesets each
       corner-quoted name). `refs` still carries the count. */
    let cite = refs.join(", ");
    if refs.iter().any(|c| is_local_cite(c)) {
        /* a local law among the citations (`^i`, `\equiv`) has no equation to compose with */
        return RowCheck {
            row,
            cite: Some(cite),
            refs: refs.len(),
            verdict: "local",
            reaches: Vec::new(),
            from: from.clone(),
            to: to.clone(),
        };
    }
    let mut rules: Vec<(String, qubitick_rewrite::RewriteRule)> = Vec::new();
    for c in &refs {
        match crate::cite_target::rule_of_cite(conn, ctx, c) {
            Some(rule) => rules.push((c.clone(), rule)),
            None => {
                return RowCheck {
                    row,
                    cite: Some(cite),
                    refs: refs.len(),
                    verdict: "unknown-cite",
                    reaches: Vec::new(),
                    from: from.clone(),
                    to: to.clone(),
                };
            }
        }
    }
    match qubitick_rewrite::justify::justified_by_chain(from, to, &refs, &rules) {
        qubitick_rewrite::justify::Verdict::Yes => RowCheck {
            row,
            cite: Some(cite),
            refs: refs.len(),
            verdict: "confirmed",
            reaches: Vec::new(),
            from: from.clone(),
            to: to.clone(),
        },
        qubitick_rewrite::justify::Verdict::No { reaches } => RowCheck {
            row,
            cite: Some(cite),
            refs: refs.len(),
            verdict: "unconfirmed",
            reaches,
            from: from.clone(),
            to: to.clone(),
        },
        qubitick_rewrite::justify::Verdict::UnknownCite => RowCheck {
            row,
            cite: Some(cite),
            refs: refs.len(),
            verdict: "unknown-cite",
            reaches: Vec::new(),
            from: from.clone(),
            to: to.clone(),
        },
    }
}

/// `\equiv` and `^i` are LOCAL laws: the definitional step and the induction hypothesis, which
/// the corpus has no equation for. They are reported as local, not as a failure.
fn is_local_cite(cite: &str) -> bool {
    crate::cite_target::is_local(cite)
}

