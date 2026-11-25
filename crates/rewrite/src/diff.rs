//! Real equation-justification term generator.
//!
//! Port of the core of the pre-refactor monolith's `try_diff_apply`: given a
//! chain row `lhs = rhs` and an optional rewrite rule (the cited justifier),
//! produce an actual Lean term `: Eq lhs rhs` — `Eq.refl` for a syntactic
//! equality, a cited-head application for a matched rewrite, `@Eq.symm` when
//! the citation is used in reverse, or a `@congrArg` congruence under a shared
//! head. No placeholders, no leaks.

use anyhow::{Result, bail};

use qubitick_model::Expr;
use qubitick_pattern::{
    bind_pat, expr_idents, expr_norm_eq, expr_subst, replace_at,
};

use crate::engine::try_rewrite;

const HOLE: &str = "$__hole";

/// A single rewrite rule: an LHS pattern matching a goal side, an RHS template,
/// and the Lean name of the justifier to apply (axiom field `R.ax_k`, theorem,
/// or definition). `lean_name: None` means the rewrite is structural (definitional),
/// justified by `Eq.refl`. `rev` marks a citation used in reversed direction.
#[derive(Debug, Clone)]
pub struct RewriteRule {
    pub from: Expr,
    pub to: Expr,
    pub lean_name: Option<String>,
    pub rev: bool,
}

impl RewriteRule {
    pub fn new(from: Expr, to: Expr, lean_name: impl Into<Option<String>>) -> Self {
        RewriteRule {
            from,
            to,
            lean_name: lean_name.into(),
            rev: false,
        }
    }
}

/// Produce a Lean term proving `Eq lhs rhs`.
///
/// `lean_name` maps a `SymbolId` to its Lean declaration name (the `TermCtx`).
#[allow(clippy::collapsible_if)]
pub fn prove_eq_term(
    lhs: &Expr,
    rhs: &Expr,
    rule: Option<&RewriteRule>,
    lean_name: &impl Fn(u32) -> String,
) -> Result<String> {
    let render = |e: &Expr| qubitick_lean::render(e, lean_name);

    // Reflexive step: `Eq.refl lhs`.
    if expr_norm_eq(lhs, rhs) {
        return Ok(format!("Eq.refl {}", render(lhs)));
    }

    // Ctor leaves against which a head is NOT flattened — not needed here.

    // Try the cited rule: bind its LHS/TEMPLATE pattern against `lhs`, then
    // check that instantiating the other side reproduces `rhs`.
    if let Some(rule) = rule {
        let (pat_from, pat_to, wrap_symm) = if rule.rev {
            (&rule.to, &rule.from, true)
        } else {
            (&rule.from, &rule.to, false)
        };
        if let Some(binds) = bind_pat(pat_from, lhs) {
            let to_inst = expr_subst(pat_to, &binds);
            if expr_norm_eq(&to_inst, rhs) {
                let core = match &rule.lean_name {
                    Some(head) => apply_named(head, &rule.from, &binds, &render)?,
                    None => format!("Eq.refl {}", render(rhs)),
                };
                if wrap_symm {
                    return Ok(format!("@Eq.symm _ _ _ ({core})"));
                }
                return Ok(core);
            }
        }
    }

    // `@congrArg` congruence: `f a = f b` follows from `a = b` under a shared
    // head, with the unchanged argument positions captured in a hole lambda.
    // Subterm descent, closing by congruence at the first position that succeeds.
    if let (Expr::Apply(h1, a1), Expr::Apply(h2, a2)) = (
        qubitick_pattern::expr_strip_parens(lhs),
        qubitick_pattern::expr_strip_parens(rhs),
    ) {
        if h1.primary == h2.primary && a1.len() == a2.len() {
            for i in 0..a1.len() {
                if !expr_norm_eq(&a1[i], &a2[i]) {
                    let t = prove_eq_term(&a1[i], &a2[i], rule, lean_name)?;
                    let mut args = a1.clone();
                    args[i] = Expr::Var(HOLE.to_string());
                    let ctx_expr = Expr::Apply(h1.clone(), args);
                    return Ok(format!(
                        "@congrArg _ _ _ _ (fun {HOLE} => {}) ({t})",
                        render(&ctx_expr)
                    ));
                }
            }
        }
    }

    // Rewrite INSIDE `lhs`: replace a proper subterm with the cited law, then wrap
    // the subterm proof in a `@congrArg` that lifts it to the whole term.
    //
    // This is the case the earlier head-only recursion could not reach, and it is
    // the whole `cannot-justify` family: a chain row from `∩ X (∩ Y Z)` to
    // `∩ (∩ X Y) Z` differs only by the associativity axiom applied to a subterm,
    // with no positional congruence available anywhere on the way down.
    if let Some(rule) = rule
        && let Some(t) = prove_by_replacing(lhs, rhs, rule, lean_name, 0)?
    {
        return Ok(t);
    }

    bail!(
        "cannot prove {} = {} with the given rule",
        render(lhs),
        render(rhs)
    )
}

/// The maximum number of cited-law applications a single chain row may need.
///
/// A corpus row is one step, but a step may need the law applied under a nesting
/// of operators. Bounded, because the search is exponential in width.
const MAX_REWRITE_DEPTH: usize = 3;

/// Try to prove `Eq lhs rhs` by rewriting a subterm of `lhs` once.
///
/// Recursion is **on the subterm only**, and terminates because each recursive call
/// is made against a strictly smaller term (a proper subterm of `lhs`). The
/// whole-term level is handled by the caller's `@Eq.trans`/`congrArg` wrapping, so
/// this never re-enters itself on the same size.
fn prove_by_replacing(
    lhs: &Expr,
    rhs: &Expr,
    rule: &RewriteRule,
    lean_name: &impl Fn(u32) -> String,
    depth: usize,
) -> Result<Option<String>> {
    if depth >= MAX_REWRITE_DEPTH {
        return Ok(None);
    }
    for (next, step) in try_rewrite(lhs, rule) {
        // Case 1: the single replacement lands exactly on `rhs` — the row is one
        // congruence step.
        if expr_norm_eq(&next, rhs) {
            let inner = replacement_term(&step, rule, lean_name)?;
            return Ok(Some(lift_with_congr(lhs, &step, &inner, lean_name)?));
        }
        // Case 2: rewrite again from `next`. Compose with `@Eq.trans` so the caller
        // receives a single term `Eq lhs rhs`.
        if let Some(inner) = prove_by_replacing(&next, rhs, rule, lean_name, depth + 1)? {
            let one = replacement_term(&step, rule, lean_name)?;
            let lifted = lift_with_congr(lhs, &step, &one, lean_name)?;
            let render = |e: &Expr| qubitick_lean::render(e, lean_name);
            return Ok(Some(format!(
                "@Eq.trans _ ({}) ({}) ({}) ({lifted}) ({inner})",
                render(lhs),
                render(&next),
                render(rhs)
            )));
        }
    }
    Ok(None)
}

/// The Lean term for the replacement itself: the cited law applied to the values
/// its metavariables bound to, `@Eq.symm`-wrapped when cited in reverse.
fn replacement_term(
    step: &crate::engine::RewriteStep,
    rule: &RewriteRule,
    lean_name: &impl Fn(u32) -> String,
) -> Result<String> {
    let render = |e: &Expr| qubitick_lean::render(e, lean_name);
    let core = match &rule.lean_name {
        Some(head) => {
            let matched = if step.reversed { &rule.to } else { &rule.from };
            apply_named(head, matched, &step.binds, &render)?
        }
        // A definitional step is kernel conversion.
        None => format!("Eq.refl {}", render(&step.after)),
    };
    if step.reversed {
        Ok(format!("@Eq.symm _ _ _ ({core})"))
    } else {
        Ok(core)
    }
}

/// Lift `Eq before after` at `step.pos` to `Eq whole (whole with pos replaced)`.
///
/// `@congrArg`-based: the whole term is rebuilt with the rewritten position
/// abstracted to a binder, which gives the generator `fun h => …`.
fn lift_with_congr(
    whole: &Expr,
    step: &crate::engine::RewriteStep,
    inner: &str,
    lean_name: &impl Fn(u32) -> String,
) -> Result<String> {
    if step.pos.is_empty() {
        // The rewrite WAS the whole term; nothing to lift.
        return Ok(inner.to_string());
    }
    let render = |e: &Expr| qubitick_lean::render(e, lean_name);
    let ctx_expr = replace_at(whole, &step.pos, Expr::Var(HOLE.to_string()));
    Ok(format!(
        "@congrArg _ _ _ _ (fun {HOLE} => {}) ({inner})",
        render(&ctx_expr)
    ))
}

/// Apply a named justifier `head` to the values bound for `from`'s metavariables,
/// in `expr_idents` order: `head a b c` (or bare `head` when no metavars).
fn apply_named(
    head: &str,
    from: &Expr,
    binds: &[(String, Expr)],
    render: &impl Fn(&Expr) -> String,
) -> Result<String> {
    let mut metas: Vec<String> = Vec::new();
    expr_idents(from, &mut metas);
    let mut args: Vec<String> = Vec::new();
    for m in &metas {
        let e = binds
            .iter()
            .find(|(n, _)| n == m)
            .map(|(_, e)| e)
            .ok_or_else(|| anyhow::anyhow!("unbound metavariable {m}"))?;
        args.push(render(e));
    }
    if args.is_empty() {
        Ok(head.to_string())
    } else {
        Ok(format!("{head} {}", args.join(" ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_model::AtomicComposite;

    fn sym(id: u32) -> Expr {
        Expr::Sym(AtomicComposite::new(id))
    }

    fn name(id: u32) -> String {
        match id {
            1 => "Delta".into(),
            2 => "Cap".into(),
            _ => format!("s{id}"),
        }
    }

    #[test]
    fn reflexive_is_eq_refl() {
        let t = prove_eq_term(&sym(1), &sym(1), None, &name).unwrap();
        assert_eq!(t, "Eq.refl Delta");
    }

    #[test]
    fn named_rule_applies_with_bound_args() {
        // comm: `a \Delta b` -> `b \Delta a`. lhs=2 Δ 3, rhs=3 Δ 2.
        let from = Expr::Apply(
            AtomicComposite::new(1),
            vec![Expr::Var("a".into()), Expr::Var("b".into())],
        );
        let to = Expr::Apply(
            AtomicComposite::new(1),
            vec![Expr::Var("b".into()), Expr::Var("a".into())],
        );
        let rule = RewriteRule::new(from, to, Some("comm".to_string()));
        let lhs = Expr::Apply(AtomicComposite::new(1), vec![sym(2), sym(3)]);
        let rhs = Expr::Apply(AtomicComposite::new(1), vec![sym(3), sym(2)]);
        let t = prove_eq_term(&lhs, &rhs, Some(&rule), &name).unwrap();
        assert!(
            t.contains("comm"),
            "should apply the named justifier, got {t}"
        );
    }

    #[test]
    fn reversed_rule_wraps_symm() {
        let from = Expr::Apply(
            AtomicComposite::new(1),
            vec![Expr::Var("a".into()), Expr::Var("b".into())],
        );
        let to = Expr::Apply(
            AtomicComposite::new(1),
            vec![Expr::Var("b".into()), Expr::Var("a".into())],
        );
        let mut rule = RewriteRule::new(from, to, Some("comm".to_string()));
        rule.rev = true;
        // lhs = a Δ b (a,b swapped lhs), instantiate via rule with x=b,y=a.
        // We need lhs to match pat_from = to = b Δ a with x->?, y->?.
        let lhs = Expr::Apply(AtomicComposite::new(1), vec![sym(3), sym(4)]);
        let rhs = Expr::Apply(AtomicComposite::new(1), vec![sym(4), sym(3)]);
        // lhs = x Δ y with x=3,y=4 ; to_inst = y Δ x = 4 Δ 3 = rhs. rev wraps symm.
        let t = prove_eq_term(&lhs, &rhs, Some(&rule), &name).unwrap();
        assert!(t.starts_with("@Eq.symm _ _ _ ("), "got {t}");
    }

    #[test]
    fn congr_arg_under_shared_head() {
        // a Δ (u Δ v) = a Δ (v Δ u): both share head Δ; only the inner Δ swaps,
        // justified by `comm`, so the outer equality is a @congrArg.
        let from = Expr::Apply(
            AtomicComposite::new(1),
            vec![Expr::Var("u".into()), Expr::Var("v".into())],
        );
        let to = Expr::Apply(
            AtomicComposite::new(1),
            vec![Expr::Var("v".into()), Expr::Var("u".into())],
        );
        let rule = RewriteRule::new(from, to, Some("comm".to_string()));
        let lhs = Expr::Apply(
            AtomicComposite::new(1),
            vec![
                sym(2),
                Expr::Apply(AtomicComposite::new(1), vec![sym(3), sym(4)]),
            ],
        );
        let rhs = Expr::Apply(
            AtomicComposite::new(1),
            vec![
                sym(2),
                Expr::Apply(AtomicComposite::new(1), vec![sym(4), sym(3)]),
            ],
        );
        let t = prove_eq_term(&lhs, &rhs, Some(&rule), &name).unwrap();
        assert!(t.starts_with("@congrArg"), "got {t}");
        assert!(t.contains("comm"), "got {t}");
    }

    #[test]
    fn unprovable_step_errors() {
        let lhs = Expr::Apply(AtomicComposite::new(1), vec![sym(2), sym(3)]);
        let rhs = Expr::Apply(AtomicComposite::new(2), vec![sym(2), sym(3)]);
        let err = prove_eq_term(&lhs, &rhs, None, &name)
            .unwrap_err()
            .to_string();
        assert!(err.contains("cannot prove"), "got {err}");
    }
}

