//! The corpus as a rewrite system: every citable equation, labelled with the
//! citation that names it.
//!
//! `autocite` asks "which rows justify this step", and that question is only
//! answerable if the rows are in hand as rewrites. The build already assembles
//! them for the justifier (`RuleTable`); this is the same material in the shape a
//! *search* needs: a flat list of `(citation, from → to)`.
//!
//! Three sources, because a proof may cite three kinds of thing:
//!   · axioms      — one rule per statement, cited `A<node>.<line>`;
//!   · definitions — the `Sema:` equation, cited `D.<division>`;
//!   · theorems    — the statement, cited `T.<division>`.
//!
//! Only equations become rules: `strip_binders` then the kernel equality symbol,
//! which is how the rest of the engine recognises one (`EQ_SYM`). An `↔` or a
//! quantified row is a fact about the corpus, not a rewrite a chain row can use.

use anyhow::Result;
use qubitick_ctx::Ctx;
use qubitick_db::read::get_node_list;
use qubitick_model::{Expr, FormBody, Node, NodeKind};
use qubitick_rewrite::RewriteRule;

/// One citable equation: the rewrite it licenses and the citation that names it.
#[derive(Debug, Clone)]
pub struct CiteRule {
    /// How a proof would write the citation (`A1.5`, `D.BT.02`, `T.BT.44`).
    pub cite: String,
    /// The equation as a rewrite, in the direction the corpus states it.
    pub rule: RewriteRule,
}

/// Every equation the corpus can cite, in scope, as rewrite rules.
///
/// Cost note: the theorem sweep asks the loader for each theorem by division, as
/// the emit path does. That is fine for a command a human runs; a hot path should
/// read `node_statements` directly.
pub fn cite_rules(ctx: &Ctx, conn: &qubitick_db::Db) -> Result<Vec<CiteRule>> {
    let scope = ctx.scope();
    let mut out: Vec<CiteRule> = Vec::new();

    for node in crate::axioms::load_axiom_nodes(conn, ctx, scope)? {
        let Node::Axiom(axiom) = &node else { continue };
        for (i, statement) in axiom.statements.iter().enumerate() {
            if let Some(rule) = equation_rule(statement) {
                out.push(CiteRule {
                    cite: format!("A{}.{}", axiom.base.id, i + 1),
                    rule,
                });
            }
        }
    }

    for loaded in crate::axioms::load_definition_nodes(conn, ctx, scope)? {
        let Node::Definition(definition) = &loaded.node else { continue };
        let Some(FormBody::MacroExpr(sema)) = &definition.d_block.form else {
            continue;
        };
        if let Some(rule) = equation_rule(sema) {
            out.push(CiteRule {
                cite: format!("D.{}", loaded.division),
                rule,
            });
        }
    }

    for header in get_node_list(conn)? {
        if header.node_type != NodeKind::Theorem {
            continue;
        }
        let Some(division) = crate::axioms::division_of_filename(&header.filename) else {
            continue;
        };
        let Some(cited) = crate::axioms::load_theorem(conn, ctx, scope, division)? else {
            continue;
        };
        if let Some(rule) = equation_rule(&cited.statement) {
            out.push(CiteRule {
                cite: format!("T.{division}"),
                rule,
            });
        }
    }

    Ok(out)
}

/// A statement as a two-sided rule, or nothing when it is not an equation.
///
/// PUBLIC because the row check asks the same question for ONE citation
/// (`crate::cite_target::rule_of_cite`) that this module asks for the whole corpus: is this
/// statement an equation a proof row can step with?
pub fn equation_rule(statement: &Expr) -> Option<RewriteRule> {
    let (_vars, body) = qubitick_rewrite::strip_binders(statement);
    let Expr::Apply(head, args) = &body else {
        return None;
    };
    if head.primary != qubitick_model::EQ_SYM || args.len() != 2 {
        return None;
    }
    // `x = x` licenses nothing a search should expand: it would make every state
    // its own neighbour and flood the frontier.
    if qubitick_pattern::expr_norm_eq(&args[0], &args[1]) {
        return None;
    }
    Some(RewriteRule::new(args[0].clone(), args[1].clone(), None::<String>))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eq(lhs: Expr, rhs: Expr) -> Expr {
        Expr::Apply(
            qubitick_model::AtomicComposite::new(qubitick_model::EQ_SYM),
            vec![lhs, rhs],
        )
    }

    #[test]
    fn an_equation_becomes_a_rule() {
        let rule = equation_rule(&eq(
            Expr::Var("a".into()),
            Expr::Var("b".into()),
        ))
        .expect("an equation is a rule");
        assert!(matches!(rule.from, Expr::Var(_)));
        assert!(!rule.rev);
    }

    #[test]
    fn a_tautology_is_not_a_rule() {
        let a = Expr::Var("a".into());
        assert!(equation_rule(&eq(a.clone(), a)).is_none());
    }

    #[test]
    fn a_non_equation_is_not_a_rule() {
        // `∩ a b`: an application, not an equality
        let applied = Expr::Apply(qubitick_model::AtomicComposite::new(7), vec![
            Expr::Var("a".into()),
            Expr::Var("b".into()),
        ]);
        assert!(equation_rule(&applied).is_none());
    }
}