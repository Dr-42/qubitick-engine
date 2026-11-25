//! Following a citation: the label the corpus writes, and the node it points at.
//!
//! `cites.rs` validates that a citation NAMES something. This module is the other direction — a
//! written label in, the node it names out — because three callers need to follow a reference
//! rather than check it: the editor's links (`[A1.4]` opens A1 at its fourth statement), the
//! PDF's cross-references, and the goal pane, which shows the statement a row is citing.
//!
//! ONE READING OF A LABEL. The corpus writes `A<node>.<statement>` for an axiom line (`A1.4`),
//! and `D.<n>` / `T.<n>` for a definition or theorem, where `n` is the number the AUTHOR wrote in
//! the node's own header. `qubitick_parse::cite_name` is the parser's reading of that syntax and
//! this module resolves the result, so the rule "what a citation points at" has one
//! implementation rather than one per caller.
//!
//! The label of a node is derived from that same number — `division_of_filename` for a
//! definition or theorem (the digits in `D_BT_02_Neg.qbit` are `2`), the authored number for an
//! axiom — which is what makes the round trip testable: a node's own label resolves back to it.

use qubitick_ctx::Ctx;
use qubitick_db::read::NodeHeader;
use qubitick_model::{CiteName, Node, NodeKind};

use crate::axioms::division_of_filename;

/// Where a citation points, as the editor (and the PDF, and the goal pane) needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CiteTarget {
    /// The label as the corpus writes it: `A1.4`, `T.1`, `D.2`.
    pub label: String,
    /// The node's own name (`Realm`, `DeltaComm`).
    pub node: String,
    /// `axiom` | `definition` | `theorem` | … — the kind, as a lowercase word.
    pub kind: String,
    /// The number the citation names (the authored number, not the ledger id).
    pub number: u32,
    /// Which statement of an axiom line it names, for `A1.4`.
    pub statement: Option<u32>,
    /// The sector the node lives in (`Era0/Strata0`).
    pub sector: String,
    /// The file's name as the corpus stores it (`A1_Realm.qbit`, `D_BT_02_Neg.qbit`).
    pub filename: String,
}

/// The label a node's own citation reads as — `A1` for an axiom, `D.2`/`T.1` for a definition or
/// a theorem.
///
/// The axiom case uses the AUTHORED number, which is the number a citation names
/// (`crates/build/src/cites.rs::resolve` compares against exactly this): the ledger `id` happens
/// to equal it in the shipped corpus, and a corpus that numbered its axioms out of file order is
/// where the two would part company.
pub fn label_of(header: &NodeHeader) -> String {
    let number = header.authored.unwrap_or(header.id) as u32;
    match header.node_type {
        NodeKind::Axiom => format!("A{number}"),
        NodeKind::Definition => format!("D.{number}"),
        NodeKind::Theorem => format!("T.{number}"),
        NodeKind::Calibration => format!("S.{number}"),
        NodeKind::Census => format!("FT.{number}"),
    }
}

/// The label of one statement of an axiom: `A1.4`.
pub fn label_of_line(header: &NodeHeader, line: u32) -> String {
    format!("{}.{line}", label_of(header))
}

/// Follow a written citation to the node it names, in `scope`.
///
/// Returns `None` when the label is not a citation at all (`\equiv` and `^i` are LOCAL laws: the
/// definitional-equivalence step and the induction hypothesis — the corpus has no node for them)
/// or when it names a node this scope cannot see. The build reports the second case as a
/// `cite.dangling` error; a caller that wants to say "this is local" can read the label's shape
/// (`crate::cites::is_local`).
pub fn resolve_cite(ctx: &Ctx, headers: &[NodeHeader], name: &str) -> Option<CiteTarget> {
    let scope = ctx.scope();
    let cite = qubitick_parse::cite_name(name);
    let (kind, number, statement) = match cite.clone() {
        CiteName::AxiomLine { node, line } => (NodeKind::Axiom, node, Some(line)),
        CiteName::TheoremId(n) => (NodeKind::Theorem, n, None),
        CiteName::DefId(n) => (NodeKind::Definition, n, None),
        CiteName::Local(_) => return None,
    };
    let header = resolve(headers, scope, kind, number)?;
    Some(CiteTarget {
        label: match statement {
            Some(line) => label_of_line(header, line),
            None => label_of(header),
        },
        node: header.name.clone(),
        kind: kind.as_str().to_lowercase(),
        number,
        statement,
        sector: format!("{}/Strata{}", header.era, header.strata),
        filename: header.filename.clone(),
    })
}

/// Find the node a citation of `kind` and `number` names, in this scope.
///
/// Own era first, then an inherited one — the same precedence the context loader uses, and the
/// same comparison `crate::cites::resolve` makes, so "what a citation resolves to" has one answer
/// rather than one per caller.
fn resolve<'a>(
    headers: &'a [NodeHeader],
    scope: &str,
    kind: NodeKind,
    number: u32,
) -> Option<&'a NodeHeader> {
    let mut inherited = None;
    for header in headers {
        if header.node_type != kind {
            continue;
        }
        if header.authored.unwrap_or(header.id) as u32 != number {
            continue;
        }
        if !crate::axioms::node_visible(scope, &header.era, header.strata) {
            continue;
        }
        if format!("{}/Strata{}", header.era, header.strata) == scope {
            return Some(header);
        }
        inherited.get_or_insert(header);
    }
    inherited
}

/// The equation a citation licenses — the same rule `cite_rules` hands to the autocite search,
/// built for ONE citation rather than for the whole corpus.
///
/// The row check needs this: a row cites `A1.4` or `T.13`, and "does this row follow?" is the
/// question "does that equation rewrite the row above into this one?".
pub fn rule_of_cite(
    conn: &qubitick_db::Db,
    ctx: &Ctx,
    name: &str,
) -> Option<qubitick_rewrite::RewriteRule> {
    let scope = ctx.scope();
    let cite = qubitick_parse::cite_name(name);
    match cite {
        CiteName::AxiomLine { node, line } => {
            let headers = qubitick_db::read::get_node_list(conn).ok()?;
            let header = resolve(&headers, scope, NodeKind::Axiom, node)?;
            let loaded = crate::axioms::load_axiom_nodes(conn, ctx, scope)
                .ok()?
                .into_iter()
                .find(|n| n.base().id as i64 == header.id)?;
            let Node::Axiom(axiom) = &loaded else { return None };
            let statement = axiom.statements.get(line.checked_sub(1)? as usize)?;
            crate::cite_rules::equation_rule(statement)
        }
        CiteName::DefId(_) | CiteName::TheoremId(_) => {
            let headers = qubitick_db::read::get_node_list(conn).ok()?;
            let kind = if matches!(cite, CiteName::DefId(_)) {
                NodeKind::Definition
            } else {
                NodeKind::Theorem
            };
            let number = match cite {
                CiteName::DefId(n) | CiteName::TheoremId(n) => n,
                _ => return None,
            };
            let header = resolve(&headers, scope, kind, number)?;
            let division = division_of_filename(&header.filename)?;
            match kind {
                NodeKind::Definition => {
                    let loaded = crate::axioms::load_definition_nodes(conn, ctx, scope)
                        .ok()?
                        .into_iter()
                        .find(|l| l.division == division)?;
                    let Node::Definition(definition) = &loaded.node else { return None };
                    match &definition.d_block.form {
                        Some(qubitick_model::FormBody::MacroExpr(sema)) => {
                            crate::cite_rules::equation_rule(sema)
                        }
                        _ => None,
                    }
                }
                _ => {
                    let loaded = crate::axioms::load_theorem(conn, ctx, scope, division).ok()??;
                    crate::cite_rules::equation_rule(&loaded.statement)
                }
            }
        }
        CiteName::Local(_) => None,
    }
}

/// Whether a label is a LOCAL law rather than a corpus node: `\equiv` (the definitional step) and
/// `^i` (the induction hypothesis) are steps a proof takes without citing anything.
pub fn is_local(name: &str) -> bool {
    matches!(qubitick_parse::cite_name(name), CiteName::Local(_))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(kind: NodeKind, authored: i64, name: &str, filename: &str) -> NodeHeader {
        NodeHeader {
            id: authored,
            node_type: kind,
            era: "Era0".into(),
            strata: 0,
            name: name.into(),
            filename: filename.into(),
            tag: None,
            version: 1,
            godel: None,
            authored: Some(authored),
        }
    }

    #[test]
    fn a_node_labels_itself_the_way_a_citation_names_it() {
        assert_eq!(label_of(&header(NodeKind::Axiom, 1, "Realm", "A1_Realm.qbit")), "A1");
        assert_eq!(
            label_of_line(&header(NodeKind::Axiom, 1, "Realm", "A1_Realm.qbit"), 4),
            "A1.4"
        );
        // a definition's number is the digits in its filename, which is what cite_rules labels it
        // with — and what a written `D.2` extracts
        assert_eq!(label_of(&header(NodeKind::Definition, 2, "Neg", "D_BT_02_Neg.qbit")), "D.2");
        assert_eq!(label_of(&header(NodeKind::Theorem, 1, "DeltaComm", "T_BT_01_DeltaComm.qbit")), "T.1");
    }

    #[test]
    fn a_written_citation_resolves_to_the_node_that_wrote_that_label() {
        let ctx = Ctx::from_rows("Era0/Strata0", vec![]);
        let headers = vec![
            header(NodeKind::Axiom, 1, "Realm", "A1_Realm.qbit"),
            header(NodeKind::Theorem, 1, "DeltaComm", "T_BT_01_DeltaComm.qbit"),
            header(NodeKind::Definition, 2, "Neg", "D_BT_02_Neg.qbit"),
        ];
        /* THE ROUND TRIP the editor's links and the PDF's cross-references rest on: a node's own
           label resolves back to that node. An axiom's bare label names the node and reads as its
           FIRST statement (`A1.4` is what a proof writes; `A1` is the node), which is why the
           label comparison is exact for a definition and a theorem and one line deep for an
           axiom. */
        for h in &headers {
            let label = label_of(h);
            let target = resolve_cite(&ctx, &headers, &label).expect(&format!("{label} resolves"));
            assert_eq!(target.node, h.name, "{label} resolves to its own node");
            match h.node_type {
                NodeKind::Axiom => assert_eq!(target.label, format!("{label}.1"), "{label}"),
                _ => assert_eq!(label_of(h), target.label, "{label} round-trips"),
            }
        }

        let axiom = resolve_cite(&ctx, &headers, "A1.4").expect("a statement of an axiom");
        assert_eq!(axiom.label, "A1.4");
        assert_eq!(axiom.statement, Some(4), "which statement of it");
        assert_eq!(axiom.node, "Realm");
        assert_eq!(axiom.sector, "Era0/Strata0");
        assert_eq!(axiom.filename, "A1_Realm.qbit");

        // a local law is not a node: `\equiv` and `^i` are steps a proof takes by itself
        assert!(resolve_cite(&ctx, &headers, "\\equiv").is_none());
        assert!(is_local("\\equiv") && is_local("^i"));
        assert!(!is_local("A1.4"));
        // and a citation of something the scope cannot see is not a target either
        assert!(resolve_cite(&ctx, &headers, "T.9").is_none());
    }
}
