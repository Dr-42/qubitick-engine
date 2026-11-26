//! `compile_steps` — the top-level proof compiler (CodeArch §5.9).

use anyhow::{Result, bail};
use qubitick_ctx::Ctx;
use qubitick_lean::render as lean_render;
use qubitick_model::{Expr, ProofStep};

use crate::chain::compile_chain;
use crate::conclude::compile_conclude;
use crate::discriminate::compile_discriminate;
use crate::env::Env;
use crate::induction::compile_induction;
use crate::rewrite::compile_rewrite;
use crate::termctx::TermCtx;

/// Compile a proof against `goal` into a Lean term.
///
/// Each top-level `ProofStep` is lowered to a Lean tactic/preterm segment. The
/// result is a proof script that closes the stated goal.
pub fn compile_steps(
    ctx: &Ctx,
    goal: &Expr,
    steps: &[ProofStep],
    rules: &crate::rules::RuleTable,
    fn_names: &std::collections::HashMap<u32, String>,
) -> Result<String> {
    let tc = TermCtx::with_fn(ctx, fn_names);
    let mut env = Env::new();
    compile_steps_with_env(ctx, &tc, &mut env, goal, steps, rules, fn_names)
}

pub(crate) fn compile_steps_with_env(
    ctx: &Ctx,
    tc: &TermCtx,
    env: &mut Env,
    goal: &Expr,
    steps: &[ProofStep],
    rules: &crate::rules::RuleTable,
    fn_names: &std::collections::HashMap<u32, String>,
) -> Result<String> {
    let mut segments: Vec<String> = Vec::new();
    let mut open = Vec::new();
    let mut cur_goal = goal.clone();

    // 1. Unpack ∀ binders
    while let Expr::Binder(_, binders, body) = &cur_goal {
        for b in binders {
            for v in &b.vars {
                env.introduce(v);
            }
        }
        cur_goal = *body.clone();
    }

    // 2. Unpack → implications using dynamic name matching
    let mut arrow_ids = Vec::new();
    for tex in qubitick_model::format::IMPLICATION {
        if let Some(rows) = ctx.by_tex(tex) {
            for r in rows {
                arrow_ids.push(r.id as u32);
            }
        }
    }
    arrow_ids.push(u32::MAX - 3);

    let mut k = 0usize;
    while let Expr::Apply(head, args) = &cur_goal {
        let resolved = tc.resolve(head.primary);
        if (head.primary == 0 // Lexer fallback for \to
            || arrow_ids.contains(&head.primary)
            || resolved == "to"
            || resolved == "rightarrow"
            || resolved == "Longrightarrow"
            || resolved == "implies"
            || resolved == "impl") // For \Longrightarrow
            && args.len() == 2
        {
            let name = if k == 0 {
                "h".to_string()
            } else {
                format!("h{}", k)
            };
            env.note(&name, args[0].clone());
            k += 1;
            cur_goal = args[1].clone();
        } else {
            break;
        }
    }

    for step in steps {
        let seg = compile_step(ctx, tc, env, step, &cur_goal, &mut open, rules, fn_names)?;
        segments.push(seg);
    }

    if segments.is_empty() {
        return Ok("Eq.refl _".to_string());
    }

    let chain = segments.join("\n");
    // A non-Eq goal (e.g. a relational iff-unfolded Prop like T_BT_72's
    // `ge (a + -a)`) is concluded from the equality chain running to `\top` by
    // `Eq.mpr chain trivial` (the chain accounts for the whole changed side).
    if !matches!(
        &cur_goal,
        Expr::Apply(h, _) if h.primary == qubitick_model::EQ_SYM
    ) {
        return Ok(format!(
            "Eq.subst (motive := fun _y => _y) (@Eq.symm _ _ _ ({chain})) (@True.intro)"
        ));
    }

    Ok(chain)
}

#[allow(clippy::too_many_arguments)]
fn compile_step(
    ctx: &Ctx,
    tc: &TermCtx,
    env: &mut Env,
    step: &ProofStep,
    goal: &Expr,
    open: &mut Vec<String>,
    rules: &crate::rules::RuleTable,
    fn_names: &std::collections::HashMap<u32, String>,
) -> Result<String> {
    match step {
        ProofStep::Chain { rows } => compile_chain(ctx, tc, env, rows, rules),
        ProofStep::Conclude { term } => compile_conclude(ctx, tc, term),
        ProofStep::Fix { vars } => {
            let vs = vars.join(" ");
            env.introduce_many(vars.iter().map(|s| s.as_str()));
            Ok(format!("fix {vs}"))
        }
        ProofStep::Rewrite { refs } => compile_rewrite(ctx, tc, env, refs),
        ProofStep::Induction { var, cases } => {
            let scrut = goal_var_expr(var);
            compile_induction(ctx, tc, env, &scrut, cases, rules, fn_names, goal)
        }
        ProofStep::Discriminate { hyp } => compile_discriminate(ctx, tc, hyp),
        ProofStep::Note { name, prop, body } => {
            let prop_lean = lean_render(prop, &|id| tc.resolve(id));
            let mut inner = String::new();
            let mut inner_open = Vec::new();
            for s in body {
                let seg = compile_step(ctx, tc, env, s, prop, &mut inner_open, rules, fn_names)?;
                inner.push_str(&seg);
                inner.push('\n');
            }
            env.note(name, prop.clone());
            let _ = open;
            Ok(format!(
                "have {name} : {prop_lean} := by\n{}",
                indent(&inner)
            ))
        }
        ProofStep::Case { scrutinee, arms } => {
            // Case analysis on a scrutinee.
            let mut out = format!(
                "cases {} with",
                lean_render(scrutinee, &|id| tc.resolve(id))
            );
            for arm in arms {
                let pat = render_pattern(ctx, &arm.pattern);
                let body = compile_steps(ctx, goal, &arm.body, rules, fn_names)?;
                out.push_str(&format!("\n| {pat} => {body}"));
            }
            Ok(out)
        }
        ProofStep::Recursor { scrutinee, cases } => {
            let mut out = format!(
                "match {} with",
                lean_render(scrutinee, &|id| tc.resolve(id))
            );
            for c in cases {
                let pat = c
                    .pattern
                    .as_ref()
                    .map(|p| render_pattern(ctx, p))
                    .unwrap_or_else(|| "_".to_string());
                let body = compile_steps(ctx, goal, &c.body, rules, fn_names)?;
                out.push_str(&format!("\n| {pat} => {body}"));
            }
            Ok(out)
        }
    }
}

fn goal_var_expr(var: &str) -> Expr {
    Expr::var(var)
}

fn render_pattern(
    ctx: &Ctx,
    p: &qubitick_model::Pattern,
) -> String {
    if p.binders.is_empty() {
        // Nullary constructor: match on the constructor's lean name (e.g.
        // `mathbf_p`), not `_`, so a `match` over a custom formation discriminates.
        ctx.by_id(p.constructor.primary as i64)
            .map(|r| r.lean_name.clone())
            .unwrap_or_else(|| "_".to_string())
    } else {
        p.binders.join(" ")
    }
}

fn indent(s: &str) -> String {
    s.lines()
        .map(|l| format!("  {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Re-exported for convenience.
pub fn _bail_unsupported(feature: &str) -> Result<String> {
    bail!("proof step not yet supported: {feature}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_db::queries::SymbolRow;

    fn ctx() -> Ctx {
        let row = SymbolRow { facets: 0,
            id: 1,
            scope: "".into(),
            tex: "\\Delta".into(),
            display: "Δ".into(),
            lean_name: "Delta".into(),
            trigger: None,
            kind: "notation".into(),
            role: None,
            precedence: None,
            home_node: 1,
            elab: None,
            binder_kind: None,
        };
        Ctx::from_rows("Era0/Strata0", vec![row])
    }

    #[test]
    fn compiles_empty_chain() {
        // An empty proof is reflexivity as a TERM: the emitted module is a bare
        // `prelude` module, so a tactic (`by rfl`) would not parse at all.
        let ctx = ctx();
        let goal = Expr::Sym(qubitick_model::AtomicComposite::new(1));
        let out = compile_steps(
            &ctx,
            &goal,
            &[],
            &crate::rules::RuleTable::new(),
            &Default::default(),
        )
        .unwrap();
        assert_eq!(out, "Eq.refl _");
        assert!(!out.contains("by "), "no tactic script may be emitted");
    }

    #[test]
    fn compiles_conclude() {
        let ctx = ctx();
        let goal = Expr::Sym(qubitick_model::AtomicComposite::new(1));
        let step = ProofStep::Conclude { term: goal.clone() };
        let out = compile_steps(
            &ctx,
            &goal,
            &[step],
            &crate::rules::RuleTable::new(),
            &Default::default(),
        )
        .unwrap();
        assert!(out.contains("exact") || out.contains("Delta"));
    }
}
