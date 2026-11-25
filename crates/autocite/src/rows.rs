//! The two questions an editor asks about a proof row, answered from the corpus's own equations.
//!
//! `steps` answers **"what can I write next?"** — every state one cited equation reaches from this
//! one, each with the citation that licenses it. That is the same walk `search::run_ruled` does
//! inside a bridge, exposed as suggestions instead of as a search: an author who does not know
//! which lemma applies gets the list, and the citation comes with it.
//!
//! `justified` answers **"does the row I wrote follow?"** — whether the citation the author wrote
//! turns the row above into the row itself. This is the check no term-based system can offer: a
//! Lean proof term that type-checks is right by construction, but the corpus's rows are *explicit*
//! citations, and an explicit reference can name the wrong row. The build already checks that a
//! citation NAMES something (`crate::…::cites`); this checks that it *does* something.
//!
//! Both are pure: the equations are handed in (`qubitick_build::cite_rules` builds them), so they
//! are testable without a database.

use qubitick_model::Expr;
use qubitick_rewrite::diff::RewriteRule;
use qubitick_rewrite::engine::try_rewrite;

/// The row verdict lives with the equation application it asks about (`qubitick_rewrite::justify`),
/// and is re-exported here because the editor's two questions are answered from this module.
pub use qubitick_rewrite::justify::{Verdict, justified};

/// One reachable state, with the citation that licenses the step.
#[derive(Debug, Clone)]
pub struct Step {
    pub cite: String,
    pub expr: Expr,
}

/// The states one cited equation reaches from `state`, up to `limit` of them.
///
/// A step that changes nothing is not a suggestion: `a ∧ a = a` applied to a state that already
/// is `a` would offer the state back, which is noise in a list an author reads. Duplicates (two
/// citations reaching the same state) keep the FIRST citation, which is the order the corpus
/// lists its equations in.
pub fn steps(state: &Expr, rules: &[(String, RewriteRule)], limit: usize) -> Vec<Step> {
    let mut out: Vec<Step> = Vec::new();
    for (cite, rule) in rules {
        for (next, _step) in try_rewrite(state, rule) {
            if qubitick_pattern::expr_norm_eq(&next, state) {
                continue;
            }
            if out
                .iter()
                .any(|s| qubitick_pattern::expr_norm_eq(&s.expr, &next))
            {
                continue;
            }
            out.push(Step {
                cite: cite.clone(),
                expr: next,
            });
            if out.len() >= limit {
                return out;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_model::AtomicComposite;

    fn sym(id: u32) -> Expr {
        Expr::Sym(AtomicComposite::new(id))
    }

    fn app(id: u32, args: Vec<Expr>) -> Expr {
        Expr::Apply(AtomicComposite::new(id), args)
    }

    /// The corpus's own shape, in miniature: `a Δ a -> ∅` and `a Δ ∅ -> a`.
    fn rules() -> Vec<(String, RewriteRule)> {
        vec![
            (
                "A1.3".to_string(),
                RewriteRule::new(app(1, vec![sym(2), sym(2)]), sym(3), None),
            ),
            (
                "A1.2".to_string(),
                RewriteRule::new(app(1, vec![sym(2), sym(3)]), sym(2), None),
            ),
        ]
    }

    #[test]
    fn steps_offer_the_states_one_cited_equation_reaches() {
        let state = app(1, vec![sym(2), sym(3)]);
        let got = steps(&state, &rules(), 10);
        assert_eq!(got.len(), 3, "both equations, both directions, at every position: {got:?}");
        assert!(
            got.iter().any(|s| s.cite == "A1.2" && qubitick_pattern::expr_norm_eq(&s.expr, &sym(2))),
            "`a Δ ∅` rewrites to `a` by A1.2 — the step an author is most often looking for: {got:?}"
        );
        assert!(
            got.iter().any(|s| s.cite == "A1.3"),
            "and A1.3 reaches `a Δ (a Δ a)` by expanding ∅ backwards: {got:?}"
        );
    }

    #[test]
    fn a_step_that_changes_nothing_is_not_a_suggestion() {
        // A1.3's left side is `a Δ a`; applied to `∅` nothing matches, and applied to a state it
        // already equals, the state itself would come back — which is noise, not a suggestion.
        let state = app(1, vec![sym(2), sym(2)]);
        let got = steps(&state, &rules(), 10);
        assert!(
            !got.iter().any(|s| qubitick_pattern::expr_norm_eq(&s.expr, &state)),
            "the state itself is never offered back: {got:?}"
        );
    }

    #[test]
    fn limit_is_respected() {
        let mut many = rules();
        many.push((
            "T.1".to_string(),
            RewriteRule::new(sym(2), sym(3), None),
        ));
        assert_eq!(steps(&app(1, vec![sym(2), sym(3)]), &many, 1).len(), 1);
    }

}
