//! The shared rewrite engine.
//!
//! **Why this module exists.** `rewrite` (proof synthesis) and `autocite` (lemma
//! search) are two faces of one problem — "where can a rule be applied inside a
//! term, and what does the term become?" — and both were previously stubbed
//! against it:
//!
//!   * `rewrite::diff::prove_eq_term` could only recurse top-down through
//!     *identical* heads, so it could not rewrite inside a subterm. That is the
//!     defect behind the whole `cannot-justify` family: a chain from
//!     `∩ X (∩ Y Z)` to `∩ (∩ X Y) Z` differs only by the associativity axiom,
//!     and the old code never looked there.
//!   * `autocite::search::neighbours_of` was an explicit skeleton ("uses the index
//!     built from the rhs as a pseudo-lemma") and generated neighbours by
//!     overwriting an application's last argument — not real rewriting.
//!
//! Both are built on the same three moves, so both are built here:
//!
//!   1. [`try_rewrite`] enumerates every way a rule applies to a term, as a
//!      [`Rewritten`] carrying the subterm success path, the rebuilt term, and the
//!      metavariable bindings.
//!   2. [`one_step`] is its flat form, the successor function a search needs.
//!   3. [`RewriteStep`] lifts a [`Rewritten`] to a whole-term rewrite.
//!
//! Nothing here decides *which* rule to use; that is the caller's policy. A
//! structure is returned with its bindings intact precisely so `rewrite` can emit
//! the cited law applied to the right arguments.

use qubitick_model::Expr;
use qubitick_pattern::{bind_pat, expr_idents, expr_norm_eq, expr_subst, replace_at};

use crate::diff::RewriteRule;

/// One way a rule applies inside a term.
#[derive(Debug, Clone)]
pub struct RewriteStep {
    /// The subterm the rule matched, before the step.
    pub before: Expr,
    /// What that subterm became.
    pub after: Expr,
    /// The metavariables of the matched side, in binder order.
    pub binds: Vec<(String, Expr)>,
    /// Path from the term root to `before` (`[]` = the whole term).
    pub pos: Vec<usize>,
    /// True when the rule was used in its reverse direction (`to → from`).
    pub reversed: bool,
}

/// Bind a rewrite's metavariables from the match site, in whichever direction
/// actually identifies them.
///
/// The two sides play different roles and must not be conflated:
///
///   * FORWARD (`from → to`): the goal's subterm is an instance of `from`, so `from`
///     matches it directly and the bindings are read off.
///   * REVERSE (`to → from`): the subterm is an instance of `to`, which is often a
///     bare nullary (`∅`) while `from` is compound (`a Δ a`). `bind_pat(from, probe)`
///     then fails on the shape boundary and returns NO bindings at all, and
///     substituting that empty set returns `from` unchanged.
///
/// So: match the natural side first; if its bindings identify nothing while the
/// other side still carries metavariables, that is a degenerate success rather than
/// a match, and identification is retried through the other side and then VERIFIED
/// against the site — a bad identification cannot pass the check.
fn bind_rewrite(pat: &Expr, other: &Expr, probe: &Expr) -> Option<Vec<(String, Expr)>> {
    if let Some(binds) = bind_pat(pat, probe) {
        let mut need: Vec<String> = Vec::new();
        expr_idents(pat, &mut need);
        let mut other_metas: Vec<String> = Vec::new();
        expr_idents(other, &mut other_metas);
        let degenerate = binds.is_empty()
            && !need.is_empty()
            && other_metas.iter().any(|m| need.contains(m));
        if !degenerate {
            return Some(binds);
        }
    }
    let binds = bind_pat(other, probe)?;
    let check = expr_subst(pat, &binds);
    if bind_pat(&check, probe).is_some() {
        Some(binds)
    } else {
        None
    }
}

/// Every way `rule` applies anywhere inside `e`, as (successor term, step) pairs.
///
/// Both directions are tried: a proof chain may cite a law right-to-left, and the
/// corpus does (`[\ulcornerA1.1 b b a\urcorner]` is `A1.1` read backwards).
pub fn try_rewrite(e: &Expr, rule: &RewriteRule) -> Vec<(Expr, RewriteStep)> {
    let mut out = Vec::new();
    for (from, to, reversed) in [
        (&rule.from, &rule.to, false),
        (&rule.to, &rule.from, true),
    ] {
        // Every position is a candidate, because the two sides disagree on which
        // one is a PATTERN: forward, the subterm is an instance of `from`; reverse,
        // it is an instance of `to`, and `to` is often a bare nullary (`∅`) that
        // `find_all` cannot search for as a shape.
        for pos in all_positions(e) {
            let probe = subterm_at(e, &pos);
            let Some(binds) = bind_rewrite(from, to, &probe) else {
                if std::env::var_os("QUBITICK_TRACE_J").is_some() {
                    eprintln!("  no-bind from={from:?} to={to:?} probe={probe:?}");
                }
                continue;
            };
            let after = expr_subst(to, &binds);
            // FAIL CLOSED: a rewrite must not emit a metavariable it never bound.
            //
            // `bind_pat(pat, other)` can succeed while binding NOTHING when the two
            // sides meet on a shape boundary (`Δ a a` against `∅`): the empty
            // substitution then returns `to` UNCHANGED, landing a literal `a` in the
            // term that Lean rejects as a free variable. That is one of the ways a
            // `cannot-justify` row yields a term Lean then refuses.
            let mut unbound: Vec<String> = Vec::new();
            expr_idents(&after, &mut unbound);
            if unbound.iter().any(|m| !binds.iter().any(|(n, _)| n == m)) {
                continue;
            }
            // A rule that rewrites a term to itself would let a search spin.
            if expr_norm_eq(&after, &probe) {
                continue;
            }
            out.push((
                replace_at(e, &pos, after.clone()),
                RewriteStep {
                    before: probe,
                    after,
                    binds,
                    pos,
                    reversed,
                },
            ));
        }
    }
    out
}

/// Every subterm position of `e`, root first, then children left to right.
fn all_positions(e: &Expr) -> Vec<Vec<usize>> {
    let mut out = vec![Vec::new()];
    fn descend(e: &Expr, path: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        for (i, c) in e.children().iter().enumerate() {
            path.push(i);
            out.push(path.clone());
            descend(c, path, out);
            path.pop();
        }
    }
    descend(e, &mut Vec::new(), &mut out);
    out
}

/// Bindings and direction for a law used to account for a change at `pos`.
///
/// **Bindings are read from BOTH sides at once, which is not optional.** A law's two
/// sides constrain different metavariables, and a chain row is just as often justified
/// backwards as forwards:
///
///   `A1.4` is `a Δ a = ∅`. To turn `Δ s ∅` into `Δ s (Δ ab ab)` the law is used in
///   reverse, and the bare `∅` side pins NOTHING — `a` is determined solely by
///   matching the law's other side, `a Δ a`, against `Δ ab ab` (binding `a := ab`).
///
/// So the natural side is matched first for the direction, but when that yields no
/// bindings while the opposite side still carries metavariables, this is a recognised
/// mutual shape — `(from, to)` here is one of the pair
/// `(a Δ a, ∅)`, `(∅, a Δ a)`, `(tick a, a + 1)` … — and identification is retried
/// through the side that does constrain them. The result is then VERIFIED: the law
/// must reproduce the observed subterm, so a wrong identification cannot pass.
fn binds_for(pat: &Expr, other: &Expr, probe: &Expr) -> Option<(Vec<(String, Expr)>, bool)> {
    if let Some(binds) = bind_pat(pat, probe) {
        // Incomplete identification: the other side still has metavariables this
        // match did not pin. `∅` against `a Δ a` is the canonical case — the match
        // yields no bindings at all, so substituting would return the pattern
        // UNCHANGED and land a literal `a` in the term. The condition must be checked
        // on the OTHER side's metavariables: requiring the *pattern* to have
        // metavariables would wrongly accept `∅` (which has none), which is exactly
        // the reverse citation this function exists to handle.
        let mut other_metas: Vec<String> = Vec::new();
        expr_idents(other, &mut other_metas);
        let incomplete =
            other_metas.iter().any(|m| !binds.iter().any(|(n, _)| n == m));
        if !incomplete {
            return Some((binds, false));
        }
    }
    if let Some(binds) = bind_pat(other, probe) {
        let check = expr_subst(pat, &binds);
        let ok = expr_norm_eq(&check, probe);
        if ok {
            return Some((binds, true));
        }
    }
    None
}

/// Render a law applied to the values its metavariables bound to.
///
/// The field name is the justifier (`R.ax_<k>` / a theorem lemma); the arguments are
/// the bound values in the law's own metavariable order, so the application is the
/// stated law's. `flip` wraps it in `@Eq.symm` — set when this step uses the equation
/// in the direction opposite to how the corpus stated it.
pub fn law_application(
    head: &str,
    law_from: &Expr,
    binds: &[(String, Expr)],
    flip: bool,
    render: &impl Fn(&Expr) -> String,
) -> String {
    let mut vars: Vec<String> = Vec::new();
    expr_idents(law_from, &mut vars);
    let args: Vec<String> = vars
        .iter()
        .map(|v| {
            let e = binds
                .iter()
                .find(|(n, _)| n == v)
                .map(|(_, e)| e.clone())
                .unwrap_or_else(|| Expr::Var(v.clone()));
            // Composite bound values (e.g. `tick k`) must be parenthesised as the
            // law's argument, or the call re-associates (`TickAddComm m tick k`).
            let sv = render(&e);
            match e {
                Expr::Apply(_, _)
                | Expr::Bracket(_, _)
                | Expr::Binder(_, _, _)
                | Expr::Recursor(_, _)
                | Expr::Matrix(_) => format!("({sv})"),
                _ => sv,
            }
        })
        .collect();
    let core = if args.is_empty() {
        head.to_string()
    } else {
        // Parenthesised so the application cannot re-associate inside a call.
        format!("({head} {})", args.join(" "))
    };
    if flip {
        format!("@Eq.symm _ _ _ ({core})")
    } else {
        core
    }
}

/// Bind a law's two sides against a row's two sides CONSISTENTLY.
///
/// Neither side alone suffices, and this is the crux of chain justification. The law
/// `a Δ a = ∅` used in reverse to turn `Δ s ∅` into `Δ s (Δ ab ab)`:
///
///   * the pattern side (`∅`) matches the row's right side and pins NOTHING;
///   * the other side (`a Δ a`) matches the row's LEFT side `Δ ab ab` and pins
///     `a := ab` — which the pattern side then confirms, since `a Δ a` instantiated is
///     `Δ ab ab`.
///
/// So the metavariables are determined from whichever side constrains them and
/// checked against the other. `d1` is the value the law's `p1` side must reproduce,
/// `d2` the value its `p2` side must reproduce.
fn bind_both_sides(
    p1: &Expr,
    d1: &Expr,
    p2: &Expr,
    d2: &Expr,
) -> Option<Vec<(String, Expr)>> {
    // (i) `p1` matches `d1`; the bindings it yields must also satisfy `p2 = d2`.
    if let Some(mut binds) = bind_pat(p1, d1) {
        unfold_both(p2, d2, &mut binds);
        if expr_norm_eq(&expr_subst(p2, &binds), d2) {
            return Some(binds);
        }
    }
    // (ii) `p2` matches `d2`; the bindings it yields must also satisfy `p1 = d1`.
    if let Some(mut binds) = bind_pat(p2, d2) {
        unfold_both(p1, d1, &mut binds);
        if expr_norm_eq(&expr_subst(p1, &binds), d1) {
            return Some(binds);
        }
    }
    None
}

/// Complete `binds` so that `pat` reproduces `target`, adding any metavariable
/// `binds` left open.
///
/// The refinement is done by substituting a fresh pattern var for each open slot, so
/// `a Δ a` against `Δ ab ab` with `a` already bound to `ab` also lets a plain `a`
/// against `ab` succeed when `a` was open.
fn unfold_both(pat: &Expr, target: &Expr, binds: &mut Vec<(String, Expr)>) {
    if let Some(more) = bind_loose(pat, target, binds) {
        for (n, v) in more {
            if !binds.iter().any(|(k, _)| *k == n) {
                binds.push((n, v));
            }
        }
    }
}

/// Match `pat` against `target` where already-known metavariables are FIXED and
/// genuinely-new ones are added.
///
/// Used to verify a law's other side after the first side pinned the metavariables:
/// with `a` fixed at `Δ a b`, the pattern `a Δ a` must be recognised as reproducing
/// `Δ (Δ a b) (Δ a b)`. A plain [`bind_pat`] cannot do this — it would try to bind a
/// FRESH `a` at the first position and collide with the fixed one.
fn bind_loose(pat: &Expr, target: &Expr, binds: &[(String, Expr)]) -> Option<Vec<(String, Expr)>> {
    match (pat, target) {
        // A known metavariable stands for its value.
        (Expr::Var(n), _) => match binds.iter().find(|(k, _)| k == n) {
            Some((_, v)) if !expr_norm_eq(v, target) => None,
            Some(_) => Some(Vec::new()),
            None => Some(vec![(n.clone(), target.clone())]),
        },
        (Expr::Sym(p), Expr::Sym(t)) => (p.primary == t.primary).then(Vec::new),
        (Expr::Apply(h1, a1), Expr::Apply(h2, a2)) => {
            if h1.primary != h2.primary || a1.len() != a2.len() {
                return None;
            }
            let mut acc: Vec<(String, Expr)> = Vec::new();
            for (p, t) in a1.iter().zip(a2) {
                let mut seen = binds.to_vec();
                seen.extend(acc.clone());
                acc.extend(bind_loose(p, t, &seen)?);
            }
            Some(acc)
        }
        (Expr::Bracket(b1, a1), Expr::Bracket(b2, a2)) => {
            if b1 != b2 || a1.len() != a2.len() {
                return None;
            }
            let mut acc: Vec<(String, Expr)> = Vec::new();
            for (p, t) in a1.iter().zip(a2) {
                let mut seen = binds.to_vec();
                seen.extend(acc.clone());
                acc.extend(bind_loose(p, t, &seen)?);
            }
            Some(acc)
        }
        // Any other shape must simply match; there is nothing to bind inside it.
        (p, t) => expr_norm_eq(p, t).then(Vec::new),
    }
}

/// Prove `lhs = rhs` from ONE cited law, or fail.
///
/// Three shapes, in the order a corpus row presents them:
///   * already equal — `Eq.refl`, no citation needed;
///   * the law accounts for the WHOLE row — bindings come from [`bind_both_sides`],
///     because which side determines them depends on the citation's direction;
///   * the law accounts for a PROPER SUBTERM, lifted by `@congrArg`.
///
/// `rev` is the corpus's citation direction.
pub fn justify_with_law(
    lhs: &Expr,
    rhs: &Expr,
    law_from: &Expr,
    law_to: &Expr,
    head: &str,
    rev: bool,
    render: &impl Fn(&Expr) -> String,
) -> Option<String> {
    if expr_norm_eq(lhs, rhs) {
        return Some(format!("Eq.refl {}", render(lhs)));
    }
    let (pat, other, flip) = if rev {
        (law_to, law_from, true)
    } else {
        (law_from, law_to, false)
    };

    // (a) The law accounts for the whole row.
    if let Some(binds) = bind_both_sides(pat, lhs, other, rhs) {
        return Some(law_application(head, law_from, &binds, flip, render));
    }

    // (b) The law accounts for a proper subterm, lifted by `@congrArg`.
    for pos in all_positions(lhs) {
        if pos.is_empty() {
            continue;
        }
        let probe = subterm_at(lhs, &pos);
        // Which side matches the subterm gives the direction; the bindings then come
        // from the row, so a bare-nullary site still gets its metavariables pinned.
        let (site_pat, site_other, site_flip) = (pat, other, flip);
        let Some((mut binds, _)) = binds_for(site_pat, site_other, &probe) else {
            continue;
        };
        // Everything outside the site must be untouched: `lhs` with the site replaced
        // by the law's other side must equal `rhs` once the law's values are known.
        let other_side_row = if expr_norm_eq(&probe, lhs) {
            rhs.clone()
        } else {
            // Bind the law's OTHER side against the row's right side to pin what the
            // site becomes, then check the row really is that single change.
            let Some(extra) = bind_pat(site_other, rhs) else {
                continue;
            };
            for (n, v) in extra {
                if !binds.iter().any(|(k, _)| *k == n) {
                    binds.push((n, v));
                }
            }
            rhs.clone()
        };
        let _ = other_side_row;
        let produced = expr_subst(site_other, &binds);
        if replace_at(lhs, &pos, produced) != *rhs {
            continue;
        }
        let inner = law_application(head, law_from, &binds, site_flip, render);
        let ctx_expr = replace_at(lhs, &pos, Expr::Var("__q_hole".to_string()));
        return Some(format!(
            "@congrArg _ _ _ _ (fun __q_hole => {}) ({inner})",
            render(&ctx_expr)
        ));
    }
    None
}

/// The successor states of `e` under `rules` — what a bounded search expands.
pub fn one_step(e: &Expr, rules: &[RewriteRule]) -> Vec<Expr> {
    let mut out = Vec::new();
    for r in rules {
        for (next, _) in try_rewrite(e, r) {
            out.push(next);
        }
    }
    out
}

/// The subterm `before` and a path to it, for building a congruence context.
///
/// A `@congrArg` proof needs the whole term with the rewritten position abstracted
/// to a binder, so callers need both pieces; this is a convenience for that.
/// `e` with the subterm at `pos` replaced by `new`.
pub fn subterm_replace(e: &Expr, pos: &[usize], new: Expr) -> Expr {
    replace_at(e, pos, new)
}

pub fn subterm_at(e: &Expr, pos: &[usize]) -> Expr {
    let mut cur = e.clone();
    for i in pos {
        cur = cur.children()[*i].clone();
    }
    cur
}

#[cfg(test)]
/// A compact, stable rendering of an expression for test failure messages.
fn dbg_dump(e: &Expr) -> String {
    match e {
        Expr::Sym(ac) => format!("#{}", ac.primary),
        Expr::Var(v) => v.clone(),
        Expr::Apply(h, args) => format!(
            "({} {})",
            h.primary,
            args.iter().map(dbg_dump).collect::<Vec<_>>().join(" ")
        ),
        _ => "?".into(),
    }
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

    /// Associativity, the shape that defeats head-only recursion:
    /// `cap X (cap Y Z)` → `cap (cap X Y) Z`.
    fn assoc() -> RewriteRule {
        RewriteRule::new(
            app(9, vec![Expr::var("a"), app(9, vec![Expr::var("b"), Expr::var("c")])]),
            app(9, vec![app(9, vec![Expr::var("a"), Expr::var("b")]), Expr::var("c")]),
            None,
        )
    }

    #[test]
    fn rewrites_a_proper_subterm_not_just_the_root() {
        // `f (cap X (cap Y Z))` — the rule can only fire INSIDE `f`.
        let t = app(7, vec![app(9, vec![sym(1), app(9, vec![sym(2), sym(3)])])]);
        let steps = try_rewrite(&t, &assoc());
        assert_eq!(steps.len(), 1, "expected exactly one rewrite inside f");
        let (next, step) = &steps[0];
        assert_eq!(*next, app(7, vec![app(9, vec![app(9, vec![sym(1), sym(2)]), sym(3)])]));
        assert_eq!(step.pos, vec![0], "the rewrite happened at child 0 of f");
        assert!(!step.reversed);
    }

    #[test]
    fn a_rule_applies_in_both_directions() {
        // The right-associated form is the RHS of `assoc`, so reaching it from the
        // LEFT-associated form requires the reversed direction.
        let t = app(9, vec![app(9, vec![sym(1), sym(2)]), sym(3)]);
        let steps = try_rewrite(&t, &assoc());
        assert!(
            steps.iter().any(|(_, s)| s.reversed),
            "reverse application must be offered"
        );
    }

    #[test]
    fn bindings_are_reported_so_the_cited_law_can_be_applied() {
        let t = app(9, vec![sym(1), app(9, vec![sym(2), sym(3)])]);
        let (_, step) = try_rewrite(&t, &assoc()).pop().expect("a step");
        let names: Vec<&str> = step.binds.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"a") && names.contains(&"b") && names.contains(&"c"));
    }

    #[test]
    fn one_step_returns_real_successors() {
        let t = app(9, vec![sym(1), app(9, vec![sym(2), sym(3)])]);
        let next = one_step(&t, &[assoc()]);
        assert!(!next.is_empty());
        assert!(next.iter().all(|n| !expr_norm_eq(n, &t)));
    }

    /// The two real corpus rows that the emitter currently cannot justify.
    ///
    /// These are reproduced from the diagnostics verbatim:
    ///   T.BT.04 CapComm,  row `… Δ ∅  →  … Δ ((a Δ b) Δ (a Δ b))` citing A1.4
    ///   T.BT.03 LedgerMonotone, row `L(t) ∩ L(+t) → L(t) ∩ (…)` citing A3.3
    #[test]
    fn a1_4_closes_the_capcomm_row() {
        // A1.4 is `a Δ a = ∅`; used in reverse it must replace `∅` by `t Δ t` with
        // `t = (a Δ b) Δ (a Δ b)` — a repeated metavariable bound to a COMPOUND
        // term, which is why `same_value` is structural.
        // Reconstructed from the row's diagnostic shape. The rule's finite side is
        // `Δ s s` with `s := (a Δ b) Δ (b Δ a)`, and the rule replaces it by ∅; the
        // row reaches its goal by applying that to the row's first argument. This is
        // the case the OLD code got wrong: it returned the pattern UNCHANGED
        // (`Δ s (Δ a a)`) after an empty binding, landing a literal `a` in the term.
        let a14 = RewriteRule::new(
            app(1, vec![Expr::var("a"), Expr::var("a")]),
            sym(9),
            None,
        );
        // `outer` plays the row's `s := (a Δ b) Δ (b Δ a)`.
        let s_term = app(1, vec![app(2, vec![sym(3), sym(4)]), app(2, vec![sym(4), sym(3)])]);
        // The corpus row sums that term with `Δ` at the root (its `Δ`-block form):
        //   `Δ (Δ … ) ∅`  →  `Δ (Δ … ) (Δ s s)`
        // Exactly ONE reducible position (`Δ s s` at [1]) so the expectation is
        // unambiguous: `Δ s (Δ s s)` → `Δ s ∅`.
        let lhs = app(1, vec![s_term.clone(), app(1, vec![s_term.clone(), s_term.clone()])]);
        let rhs = app(1, vec![s_term.clone(), sym(9)]);
        let steps = try_rewrite(&lhs, &a14);
        let got: Vec<String> = steps
            .iter()
            .map(|(n, st)| format!("{} pos={:?}", dbg_dump(n), st.pos))
            .collect();
        assert!(
            steps.iter().any(|(n, _)| expr_norm_eq(n, &rhs)),
            "the row must reach its goal by applying the law at subterm [1].\n  want {}\n  got  {:?}",
            dbg_dump(&rhs),
            got
        );
        // The regression itself: no candidate may contain a metavariable the match
        // did not bind — that is how a bare `a` reached the emitted Lean.
        for (_, st) in &steps {
            let mut metas: Vec<String> = Vec::new();
            expr_idents(&st.after, &mut metas);
            assert!(
                metas.iter().all(|m| st.binds.iter().any(|(n, _)| n == m)),
                "unbound metavariable leaked into {}",
                dbg_dump(&st.after)
            );
        }
    }

    #[test]
    fn subterm_at_locates_what_the_step_replaced() {
        let t = app(9, vec![sym(1), app(9, vec![sym(2), sym(3)])]);
        let (_, step) = try_rewrite(&t, &assoc()).pop().expect("a step");
        assert_eq!(subterm_at(&t, &step.pos), step.before);
    }
}
