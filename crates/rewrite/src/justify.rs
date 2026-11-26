//! Does this citation do this to this row?
//!
//! The corpus's proof rows are explicit citations, and an explicit reference can name the wrong
//! row. This is the question the row check asks (`crates/build/src/row_check.rs`): given the two
//! sides of a row and the citation the author wrote, does that equation turn the first into the
//! second? It lives HERE rather than in `autocite` because it is the same equation application
//! the rest of this crate compiles with — the citation is what `autocite` searches for, and what
//! `chain` composes.
//!
//! Three answers, because "no" is not enough: `Yes`, `No { reaches }` (the states the citation
//! DOES reach, so the author sees the difference), and `UnknownCite` (a citation this scope
//! cannot resolve — the build reports that one as an error, an editor wants it while typing).

use qubitick_model::Expr;

use crate::diff::RewriteRule;
use crate::engine::try_rewrite;

/// What the corpus makes of one written row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The citation does what the row claims (or, with no citation, the two sides are one
    /// expression up to the engine's normal form).
    Yes,
    /// It does not — and here is what it does instead, as `(state, cite)` pairs, so the author can
    /// see the difference rather than be told "no".
    No { reaches: Vec<Expr> },
    /// The row cites something the corpus does not have. The build reports this as an error
    /// (`crates/build/src/cites.rs`); the editor wants to say it while the author is typing.
    UnknownCite,
}

/// Whether the citation `cite` (or reflexivity, when there is none) turns `from` into `to`.
///
/// A citation may name an INSTANCE (`\ulcornerA1.1 b b a\urcorner`): the corpus writes those to
/// say which arguments the lemma is applied to, and the underlying equation is the same one, so
/// the instance is reduced to the name it wraps before the lookup.
pub fn justified(
    from: &Expr,
    to: &Expr,
    cite: Option<&str>,
    rules: &[(String, RewriteRule)],
) -> Verdict {
    let Some(cite) = cite else {
        return if qubitick_pattern::expr_norm_eq(from, to) {
            Verdict::Yes
        } else {
            Verdict::No { reaches: vec![] }
        };
    };
    let name = base_cite(cite);
    let Some((_, rule)) = rules.iter().find(|(c, _)| c == name) else {
        return Verdict::UnknownCite;
    };
    let reaches: Vec<Expr> = try_rewrite(from, rule)
        .into_iter()
        .map(|(next, _)| next)
        .filter(|next| !qubitick_pattern::expr_norm_eq(next, from))
        .collect();
    if reaches
        .iter()
        .any(|next| qubitick_pattern::expr_norm_eq(next, to))
    {
        Verdict::Yes
    } else {
        Verdict::No { reaches }
    }
}

/// Whether a SEQUENCE of citations turns `from` into `to`.
///
/// A row may cite several equations — `[\ulcornerA1.4 a\urcorner, A1.1]` is a row that REGROUPS and
/// then rewrites — and the compiler composes them into one step. So the check has to compose them
/// too: checking only the first and calling the row unjustified is an over-claim about a row that
/// is fine, which is worse than saying nothing.
///
/// Bounded on purpose (this answers while an author types): at most one application per citation,
/// and a capped frontier.
pub fn justified_by_chain(
    from: &Expr,
    to: &Expr,
    cites: &[String],
    rules: &[(String, RewriteRule)],
) -> Verdict {
    if cites.is_empty() {
        return justified(from, to, None, rules);
    }
    if cites.len() == 1 {
        return justified(from, to, Some(&cites[0]), rules);
    }
    /* Every citation must resolve: a row naming one equation the corpus does not have is not
       "not justified", it is unanswerable. */
    let mut laws: Vec<&RewriteRule> = Vec::new();
    for cite in cites {
        let name = base_cite(cite);
        let Some((_, rule)) = rules.iter().find(|(c, _)| c == name) else {
            return Verdict::UnknownCite;
        };
        laws.push(rule);
    }

    const FRONTIER_CAP: usize = 64;
    let mut frontier: Vec<Expr> = vec![from.clone()];
    let mut seen: Vec<Expr> = vec![from.clone()];
    for _ in 0..cites.len() {
        let mut next: Vec<Expr> = Vec::new();
        for state in &frontier {
            for law in &laws {
                for (candidate, _) in try_rewrite(state, law) {
                    if qubitick_pattern::expr_norm_eq(&candidate, to) {
                        return Verdict::Yes;
                    }
                    if qubitick_pattern::expr_norm_eq(&candidate, from) {
                        continue;
                    }
                    if seen.iter().any(|s| qubitick_pattern::expr_norm_eq(s, &candidate)) {
                        continue;
                    }
                    seen.push(candidate.clone());
                    next.push(candidate);
                    if next.len() >= FRONTIER_CAP {
                        break;
                    }
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    /* What it did reach, so the author sees the difference rather than a bare "no". */
    Verdict::No {
        reaches: seen
            .into_iter()
            .filter(|s| !qubitick_pattern::expr_norm_eq(s, from))
            .take(4)
            .collect(),
    }
}

/// The equation a citation names: `A1.1`, `D.BT.02`, `T.BT.44` — and, for an instance
/// (`\ulcornerA1.1 b b a\urcorner`), the name inside the corners.
fn base_cite(cite: &str) -> &str {
    let trimmed = cite.trim();
    if let Some(rest) = trimmed.strip_prefix(qubitick_model::format::CITE_OPEN) {
        let inner = rest
            .strip_suffix(qubitick_model::format::CITE_CLOSE)
            .unwrap_or(rest);
        return inner.trim().split_whitespace().next().unwrap_or(inner.trim());
    }
    trimmed
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
    fn a_row_is_justified_by_the_citation_it_names() {
        let from = app(1, vec![sym(2), sym(3)]);
        let to = sym(2);
        assert_eq!(justified(&from, &to, Some("A1.2"), &rules()), Verdict::Yes);
        // an instance names the same equation
        assert_eq!(
            justified(&from, &to, Some("\\ulcornerA1.2 a\\urcorner"), &rules()),
            Verdict::Yes
        );
        /* THE WRONG CITATION IS REFUSED, AND SAYS WHAT IT WOULD HAVE REACHED INSTEAD.
           A1.3 is `a Δ a = ∅`: applied to `a Δ ∅` it fires BACKWARDS (∅ is rewritten to `a Δ a`),
           which is a step, but not the step this row claims — so the verdict is `No`, carrying
           the state that citation does reach. That list is the whole value of the answer: the
           author sees the difference instead of being told "no". */
        match justified(&from, &to, Some("A1.3"), &rules()) {
            Verdict::No { reaches } => {
                assert_eq!(reaches.len(), 1, "the state A1.3 does reach: {reaches:?}");
                assert!(
                    !qubitick_pattern::expr_norm_eq(&reaches[0], &to),
                    "and it is not the one the row claims: {reaches:?}"
                );
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
        // a citation that cannot fire AT ALL is the empty case of the same answer
        match justified(&sym(3), &sym(2), Some("A1.2"), &rules()) {
            Verdict::No { reaches } => assert!(reaches.is_empty(), "no Δ to rewrite: {reaches:?}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
        // a citation the corpus does not have is a third answer, not a "no"
        assert_eq!(justified(&from, &to, Some("A9.9"), &rules()), Verdict::UnknownCite);
    }

    #[test]
    fn a_row_with_no_citation_is_justified_by_being_the_same_expression() {
        assert_eq!(justified(&sym(2), &sym(2), None, &rules()), Verdict::Yes);
        assert_eq!(
            justified(&sym(2), &sym(3), None, &rules()),
            Verdict::No { reaches: vec![] }
        );
    }
}
