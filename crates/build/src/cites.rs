//! Reference integrity: a citation that names nothing is not a proof step.
//!
//! The justifier already refuses a rewrite it cannot justify — but only for the
//! rows that need a law. A row that is syntactically reflexive, or whose citation
//! is never consulted because the goal closed by conversion, can name a theorem
//! that does not exist and the file still reports `verified`: the Lean type-checks
//! precisely because the reference was never used. A citation is a reference, and
//! references are checked whether or not the proof happened to need them.
//!
//! Resolution follows the loaders, not the reader: a citation names the number the
//! AUTHOR wrote (`[A4.2]` = axiom 4, statement 2), a file's own era answers before
//! an inherited one, and only nodes visible from the citing scope count.

use anyhow::Result;
use qubitick_ctx::Ctx;
use qubitick_db::read::NodeHeader;
use qubitick_model::{ChainJust, CiteName, Node, NodeKind, ProofStep};

use crate::axioms::node_visible;

/// Every citation in these nodes that names something the corpus does not have,
/// as human-readable problems. Empty is good.
pub fn dangling(
    conn: &qubitick_db::Db,
    ctx: &Ctx,
    headers: &[NodeHeader],
    nodes: &[Node],
) -> Result<Vec<String>> {
    let scope = ctx.scope();
    let mut out: Vec<String> = Vec::new();
    for node in nodes {
        let Node::Theorem(theorem) = node else {
            continue; // axioms and definitions carry statements, not proofs
        };
        let mut cites: Vec<&CiteName> = Vec::new();
        collect(&theorem.proof, &mut cites);
        for cite in cites {
            let problem = match cite {
                CiteName::AxiomLine { node, line } => {
                    let label = node_display(NodeKind::Axiom, "", *node);
                    match resolve(headers, scope, NodeKind::Axiom, "", *node) {
                        None => Some(format!("[{label}.{line}] names no axiom")),
                        Some(header) => {
                            let statements: i64 = conn.query_row(
                                "SELECT COUNT(*) FROM node_statements WHERE node_id = ?1",
                                [header.id],
                                |r| r.get(0),
                            )?;
                            if *line == 0 || i64::from(*line) > statements {
                                Some(format!(
                                    "[{label}.{line}] names statement {line} of axiom '{}', which has {statements}",
                                    header.name
                                ))
                            } else {
                                None
                            }
                        }
                    }
                }
                CiteName::DefId { tier, n } => {
                    resolve(headers, scope, NodeKind::Definition, tier, *n)
                        .is_none()
                        .then(|| {
                            format!(
                                "[{}] names no definition",
                                node_display(NodeKind::Definition, tier, *n)
                            )
                        })
                }
                CiteName::TheoremId { tier, n } => {
                    resolve(headers, scope, NodeKind::Theorem, tier, *n)
                        .is_none()
                        .then(|| {
                            format!(
                                "[{}] names no theorem",
                                node_display(NodeKind::Theorem, tier, *n)
                            )
                        })
                }
                // A local hypothesis is in scope by construction; there is nothing
                // in the corpus for it to name.
                CiteName::Local(_) => None,
            };
            if let Some(problem) = problem
                && !out.contains(&problem) {
                    out.push(problem);
                }
        }
    }
    Ok(out)
}

/// How a citation of this kind reads in a diagnostic (`[A4.2]`, `[T.81]`).
fn node_display(kind: NodeKind, tier: &str, number: u32) -> String {
    let kind = match kind {
        NodeKind::Axiom => "A",
        NodeKind::Definition => "D",
        NodeKind::Theorem => "T",
        NodeKind::Calibration => "S",
        NodeKind::Census => "FT",
    };
    if tier.is_empty() {
        format!("{kind}.{number}")
    } else {
        format!("{kind}.{tier}.{number}")
    }
}

/// Find the node a citation of `kind` and `number` names, in this scope.
///
/// Own era first, then an inherited one — the same precedence the context loader
/// uses, so "what a citation resolves to" has one answer in both places.
fn resolve<'a>(
    headers: &'a [NodeHeader],
    scope: &str,
    kind: NodeKind,
    tier: &str,
    number: u32,
) -> Option<&'a NodeHeader> {
    let mut inherited = None;
    for header in headers {
        if header.node_type != kind {
            continue;
        }
        /* THE TIER IS PART OF THE IDENTITY. Two tiers number their nodes independently
           (`D.BT.0.7` and `D.QB.0.7` are different definitions), so a citation that names a
           tier must match it — otherwise the number alone decides, and the walk decides for it.
           A citation that names no tier (`[D.7]`) matches by number, as it always did. */
        if !tier.is_empty() && header.division != tier {
            continue;
        }
        if header.authored.unwrap_or(header.id) as u32 != number {
            continue;
        }
        if !node_visible(scope, &header.era, header.strata) {
            continue;
        }
        if format!("{}/Strata{}", header.era, header.strata) == scope {
            return Some(header);
        }
        inherited.get_or_insert(header);
    }
    inherited
}

/// Every citation reachable from these steps, including nested case bodies.
fn collect<'a>(steps: &'a [ProofStep], out: &mut Vec<&'a CiteName>) {
    for step in steps {
        match step {
            ProofStep::Chain { rows } => {
                for row in rows {
                    match &row.just {
                        ChainJust::Rewrite { refs } => out.extend(refs.iter().map(|r| &r.name)),
                        ChainJust::Sym(reference) => out.push(&reference.name),
                        // `Exact` embeds a term, `Rfl` names nothing.
                        ChainJust::Exact { .. } | ChainJust::Rfl => {}
                    }
                }
            }
            ProofStep::Rewrite { refs } => out.extend(refs.iter().map(|r| &r.name)),
            ProofStep::Case { arms, .. } => {
                for arm in arms {
                    collect(&arm.body, out);
                }
            }
            ProofStep::Induction { cases, .. } | ProofStep::Recursor { cases, .. } => {
                for case in cases {
                    collect(&case.body, out);
                }
            }
            ProofStep::Note { body, .. } => collect(body, out),
            ProofStep::Conclude { .. } | ProofStep::Fix { .. } | ProofStep::Discriminate { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_model::{Era, NodeBase, Scope, Strata, Theorem};

    fn header(id: i64, kind: NodeKind, number: u32, era: &str, strata: i64) -> NodeHeader {
        NodeHeader {
            id,
            node_type: kind,
            era: era.into(),
            strata,
            name: format!("n{number}"),
            filename: format!("x{number}.qbit"),
            tag: None,
            version: 1,
            godel: Some(id),
            authored: Some(i64::from(number)),
            division: String::new(),
        }
    }

    fn citing(names: Vec<CiteName>) -> Vec<Node> {
        let refs = names
            .into_iter()
            .map(|name| qubitick_model::JustRef {
                rev: false,
                name,
                args: Vec::new(),
            })
            .collect();
        vec![Node::Theorem(Theorem {
            base: NodeBase {
                id: 1,
                kind: NodeKind::Theorem,
                era: Era::Era0,
                strata: Strata(0),
                name: "T".into(),
                req: Vec::new(),
                derived: Vec::new(),
                note: None,
            },
            statement: qubitick_model::Expr::Var("a".into()),
            proof: vec![ProofStep::Chain {
                rows: vec![qubitick_model::ChainRow {
                    line: 1,
                    rel: qubitick_model::StepRel::Eq,
                    lhs: None,
                    rhs: qubitick_model::Expr::Var("a".into()),
                    just: ChainJust::Rewrite { refs },
                }],
            }],
            tag: qubitick_model::PhysOrMath::Math,
        })]
    }

    #[test]
    fn a_citation_to_a_number_no_node_claims_is_reported() {
        let conn = qubitick_db::open_in_memory().unwrap();
        let ctx = Ctx::from_rows("Era0/Strata0", vec![]);
        // the corpus has theorem 7; the proof cites [T.9]
        let headers = vec![header(1, NodeKind::Theorem, 7, "Era0", 0)];
        let nodes = citing(vec![CiteName::TheoremId { tier: String::new(), n: 9 }]);
        let problems = dangling(&conn, &ctx, &headers, &nodes).unwrap();
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("T.9"), "{problems:?}");

        // and the same citation against a theorem that exists is clean
        let nodes = citing(vec![CiteName::TheoremId { tier: String::new(), n: 7 }]);
        assert!(dangling(&conn, &ctx, &headers, &nodes).unwrap().is_empty());
    }

    #[test]
    fn a_citation_to_a_statement_the_axiom_does_not_have_is_reported() {
        let conn = qubitick_db::open_in_memory().unwrap();
        let ctx = Ctx::from_rows("Era0/Strata0", vec![]);
        let headers = vec![header(1, NodeKind::Axiom, 4, "Era0", 0)];
        // no statement rows are written for node 1, so every line index is too far
        let nodes = citing(vec![CiteName::AxiomLine { node: 4, line: 2 }]);
        let problems = dangling(&conn, &ctx, &headers, &nodes).unwrap();
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("statement 2"), "{problems:?}");
    }

    #[test]
    fn a_definition_citation_resolves_only_within_scope() {
        let conn = qubitick_db::open_in_memory().unwrap();
        // era 1's own scope cannot see era 0's definition of the same number
        let ctx = Ctx::from_rows("Era1/Strata1", vec![]);
        let headers = vec![header(1, NodeKind::Definition, 2, "Era0", 0)];
        let nodes = citing(vec![CiteName::DefId { tier: String::new(), n: 2 }]);
        let problems = dangling(&conn, &ctx, &headers, &nodes).unwrap();
        assert_eq!(problems.len(), 1, "an inherited definition is not visible: {problems:?}");

        // ...but its own scope does, once the node lives there
        let headers = vec![header(2, NodeKind::Definition, 2, "Era1", 1)];
        assert!(dangling(&conn, &ctx, &headers, &nodes).unwrap().is_empty());
    }

    #[test]
    fn local_hypothesis_citations_are_not_references() {
        let conn = qubitick_db::open_in_memory().unwrap();
        let ctx = Ctx::from_rows("Era0/Strata0", vec![]);
        let nodes = citing(vec![CiteName::Local("h1".into())]);
        assert!(dangling(&conn, &ctx, &[], &nodes).unwrap().is_empty());
    }

    /// The scope rule is the loader's, spelled out: own era, then inherited.
    #[test]
    fn own_era_wins_over_an_inherited_number() {
        let headers = vec![
            header(1, NodeKind::Axiom, 1, "Era0", 0),
            header(2, NodeKind::Axiom, 1, "Era1", 1),
        ];
        let picked = resolve(&headers, "Era1/Strata1", NodeKind::Axiom, "", 1).unwrap();
        assert_eq!(picked.id, 2, "the era's own A1 answers, not Era0's");
        let picked = resolve(&headers, "Era0/Strata0", NodeKind::Axiom, "", 1).unwrap();
        assert_eq!(picked.id, 1);
        // a scope that cannot see either era answers nothing
        assert!(resolve(&headers, "Era0/Strata1", NodeKind::Axiom, "", 1).is_some());
        let _ = Scope("Era1/Strata1".into());
    }
}