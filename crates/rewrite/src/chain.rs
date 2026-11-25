//! `Chain` step compilation: an `@Eq.trans` composition (Document 5 §8.2.1).
//!
//! An equality `Chain` is not a tactic `calc` block; it is a CIC kernel term
//! built from nested `@Eq.trans` applications. Each row contributes a term proving
//! `Eq lhs rhs`, and rows are folded right-to-left.

use crate::env::Env;
use crate::termctx::TermCtx;
use anyhow::{Result, bail};
use qubitick_ctx::Ctx;
use qubitick_lean::render as lean_render;
use qubitick_model::{ChainJust, ChainRow, Expr};

/// Compile a `ProofStep::Chain` into a `@Eq.trans` term.
///
/// For rows `[a = b, b = c]` the result is
/// `@Eq.trans _ a b c <term a= b> (@Eq.trans _ b c c <term b=c> (Eq.refl c))`.
/// A trailing identity `Eq.refl <last_rhs>` closes the composition.
pub fn compile_chain(
    ctx: &Ctx,
    tc: &TermCtx,
    env: &Env,
    rows: &[ChainRow],
    rules: &crate::rules::RuleTable,
) -> Result<String> {
    if rows.is_empty() {
        /* NO ROWS IS NOT A PROOF. This returned the bare text `Eq.refl`, which the emitter
           wrote as `fun a => Eq.refl _` — so an unfinished proof of a DEFINITIONAL identity
           (`a Δ ∅ = a` unfolds to `a`) type-checked and the corpus reported `verified`, while
           anything else failed with a Lean `Type mismatch` that never mentioned the missing
           proof. An admitted theorem is emitted as an axiom (`crates/emit/src/theorem.rs`),
           and a chain that reaches here with no rows is a caller bug. */
        bail!("a proof chain with no rows is not a proof — an admitted theorem is an axiom");
    }

    let mut seen: Vec<(String, String, String)> = Vec::new();
    let mut prev_rhs_expr: Option<Expr> = None;
    for row in rows {
        let lhs_expr = match &row.lhs {
            Some(l) => l.clone(),
            None => prev_rhs_expr.clone().unwrap_or_else(|| row.rhs.clone()),
        };
        let lhs = lean_render(&lhs_expr, &|id| tc.resolve(id));
        let rhs = lean_render(&row.rhs, &|id| tc.resolve(id));
        // A failure here is a failure of ONE written row: say which, because the
        // row's own two sides plus its index are what locate the line in the file.
        let just = render_just(ctx, tc, env, &row.just, &lhs_expr, &row.rhs, rules)
            .map_err(|e| anyhow::anyhow!("row {}: {e}", row.line))?;
        seen.push((lhs.clone(), rhs.clone(), just));
        prev_rhs_expr = Some(row.rhs.clone());
    }

    Ok(fold_trans(&seen))
}

/// Fold the collected row terms into nested `@Eq.trans` applications.
fn fold_trans(rows: &[(String, String, String)]) -> String {
    // The whole chain ends at the LAST row's rhs. Start from the end, closed by
    // `Eq.refl <last-rhs>`, and fold forward: `@Eq.trans _ lhs_i rhs_i final
    // (just_i) (rest)` — the final endpoint is the LAST row's rhs throughout, not
    // the current row's rhs (the old code used `(rhs) (rhs)`, which only held for a
    // single-row chain and produced mis-typed terms for multi-step proofs).
    let final_ep = rows
        .last()
        .map(|r| r.1.clone())
        .unwrap_or_default();
    let mut term = format!("Eq.refl ({final_ep})");
    for (lhs, rhs, just) in rows.iter().rev() {
        term = format!("@Eq.trans _ ({lhs}) ({rhs}) ({final_ep}) ({just}) ({term})");
    }
    term
}

/// Compile the per-row justification into a term proving `Eq lhs rhs`.
///
/// Closed forms:
///   * `[\equiv]`             — kernel conversion (definitional / recursor step)
///   * `[h]` / `[hN]` / `[^i]` — a local hypothesis in scope (via `env`)
///   * `[A1.N]` / `[D.BT.N]`  — an axiom line or definitional rule
///   * `[rev X]`              — parser-side; must not appear here (see the bail below)
fn render_just(
    ctx: &Ctx,
    tc: &TermCtx,
    env: &Env,
    just: &ChainJust,
    lhs: &Expr,
    rhs: &Expr,
    rules: &crate::rules::RuleTable,
) -> Result<String> {
    let _ = ctx;
    let l = lean_render(lhs, &|id| tc.resolve(id));
    let r = lean_render(rhs, &|id| tc.resolve(id));
    if qubitick_pattern::expr_norm_eq(lhs, rhs) {
        return Ok(format!("Eq.refl ({l})"));
    }
    match just {
        ChainJust::Rfl => bail!("rfl-justified row is not syntactically reflexive: {l} ≠ {r}"),
        ChainJust::Rewrite { refs } => {
            if refs.is_empty() {
                bail!("rewrite row ({l} → {r}) has no citation and the sides differ")
            }
            let mut tried: Vec<String> = Vec::new();
            for rf in refs {
                // ---- Local citation: hypothesis / IH / \equiv marker ----
                if let qubitick_model::CiteName::Local(v) = &rf.name {
                    let name = v.as_str(); // <--- Notice: no .0
                    match name {
                        "\\equiv" | "equiv" | "≡" => {
                            return Ok(format!("Eq.refl ({l})"));
                        }
                        "rev" => bail!(
                            "citation `[rev …]` reached the justifier as the bare \
                             name `rev` — the citation lexer in `crates/parse/src/` \
                             is dropping the wrapped name AND the `rev` prefix. Fix \
                             the lexer, not this function."
                        ),
                        hyp_name => {
                            // Now works perfectly with Option<Expr>
                            if let Some(hyp_ty) = lookup_hyp(env, hyp_name) {
                                if let Some(term) =
                                    justify_hypothesis(hyp_name, &hyp_ty, lhs, rhs, rf.rev, tc)
                                {
                                    return Ok(term);
                                }
                                tried.push(format!("{hyp_name} (hypothesis; does not match)"));
                                continue;
                            }
                            tried.push(format!("{hyp_name} (no such hypothesis in scope)"));
                            continue;
                        }
                    }
                }
                // ---- Axiom / theorem / definition citation ----
                let cite = crate::rewrite::cite_name(&rf.name);
                let Some((node, line)) = cite_axiom_line(&rf.name) else {
                    tried.push(format!("{cite} (not an axiom line)"));
                    continue;
                };
                let Some(rule) = rules.get(node, line) else {
                    tried.push(format!("{cite} (not in this module)"));
                    continue;
                };
                if let Some(term) = justify_step(rule, lhs, rhs, rf.rev, tc) {
                    return Ok(term);
                }
                if let Some(term) = justify_multi(rules, refs, lhs, rhs, tc) {
                    return Ok(term);
                }
                let (_, body) = crate::rules::strip_binders(&rule.statement);
                tried.push(format!(
                    "{cite} (does not match; rule is `{}`)",
                    lean_render(&body, &|id| tc.resolve(id))
                ));
            }
            bail!(
                "cannot justify rewrite ({l} → {r}) citing [{}]",
                tried.join(", ")
            )
        }
        ChainJust::Exact { term } => Ok(lean_render(term, &|id| tc.resolve(id))),
        ChainJust::Sym(_) => bail!("cannot synthesize an Eq.symm rewrite term for ({l} → {r})"),
    }
}

fn cite_axiom_line(name: &qubitick_model::CiteName) -> Option<(u32, u32)> {
    match name {
        qubitick_model::CiteName::AxiomLine { node, line } => Some((*node, *line)),
        // Definitions are keyed as node `0` at their corpus division index.
        qubitick_model::CiteName::DefId(div) => Some((0, *div)),
        qubitick_model::CiteName::TheoremId(div) => Some((u32::MAX, *div)),
        _ => None,
    }
}

/// Build the term `R.ax_<k> a b c` justifying `lhs = rhs`.
///
/// The axiom statement is stripped of its `∀` prefix and its left-hand side is
/// matched against `lhs`; the bound values are then passed in binder order.
/// When `rev` is set the citation is used in the opposite direction and the
/// result is wrapped in `@Eq.symm`.
/// Hole name standing for the rewritten position in a congruence lambda.
const HOLE: &str = "__q_hole";

/// A cited law ready to be applied: its two sides and the `∀` variables.
struct Law<'a> {
    rule: &'a crate::rules::AxRule,
    pat: Expr,
    tgt: Expr,
    vars: Vec<String>,
    rev: bool,
}

fn law_of(rule: &crate::rules::AxRule, rev: bool) -> Option<Law<'_>> {
    // Definitional unfolding is a kernel-conversion step, handled by the direct
    // justifier; it is not a rewrite the search can compose with.
    if rule.definitional {
        return None;
    }
    let (_vars, body) = crate::rules::strip_binders(&rule.statement);
    let Expr::Apply(head, args) = &body else {
        return None;
    };
    if head.primary != qubitick_model::EQ_SYM || args.len() != 2 {
        return None;
    }
    // The emitted class field binds ONLY the variables its body uses (see emit's
    // `reduce_field_statement`); the citation must use the SAME arity. So the Law's
    // variables are the body's free idents, not the axiom's full `∀ a b c` list
    // (whose unused trailing binders would over-apply the field).
    let vars = body_idents(&body);
    let (pat, tgt) = if rev {
        (args[1].clone(), args[0].clone())
    } else {
        (args[0].clone(), args[1].clone())
    };
    Some(Law {
        rule,
        pat,
        tgt,
        vars,
        rev,
    })
}

/// Consistent binding of a law's two sides against a goal's two sides.
///
/// The law `p1 = p2` justifies `v1 = v2` when `p1` matches `v1` and `p2` matches
/// `v2` under one consistent substitution. Matching BOTH sides is essential: a
/// law like `a Δ a = ∅` used backwards to replace `∅` by `(a Δ b) Δ (a Δ b)`
/// only determines `a` from the goal's OTHER side.
fn bind_pair(p1: &Expr, v1: &Expr, p2: &Expr, v2: &Expr) -> Option<Vec<(String, Expr)>> {
    let mut b = qubitick_pattern::bind_pat(p1, v1)?;
    for (k, v) in qubitick_pattern::bind_pat(p2, v2)? {
        match b.iter().find(|(n, _)| *n == k) {
            Some((_, e)) if !qubitick_pattern::expr_norm_eq(e, &v) => return None,
            Some(_) => {}
            None => b.push((k, v)),
        }
    }
    if qubitick_pattern::expr_norm_eq(&qubitick_pattern::expr_subst(p2, &b), v2) {
        Some(b)
    } else {
        None
    }
}

fn law_term(
    law: &Law,
    bindings: &[(String, Expr)],
    reversed: bool,
    tc: &TermCtx,
) -> Option<String> {
    let mut rendered: Vec<String> = Vec::with_capacity(law.vars.len());
    for v in &law.vars {
        // A variable the match did not pin is taken to be the corpus's own
        // metavariable of the same name (the corpus and the laws share names).
        let bound = bindings
            .iter()
            .find(|(n, _)| n == v)
            .map(|(_, e)| e.clone())
            .unwrap_or_else(|| Expr::Var(v.clone()));
        // Composite bound values (e.g. `tick k`) must stay ONE argument: the
        // called law would otherwise re-associate (`TickAddComm m tick k`).
        let sv = qubitick_lean::render(&bound, &|id| tc.resolve(id));
        match &bound {
            Expr::Apply(_, _)
            | Expr::Bracket(_, _)
            | Expr::Binder(_, _, _)
            | Expr::Recursor(_, _)
            | Expr::Matrix(_) => rendered.push(format!("({sv})")),
            _ => rendered.push(sv),
        }
    }
    let field = law.rule.head.clone();
    // Parenthesize an applied law so it cannot re-associate inside a call.
    let core = if rendered.is_empty() {
        field
    } else {
        format!("({field} {})", rendered.join(" "))
    };
    Some(if reversed {
        format!("@Eq.symm _ _ _ ({core})")
    } else {
        core
    })
}

/// Rewrite `expr` at every position where `law.pat` matches.
///
/// Yields the rewritten expression together with a Lean term proving
/// `Eq expr new` — the law applied directly, or under `@congrArg` when the match
/// is at a proper subterm.
fn rewrite_all_walker(law: &Law, expr: &Expr, tc: &TermCtx) -> Vec<(Expr, String)> {
    let mut out = Vec::new();
    if let Some(b) = qubitick_pattern::bind_pat(&law.pat, expr) {
        let new = qubitick_pattern::expr_subst(&law.tgt, &b);
        if !qubitick_pattern::expr_norm_eq(&new, expr)
            && let Some(term) = law_term(law, &b, law.rev, tc)
        {
            out.push((new, term));
        }
    }
    // Reversed orientation: the law's other side matches here.
    if let Some(b) = qubitick_pattern::bind_pat(&law.tgt, expr) {
        let new = qubitick_pattern::expr_subst(&law.pat, &b);
        if !qubitick_pattern::expr_norm_eq(&new, expr)
            && let Some(term) = law_term(law, &b, !law.rev, tc)
        {
            out.push((new, term));
        }
    }
    // Descend one level, closing the step by congruence.
    if let Expr::Apply(h, args) = expr {
        for i in 0..args.len() {
            for (new_arg, step) in rewrite_all_walker(law, &args[i], tc) {
                let mut after = args.clone();
                after[i] = new_arg;
                let mut hole_args = args.clone();
                hole_args[i] = Expr::Var(HOLE.to_string());
                let hole_ctx = Expr::Apply(h.clone(), hole_args);
                out.push((
                    Expr::Apply(h.clone(), after),
                    format!(
                        "@congrArg _ _ _ _ (fun {HOLE} => {}) ({step})",
                        lean_render(&hole_ctx, &|id| tc.resolve(id))
                    ),
                ));
            }
        }
    }
    out
}


/// Every one-step rewrite of `expr` under `law`, as (successor, Lean term).
///
/// UNION of two generators, deliberately:
///   * `rewrite_all_walker` — the original positional descent. Where it succeeds it
///     produces the term the rest of the pipeline was tuned against.
///   * the shared `engine::try_rewrite` — reads a law's metavariables off whichever
///     side constrains them, so it reaches sites the walker cannot (a reverse
///     citation whose determining side is a bare nullary, and associativity
///     reshaping a nested operator).
///
/// Neither dominates: routing `justify_multi` through the engine ALONE measured
/// built 128 -> 127, and the walker alone leaves the sites above unreachable. The
/// search needs both, so this concatenates them and drops duplicates by rendered
/// term. Order puts the walker first so existing successes are unchanged.
fn rewrite_all(law: &Law, expr: &Expr, tc: &TermCtx) -> Vec<(Expr, String)> {
    let mut out = rewrite_all_walker(law, expr, tc);
    let rule =
        crate::diff::RewriteRule::new(law.pat.clone(), law.tgt.clone(), law.rule.head.clone());
    for (next, step) in crate::engine::try_rewrite(expr, &rule) {
        let Some(core) = law_term(law, &step.binds, step.reversed, tc) else {
            continue;
        };
        let term = if step.pos.is_empty() {
            core
        } else {
            let ctx_expr =
                qubitick_pattern::replace_at(expr, &step.pos, Expr::Var(HOLE.to_string()));
            format!(
                "@congrArg _ _ _ _ (fun {HOLE} => {}) ({core})",
                lean_render(&ctx_expr, &|id| tc.resolve(id))
            )
        };
        if !out
            .iter()
            .any(|(e, _)| qubitick_pattern::expr_norm_eq(e, &next))
        {
            out.push((next, term));
        }
    }
    out
}

/// Close `lhs = rhs` by composing the cited laws (bounded breadth-first search).
///
/// The corpus frequently justifies one row with several laws — e.g.
/// `[\ulcornerA1.4 a\urcorner, A1.1]` substitutes an instance of `a Δ a = ∅` and
/// then reassociates. Each hop is a real term, composed with `@Eq.trans`.
fn justify_multi(
    rules: &crate::rules::RuleTable,
    refs: &[qubitick_model::JustRef],
    lhs: &Expr,
    rhs: &Expr,
    tc: &TermCtx,
) -> Option<String> {
    const MAX_DEPTH: usize = 4;
    const MAX_STEPS: usize = 600;
    let laws: Vec<Law> = refs
        .iter()
        .filter_map(|r| {
            let (node, line) = cite_axiom_line(&r.name)?;
            law_of(rules.get(node, line)?, r.rev)
        })
        .collect();
    if laws.is_empty() {
        return None;
    }
    let render = |e: &Expr| lean_render(e, &|id| tc.resolve(id));
    let l = render(lhs);
    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    visited.insert(l.clone());
    let mut queue: std::collections::VecDeque<(Expr, String, usize)> =
        std::collections::VecDeque::new();
    queue.push_back((lhs.clone(), String::new(), 0));
    let mut steps = 0usize;
    while let Some((cur, term, depth)) = queue.pop_front() {
        if depth >= MAX_DEPTH {
            continue;
        }
        for law in &laws {
            for (next, step) in rewrite_all(law, &cur, tc) {
                let n = render(&next);
                if !visited.insert(n.clone()) {
                    continue;
                }
                steps += 1;
                if steps > MAX_STEPS {
                    return None;
                }
                let composed = if term.is_empty() {
                    step
                } else {
                    format!(
                        "@Eq.trans _ ({l}) ({}) ({n}) ({term}) ({step})",
                        render(&cur)
                    )
                };
                if qubitick_pattern::expr_norm_eq(&next, rhs) {
                    return Some(composed);
                }
                queue.push_back((next, composed, depth + 1));
            }
        }
    }
    None
}

/// Justify `lhs = rhs` with `rule`, directly or by congruence on a subterm.
///
/// Tries EVERY mismatched argument position, not just the first. A mismatch at
/// position 0 may be "phantom" (atom-identity noise) that the cited rule cannot
/// close, while a mismatch at position 1 can — e.g. T_BT_04's
/// `ml Δ ∅ → ml Δ ((a Δ b) Δ (a Δ b))` closes by A1.4 reversed at position 1.
fn justify_step(
    rule: &crate::rules::AxRule,
    lhs: &Expr,
    rhs: &Expr,
    rev: bool,
    tc: &TermCtx,
) -> Option<String> {
    if let Some(term) = instantiate_axiom(rule, lhs, rhs, rev, tc) {
        return Some(term);
    }
    // The SHARED engine, tried only where the positional path above cannot close the
    // row. It reads a law's metavariables off whichever side constrains them, so it
    // reaches rows positional matching cannot: a reverse citation whose site is a
    // bare nullary (`a Δ a = ∅` used backwards across `… Δ ∅`), and a law that
    // reshapes a nested operator (associativity).
    let engine = || justify_via_engine(rule, lhs, rhs, rev, tc);
    match (
        qubitick_pattern::expr_strip_parens(lhs),
        qubitick_pattern::expr_strip_parens(rhs),
    ) {
        (Expr::Apply(h1, a1), Expr::Apply(h2, a2))
            if h1.primary == h2.primary && a1.len() == a2.len() =>
        {
            for i in 0..a1.len() {
                if qubitick_pattern::expr_norm_eq(&a1[i], &a2[i]) {
                    continue;
                }
                // NOTE: previously this line used `?`, which aborted the whole
                // function on the first subterm the rule could not close. It must
                // continue to the next position.
                if let Some(inner) = justify_step(rule, &a1[i], &a2[i], rev, tc) {
                    let mut args = a1.clone();
                    args[i] = Expr::Var(HOLE.to_string());
                    let hole_ctx = Expr::Apply(h1.clone(), args);
                    return Some(format!(
                        "@congrArg _ _ _ _ (fun {HOLE} => {}) ({inner})",
                        lean_render(&hole_ctx, &|id| tc.resolve(id))
                    ));
                }
            }
            engine()
        }
        _ => engine(),
    }
}

/// Justify a row with the shared engine (`rewrite::engine::justify_with_law`).
fn justify_via_engine(
    rule: &crate::rules::AxRule,
    lhs: &Expr,
    rhs: &Expr,
    rev: bool,
    tc: &TermCtx,
) -> Option<String> {
    if rule.definitional {
        return None;
    }
    let (_vars, body) = crate::rules::strip_binders(&rule.statement);
    let Expr::Apply(head, args) = &body else {
        return None;
    };
    if head.primary != qubitick_model::EQ_SYM || args.len() != 2 {
        return None;
    }
    let render = |e: &Expr| lean_render(e, &|id| tc.resolve(id));

    // The corpus cites a law by its stated direction but often USES it the other
    // way round: `[A1.4]` (`a Δ a = ∅`) discharging a row that replaces `∅` by
    // `a Δ a`. Both directions are therefore tried; the citation's `rev` hint only
    // decides which is attempted first.
    for direction in [rev, !rev] {
        if let Some(t) = crate::engine::justify_with_law(
            lhs,
            rhs,
            &args[0],
            &args[1],
            &rule.head,
            direction,
            &render,
        ) {
            return Some(t);
        }
    }

    // (b) The law accounts for ONE CHANGED SUBTERM.
    //
    // Pass the SUBTERM PAIR to the row-level justifier rather than the whole row:
    // that is what lets it read the metavariables off the side that constrains them.
    // The canonical case is `a Δ a = ∅` used backwards — the row is `X Δ ∅` →
    // `X Δ (Δ ab ab)`, and `a` is pinned ONLY by the replacement `Δ ab ab` on the
    // right; matching the left subterm (`∅`) identifies nothing.
    for pos in positions(lhs) {
        if pos.is_empty() {
            continue;
        }
        let probe_l = at(lhs, &pos);
        let probe_r = at(rhs, &pos);
        if qubitick_pattern::expr_norm_eq(&probe_l, &probe_r) {
            continue;
        }
        // Only a single-position change is a congruence step.
        if !qubitick_pattern::expr_norm_eq(
            &crate::engine::subterm_replace(lhs, &pos, probe_r.clone()),
            rhs,
        ) {
            continue;
        }
        let mut found = None;
        for direction in [rev, !rev] {
            if let Some(t) = crate::engine::justify_with_law(
                &probe_l,
                &probe_r,
                &args[0],
                &args[1],
                &rule.head,
                direction,
                &render,
            ) {
                found = Some(t);
                break;
            }
        }
        let Some(inner) = found else {
            continue;
        };
        let ctx_expr = qubitick_pattern::replace_at(lhs, &pos, Expr::Var(HOLE.to_string()));
        return Some(format!(
            "@congrArg _ _ _ _ (fun {HOLE} => {}) ({inner})",
            render(&ctx_expr)
        ));
    }
    None
}

/// Every subterm position, root first.
fn positions(e: &Expr) -> Vec<Vec<usize>> {
    fn go(e: &Expr, path: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        for (i, c) in e.children().iter().enumerate() {
            path.push(i);
            out.push(path.clone());
            go(c, path, out);
            path.pop();
        }
    }
    let mut out = vec![Vec::new()];
    go(e, &mut Vec::new(), &mut out);
    out
}

/// The subterm at `pos`.
fn at(e: &Expr, pos: &[usize]) -> Expr {
    let mut cur = e.clone();
    for i in pos {
        match cur.children().get(*i) {
            Some(c) => cur = (*c).clone(),
            None => return cur,
        }
    }
    cur
}

/// The free variables of `e`, in first-encountered order (via `expr_idents`).
fn body_idents(e: &Expr) -> Vec<String> {
    let mut out = Vec::new();
    qubitick_pattern::expr_idents(e, &mut out);
    out
}

fn instantiate_axiom(
    rule: &crate::rules::AxRule,
    lhs: &Expr,
    rhs: &Expr,
    rev: bool,
    tc: &TermCtx,
) -> Option<String> {
    let (_vars, body) = crate::rules::strip_binders(&rule.statement);
    let Expr::Apply(head, args) = &body else {
        return None;
    };
    if head.primary != qubitick_model::EQ_SYM || args.len() != 2 {
        return None;
    }
    // The law `pat = tgt` justifies `lhs = rhs` in one of two orientations:
    //   forward  — `pat` matches `lhs`, and instantiating `tgt` reproduces `rhs`;
    //   backward — `tgt` matches `lhs`, and instantiating `pat` reproduces `rhs`
    //              (the law is used in reverse, so the term is `@Eq.symm`-wrapped).
    let (lhs_pat, rhs_pat) = if rev {
        (&args[1], &args[0])
    } else {
        (&args[0], &args[1])
    };
    // Match the law's two sides against the goal's two sides, in both
    // orientations; `bind_pair` also pins variables that only one side mentions.
    let fwd = bind_pair(lhs_pat, lhs, rhs_pat, rhs);
    let bwd = if fwd.is_none() {
        bind_pair(rhs_pat, lhs, lhs_pat, rhs)
    } else {
        None
    };
    let (bindings, reversed) = match (fwd, bwd) {
        (Some(b), _) => (b, rev),
        (None, Some(b)) => (b, !rev),
        (None, None) => return None,
    };
    // A definitional step IS the unfolding, so Lean closes it by kernel
    // conversion: `Eq.refl lhs` has type `lhs = lhs`, accepted where the goal is
    // `lhs = rhs` because the cited `def` makes both sides definitionally equal.
    // If they are not, Lean rejects the file (fail-closed).
    if rule.definitional {
        // A plain `=`-macro (`Eq (op params) body`, e.g. D.BT.02
        // `lnot a = Δ 𝟙 a`) is NOT a reducible def in this frame — it is an axiom —
        // so `Eq.refl (lnot X)` cannot close `Eq (lnot X) (Δ 𝟙 X)`. The module
        // declares `axiom {op}_def : ∀ params, Eq (op params) body`; cite it by
        // name with the goal's instantiated arguments instead.
        let (_b, stmt_body) = crate::rules::strip_binders(&rule.statement);
        let plain_op = match &stmt_body {
            Expr::Apply(h, a) if h.primary == qubitick_model::EQ_SYM && a.len() == 2 => {
                match &a[0] {
                    Expr::Apply(op, _) => Some(op.primary),
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some(op_id) = plain_op {
            let op_name = tc.resolve(op_id);
            if let Expr::Apply(_, largs) = qubitick_pattern::expr_strip_parens(lhs) {
                let args = largs
                    .iter()
                    .map(|a| format!("({})", lean_render(a, &|id| tc.resolve(id))))
                    .collect::<Vec<_>>()
                    .join(" ");
                return Some(format!("{op_name}_def {args}"));
            }
        }
        return Some(format!(
            "Eq.refl ({})",
            qubitick_lean::render(lhs, &|id| tc.resolve(id))
        ));
    }
    // A field is emitted (see emit's `reduce_field_statement`) binding ONLY the
    // variables that occur in its equation body; proofs must cite it with the
    // SAME arity. `instantiate_axiom` therefore renders the body's free idents
    // (first-encountered order, as `expr_idents`), not the axiom's full `∀ a b c`
    // binder list (whose trailing unused binders would over-apply the field).
    if bindings.is_empty() && !body_idents(&body).is_empty() {
        return None;
    }
    let mut rendered: Vec<String> = Vec::new();
    for v in body_idents(&body) {
        let bound = bindings.iter().find(|(n, _)| *n == v).map(|(_, e)| e)?;
        // Parenthesise each bound term: an argument that IS a compound (e.g.
        // `cap (mathcal_R t) (lnot (mathfrak_L_fn t))`) must stay one argument,
        // not spill its glyphs into the field application. Without the parens a
        // law `∀ a b c, cap a (Delta b c) = ...` instantiated to `R.ax_6 cap
        // (mathcal_R t) (lnot ...)` parses `cap` as the first Prop argument.
        rendered.push(format!(
            "({})",
            qubitick_lean::render(bound, &|id| tc.resolve(id))
        ));
    }
    let field = rule.head.clone();
    let core = if rendered.is_empty() {
        field
    } else {
        format!("{field} {}", rendered.join(" "))
    };
    Some(if reversed {
        format!("@Eq.symm _ _ _ ({core})")
    } else {
        core
    })
}
#[allow(clippy::items_after_test_module)]
#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_db::queries::SymbolRow;
    use qubitick_model::{AtomicComposite, ChainJust, CiteName, Expr, JustRef, StepRel};
    fn ctx() -> Ctx {
        Ctx::from_rows(
            "Era0/Strata0",
            vec![SymbolRow { facets: 0,
                id: 1,
                scope: "".into(),
                tex: "\\Delta".into(),
                display: "Δ".into(),
                lean_name: "Delta".into(),
                trigger: None,
                kind: "notation".into(),
                role: None,
                precedence: None,
                assoc: None,
                home_node: 1,
            elab: None,
            binder_kind: None,
            }],
        )
    }

    fn sym(id: u32) -> Expr {
        Expr::Sym(AtomicComposite::new(id))
    }

    #[test]
    fn compiles_trans_chain() {
        let ctx = ctx();
        let tc = TermCtx::new(&ctx);
        let env = Env::new();
        let row = ChainRow {
            line: 1,
            lhs: Some(sym(1)),
            rhs: sym(1),
            rel: StepRel::Eq,
            just: ChainJust::Rfl,
        };
        let out = compile_chain(&ctx, &tc, &env, &[row], &crate::rules::RuleTable::new()).unwrap();
        assert!(out.contains("@Eq.trans"));
        assert!(out.contains("Delta"));
    }

    /// A CHAIN WITH NO ROWS IS NOT A PROOF.
    ///
    /// This test used to assert the opposite — `assert_eq!(out, "Eq.refl")` — and that
    /// assertion WAS the trap: the emitter wrote that text as `fun a => Eq.refl _`, so every
    /// unfinished theorem whose statement happened to be a definitional identity (`a Δ ∅ = a`
    /// unfolds to `a`) type-checked and the corpus reported it `verified`, with nothing proved.
    /// An admitted theorem is now an axiom of its own module (`crates/emit/src/theorem.rs`),
    /// and a chain with no rows is a caller bug.
    #[test]
    fn an_empty_chain_is_not_a_refl_proof() {
        let ctx = ctx();
        let tc = TermCtx::new(&ctx);
        let env = Env::new();
        let err = compile_chain(&ctx, &tc, &env, &[], &crate::rules::RuleTable::new())
            .expect_err("no rows must not compile");
        assert!(
            err.to_string().contains("no rows"),
            "the refusal names what is missing: {err}"
        );
    }

    #[test]
    fn rfl_on_equal_sides_emits_eq_refl() {
        let ctx = ctx();
        let tc = TermCtx::new(&ctx);
        let env = Env::new();
        let row = ChainRow {
            line: 1,
            lhs: Some(sym(1)),
            rhs: sym(1),
            rel: StepRel::Eq,
            just: ChainJust::Rfl,
        };
        let out = compile_chain(&ctx, &tc, &env, &[row], &crate::rules::RuleTable::new()).unwrap();
        // The composition wraps the reflexive sub-proof (`@Eq.trans ... (Eq.refl Delta) ...`),
        // and `Eq.refl` is emitted with its argument parenthesised. Assert the
        // property — a reflexive proof on `Delta` and nothing else — not the framing.
        assert!(out.contains("Eq.refl (Delta)"), "got {out}");
        assert!(!out.contains('['), "no citation may leak into the term: {out}");
    }

    #[test]
    fn rewrite_on_unequal_sides_fails_closed_without_leak() {
        // A rewrite that cannot be justified must fail the build with a real
        // diagnostic — never emit a placeholder that leaks `[A1.1]` into Lean.
        let ctx = ctx();
        let tc = TermCtx::new(&ctx);
        let env = Env::new();
        let row = ChainRow {
            line: 1,
            lhs: Some(sym(1)),
            rhs: sym(2),
            rel: StepRel::Eq,
            just: ChainJust::Rewrite {
                refs: vec![JustRef {
                    rev: false,
                    name: CiteName::AxiomLine { node: 1, line: 1 },
                    args: vec![],
                }],
            },
        };
        let err = compile_chain(&ctx, &tc, &env, &[row], &crate::rules::RuleTable::new())
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("A1.1"),
            "diagnostic should name the citation, got {err}"
        );
        assert!(
            !err.contains("Eq.trans _ _ _ _"),
            "must not leak the old placeholder, got {err}"
        );
    }

    #[test]
    fn justifies_reverse_law_at_a_subterm() {
        // `a Δ a = ∅` (A1.4) used BACKWARDS to replace `∅` by `(a Δ b) Δ (a Δ b)`:
        // the variable `a` is only determined by matching the law's LHS against the
        // step's RIGHT-hand side, and the step sits under a shared `Δ` head.
        use qubitick_model::{Binder, BinderKind};
        let ctx = ctx();
        let tc = TermCtx::new(&ctx);
        let env = Env::new();
        let delta = |l: Expr, r: Expr| Expr::Apply(AtomicComposite::new(1), vec![l, r]);
        let law = Expr::Binder(
            BinderKind::Forall,
            vec![Binder {
                vars: vec!["a".into()],
                ty: sym(3),
            }],
            Box::new(Expr::Apply(
                AtomicComposite::new(qubitick_model::EQ_SYM),
                vec![delta(Expr::var("a"), Expr::var("a")), sym(4)],
            )),
        );
        let mut table = crate::rules::RuleTable::new();
        table.insert(1, 4, 3, law);
        let ab = delta(Expr::var("a"), Expr::var("b"));
        let ml = sym(5);
        let row = ChainRow {
            line: 1,
            lhs: Some(delta(ml.clone(), sym(4))),
            rhs: delta(ml, delta(ab.clone(), ab)),
            rel: StepRel::Eq,
            just: ChainJust::Rewrite {
                refs: vec![JustRef {
                    rev: false,
                    name: CiteName::AxiomLine { node: 1, line: 4 },
                    args: vec![],
                }],
            },
        };
        let out = compile_chain(&ctx, &tc, &env, &[row], &table).unwrap();
        assert!(
            out.contains("@Eq.symm") && out.contains("R.ax_3"),
            "expected a reversed axiom instantiation, got {out}"
        );
        assert!(
            out.contains("@congrArg"),
            "the rewrite is at a subterm, so it must be closed by congruence: {out}"
        );
    }

    #[test]
    fn justifies_definitional_cite_by_eq_refl() {
        // A citation `[D.BT.<n>]` resolves (via DefId) to a DEFINITION rule, which
        // is closed by `Eq.refl lhs` (Lean's kernel conversion validates the
        // definitional unfolding of lhs → rhs). Core of the whole D-cite path.
        use qubitick_model::{Binder, BinderKind};
        let ctx = ctx();
        let tc = TermCtx::new(&ctx);
        let env = Env::new();
        // `lnot a = mathbb_1 Δ a` as a definitional rule under division 2
        // (Node.BT.02 => D.BT.02).
        let sema = Expr::Binder(
            BinderKind::Forall,
            vec![Binder {
                vars: vec!["a".into()],
                ty: sym(3),
            }],
            Box::new(Expr::Apply(
                AtomicComposite::new(qubitick_model::EQ_SYM),
                vec![Expr::Sym(AtomicComposite::new(6)), sym(7)],
            )),
        );
        let mut table = crate::rules::RuleTable::new();
        table.insert_definition(2, sema);
        let row = ChainRow {
            line: 1,
            lhs: Some(Expr::Sym(AtomicComposite::new(6))),
            rhs: sym(7),
            rel: StepRel::Eq,
            just: ChainJust::Rewrite {
                refs: vec![JustRef {
                    rev: false,
                    name: CiteName::DefId(2),
                    args: vec![],
                }],
            },
        };
        let out = compile_chain(&ctx, &tc, &env, &[row], &table).unwrap();
        // The justifier closes with `Eq.refl lhs` (the definitional unfolding); the
        // chain wrapper adds `@Eq.trans`, but the JUSTIFIER term must be `Eq.refl`.
        // `Eq.refl` is emitted with its argument parenthesised, so assert the
        // property (a definitional step is closed by reflexivity, not by a citation)
        // rather than the exact spelling.
        assert!(
            out.contains("Eq.refl (s6)"),
            "a definitional cite must close by Eq.refl, got {out}"
        );
    }
}

/// Fetch a hypothesis proposition from the environment by name.
///
/// Two possible `Env` APIs — pick the one that matches your `crates/rewrite/src/env.rs`:
///   * public field `hyps: Vec<(String, Expr)>` → the line below
///   * method `lookup(&self, &str) -> Option<&Expr>` → `env.lookup(name).cloned()`
fn lookup_hyp(env: &Env, name: &str) -> Option<Expr> {
    env.lookup(name)
}

/// Close `lhs = rhs` with a local hypothesis `hyp_name : hyp_ty`, at the root
/// or under a congruence.
///
/// Covers the `[h]`, `[hN]`, and induction-hypothesis `[^i]` cases (the latter
/// is stored in `env` as `ih` by `compile.rs`'s `ProofStep::Induction` arm).
/// Monomorphic hypotheses only; a hypothesis carrying its own `∀` binders would
/// need argument inference, so we fail closed rather than emit a wrong term.
fn justify_hypothesis(
    hyp_name: &str,
    hyp_ty: &Expr,
    lhs: &Expr,
    rhs: &Expr,
    rev: bool,
    tc: &TermCtx,
) -> Option<String> {
    let term_name = if hyp_name == "^i" { "ih" } else { hyp_name };
    let (binders, body) = crate::rules::strip_binders(hyp_ty);
    let Expr::Apply(head, args) = &body else {
        return None;
    };
    if head.primary != qubitick_model::EQ_SYM || args.len() != 2 {
        return None;
    }

    let (pat, tgt) = if rev {
        (&args[1], &args[0])
    } else {
        (&args[0], &args[1])
    };

    // Check root match
    if let Some(b) = qubitick_pattern::bind_pat(pat, lhs) {
        let new = qubitick_pattern::expr_subst(tgt, &b);
        if qubitick_pattern::expr_norm_eq(&new, rhs) {
            return Some(if rev {
                format!("@Eq.symm _ _ _ {term_name}")
            } else {
                term_name.to_string()
            });
        }
    }
    if let Some(b) = qubitick_pattern::bind_pat(tgt, lhs) {
        let new = qubitick_pattern::expr_subst(pat, &b);
        if qubitick_pattern::expr_norm_eq(&new, rhs) {
            return Some(if rev {
                term_name.to_string()
            } else {
                format!("@Eq.symm _ _ _ {term_name}")
            });
        }
    }

    // Subterm descent using rewrite_all (with expr_strip_parens!)
    let law = Law {
        rule: &crate::rules::AxRule {
            head: term_name.to_string(),
            statement: hyp_ty.clone(),
            definitional: false,
        },
        pat: pat.clone(),
        tgt: tgt.clone(),
        vars: binders,
        rev,
    };

    let stripped_lhs = qubitick_pattern::expr_strip_parens(lhs);
    let stripped_rhs = qubitick_pattern::expr_strip_parens(rhs);

    for (next, step) in rewrite_all(&law, stripped_lhs, tc) {
        if qubitick_pattern::expr_norm_eq(&next, stripped_rhs) {
            return Some(step);
        }
    }
    None
}
