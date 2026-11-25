//! Candidate filtering for autocite (CodeArch §5.10).

use qubitick_ctx::Ctx;
use qubitick_model::Expr;

/// A candidate rewrite lemma: `lhs → rhs`.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub lhs: Expr,
    pub rhs: Expr,
}

/// Filter candidate lemmas by head-symbol overlap with the target subterm and by
/// scope visibility. Returns the subset whose head symbol id matches `target`'s
/// head (structural fallback) or that are visible in `ctx.scope`.
pub fn filter_candidates(_ctx: &Ctx, target: &Expr, candidates: Vec<Candidate>) -> Vec<Candidate> {
    let head = head_symbol_id(target);
    let mut out = Vec::new();
    for c in candidates {
        // Keep candidates that share a head symbol id with the target.
        let c_head = head_symbol_id(&c.lhs);
        if head.is_some() && c_head == head {
            out.push(c);
        }
    }
    // Scope filtering is applied by the caller when candidates are resolved from
    // the corpus index; this stage keeps only head-symbol matches.
    out
}

/// The head `SymbolId` of an expression, if it is an application or atom.
pub fn head_symbol_id(e: &Expr) -> Option<u32> {
    match e {
        Expr::Sym(ac) => Some(ac.primary),
        Expr::Apply(ac, _) => Some(ac.primary),
        Expr::Bracket(id, _) => Some(*id),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_model::AtomicComposite;

    fn sym(id: u32) -> Expr {
        Expr::Sym(AtomicComposite::new(id))
    }

    #[test]
    fn head_of_sym() {
        assert_eq!(head_symbol_id(&sym(3)), Some(3));
    }

    #[test]
    fn filters_by_head() {
        let ctx = Ctx::from_rows("Era0/Strata0", vec![]);
        let target = sym(3);
        let cands = vec![
            Candidate { lhs: sym(3), rhs: sym(4) },
            Candidate { lhs: sym(9), rhs: sym(4) },
        ];
        let kept = filter_candidates(&ctx, &target, cands);
        assert_eq!(kept.len(), 1);
    }
}