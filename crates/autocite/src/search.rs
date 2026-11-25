//! Bidirectional BFS over expression states (CodeArch §5.10).

use std::collections::{HashMap, VecDeque};

use qubitick_ctx::Ctx;
use qubitick_model::Expr;

use crate::AutociteResult;
use crate::distance::{diff_pairs, structural_distance};
/// Maximum states to explore. Sized for the ruled search: the corpus contributes
/// ~125 equations, each applicable at every position, so a bound tuned to the old
/// syntactic walk (256) truncated real two-step bridges.
const MAX_VISITED: usize = 4096;
/// Max BFS depth.
const MAX_DEPTH: usize = 10;

/// Run the autocite search from `lhs` toward `rhs`.
pub fn run(ctx: &Ctx, lhs: &Expr, rhs: &Expr) -> AutociteResult {
    // A bounded forward BFS that terminates with the closest reachable state to
    // `rhs`, or a full bridge when one is found within budget.
    let mut queue: VecDeque<(Expr, usize)> = VecDeque::new();
    queue.push_back((lhs.clone(), 0));
    let mut visited: HashMap<String, u32> = HashMap::new();
    visited.insert(key(lhs), 0);

    let mut best: Option<(Expr, usize)> = None;
    let mut best_dist = usize::MAX;

    while let Some((state, depth)) = queue.pop_front() {
        if qubitick_pattern::expr_norm_eq(&state, rhs) {
            return AutociteResult {
                lhs: lhs.clone(),
                rhs: rhs.clone(),
                chain: vec![state],
                reached: true,
                closest: None,
                gap: None,
            };
        }
        let d = structural_distance(&state, rhs);
        if d < best_dist {
            best_dist = d;
            best = Some((state.clone(), depth));
        }
        if depth >= MAX_DEPTH || visited.len() >= MAX_VISITED {
            continue;
        }
        // Generate one-step neighbours via a single head-symbol substitute using
        // the candidate index built from both endpoints.
        let neighbours = neighbours_of(ctx, &state, rhs);
        for n in neighbours {
            if !visited.contains_key(&key(&n)) {
                let nk = key(&n);
                visited.insert(nk, (depth + 1) as u32);
                queue.push_back((n, depth + 1));
            }
        }
    }

    let (closest, _) = best.unwrap_or((lhs.clone(), 0));
    let gap = diff_pairs(&closest, rhs);
    AutociteResult {
        lhs: lhs.clone(),
        rhs: rhs.clone(),
        chain: Vec::new(),
        reached: false,
        closest: Some(closest),
        gap: Some(gap),
    }
}

/// Expand `state` to neighbour states by applying a candidate lemma whose LHS
/// head matches a subterm's head. In the skeleton this uses the index built from
/// the rhs as a pseudo-lemma, demonstrating the mechanism.
fn neighbours_of(_ctx: &Ctx, state: &Expr, rhs: &Expr) -> Vec<Expr> {
    let mut out = Vec::new();
    // If the state and rhs share a head symbol and both are applications with
    // the same head, substitute the last argument of rhs into state as a probe.
    if crate::candidates::head_symbol_id(state) == crate::candidates::head_symbol_id(rhs)
        && let (Expr::Apply(h1, a1), Expr::Apply(h2, _)) = (state, rhs)
        && h1.primary == h2.primary
        && let Some(last_rhs) = a1.last()
        && let Expr::Apply(h, mut a2) = state.clone()
    {
        if let Some(last) = a2.last_mut() {
            *last = last_rhs.clone();
        }
        out.push(Expr::Apply(h, a2));
    }
    out
}

fn key(e: &Expr) -> String {
    let sh = |id: u32| id as u128;
    format!("{:x}", qubitick_db::hash::hash_expr(e, &sh))
}

#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_model::AtomicComposite;

    fn sym(id: u32) -> Expr {
        Expr::Sym(AtomicComposite::new(id))
    }

    #[test]
    fn equal_endpoints_reach() {
        let ctx = Ctx::from_rows("Era0/Strata0", vec![]);
        let e = sym(1);
        let r = run(&ctx, &e, &e);
        assert!(r.reached);
    }

    #[test]
    fn distinct_endpoints_report_gap() {
        let ctx = Ctx::from_rows("Era0/Strata0", vec![]);
        let r = run(&ctx, &sym(1), &sym(2));
        assert!(!r.reached);
        assert!(r.gap.is_some());
    }
}

/* ══════════════════════════════════════════════════════════════════════════
 * The ruled search: the same BFS, but over the corpus's own equations.
 *
 * The neighbour walk above was a skeleton — it substitutes the head symbol and
 * hopes. A chain row is justified by a *citation*, so the search should expand by
 * applying citable equations and remember which one produced each state. That is
 * what makes its answer usable: it returns `a ∩ 1 → a` **because of `[A1.5]`**,
 * not because the shapes looked close.
 * ══════════════════════════════════════════════════════════════════════════ */

use qubitick_rewrite::diff::RewriteRule;
use qubitick_rewrite::engine::try_rewrite;

/// One step of a cited bridge: the state, and the citation that produced it.
#[derive(Debug, Clone)]
pub struct CitedStep {
    pub expr: Expr,
    /// `A1.5`, `D.BT.02`, `T.BT.44`; `None` for the starting expression.
    pub cite: Option<String>,
}

/// A bridge found by applying citable equations, with its citations.
#[derive(Debug, Clone)]
pub struct CitedChain {
    pub reached: bool,
    pub steps: Vec<CitedStep>,
    /// On failure: the reachable state closest to `rhs`.
    pub closest: Option<Expr>,
    /// On failure: the differing leaves of `closest` vs `rhs`.
    pub gap: Option<Vec<(String, String)>>,
}

/// What the search remembers about a state: the expression, where it came from,
/// which citation produced it, and how deep it is.
type Node = (Expr, Option<usize>, Option<String>, usize);

/// Bounded BFS from `lhs` to `rhs` over `(citation, equation)` pairs.
///
/// Depth and frontier are bounded (`MAX_DEPTH`, `MAX_VISITED`): this answers an
/// author's question interactively, it is not a completeness claim. A miss still
/// returns the closest state and the leaves that differ, which is the useful half.
pub fn run_ruled(lhs: &Expr, rhs: &Expr, rules: &[(String, RewriteRule)]) -> CitedChain {
    let mut nodes: Vec<Node> = vec![(lhs.clone(), None, None, 0)];
    let mut visited: HashMap<String, usize> = HashMap::new();
    visited.insert(key(lhs), 0);
    let mut queue: VecDeque<usize> = VecDeque::from([0usize]);

    let mut best = 0usize;
    let mut best_dist = structural_distance(lhs, rhs);

    while let Some(i) = queue.pop_front() {
        let (state, _, _, depth) = nodes[i].clone();
        if qubitick_pattern::expr_norm_eq(&state, rhs) {
            return rebuild(&nodes, i, true, None);
        }
        let d = structural_distance(&state, rhs);
        if d < best_dist {
            best_dist = d;
            best = i;
        }
        if depth >= MAX_DEPTH || visited.len() >= MAX_VISITED {
            continue;
        }
        for (cite, rule) in rules {
            for (next, _step) in try_rewrite(&state, rule) {
                let k = key(&next);
                if visited.contains_key(&k) {
                    continue;
                }
                visited.insert(k, nodes.len());
                nodes.push((next, Some(i), Some(cite.clone()), depth + 1));
                queue.push_back(nodes.len() - 1);
            }
        }
    }

    let gap = diff_pairs(&nodes[best].0, rhs);
    rebuild(&nodes, best, false, Some(gap))
}

/// Walk parent links back to the start, producing the chain in order.
fn rebuild(
    nodes: &[Node],
    end: usize,
    reached: bool,
    gap: Option<Vec<(String, String)>>,
) -> CitedChain {
    let mut steps: Vec<CitedStep> = Vec::new();
    let mut at = Some(end);
    while let Some(i) = at {
        let (expr, parent, cite, _) = &nodes[i];
        steps.push(CitedStep {
            expr: expr.clone(),
            cite: cite.clone(),
        });
        at = *parent;
    }
    steps.reverse();
    let closest = if reached {
        None
    } else {
        steps.last().map(|s| s.expr.clone())
    };
    CitedChain {
        reached,
        steps,
        closest,
        gap,
    }
}
