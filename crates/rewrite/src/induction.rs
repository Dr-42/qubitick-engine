//! `Induction` step compilation — `<Inductive>.rec` (Document 5 §11).

use anyhow::{Result, bail};
use qubitick_ctx::Ctx;
use qubitick_lean::render as lean_render;
use qubitick_model::{Expr, IndCase};

use crate::env::Env;
use crate::termctx::TermCtx;

/// Compile a `ProofStep::Induction` into an `<Inductive>.rec` kernel term
/// (Document 5 §11.1). The motive is implicit (`_`), inferred by Lean from the
/// surrounding goal context.
#[allow(clippy::too_many_arguments)]
/// A CONSTRUCTOR'S DECLARED FIELDS, and the carrier it constructs: ctor symbol id →
/// (the sorts of its fields, in order, the carrier).
///
/// The emitter has the formation declarations; this crate has the symbol table. Which field
/// is RECURSIVE — the one the recursor hands an induction hypothesis for — is a fact of the
/// declaration, so it has to cross the boundary rather than be guessed from the proof text.
pub type CtorFields = std::collections::HashMap<u32, (Vec<u32>, u32)>;

pub fn compile_induction(
    ctx: &Ctx,
    tc: &TermCtx,
    env: &Env,
    scrutinee: &Expr,
    cases: &[IndCase],
    rules: &crate::rules::RuleTable,
    fn_names: &std::collections::HashMap<u32, String>,
    outer_goal: &Expr,
    ctors: &CtorFields,
) -> Result<String> {
    let scrut = lean_render(scrutinee, &|id| tc.resolve(id));
    // Extract the variable name being inducted on
    let var_name = match scrutinee {
        Expr::Var(v) => v.clone(),
        _ => bail!("induction variable is not a Var"),
    };

    // Explicit motive: `fun x => goal[var := x]`. The body of the goal (after its
    // leading ∀-binders, which the goal/env already unrolled) mentions the induction
    // variable; replacing it with a fresh binder yields the motive, so Lean does not
    // have to solve it from the `_` placeholder (which it cannot, here).
    let mut goal_body = outer_goal.clone();
    while let Expr::Binder(_, _, inner) = &goal_body {
        goal_body = (**inner).clone();
    }
    let mvar = "x_i".to_string();
    let motive_expr = qubitick_pattern::expr_subst(
        &goal_body,
        &[(var_name.clone(), Expr::Var(mvar.clone()))],
    );
    let motive = lean_render(&motive_expr, &|id| tc.resolve(id));
    // The bare `prelude` cannot elaborate the `{scrut}.rec (motive := ...)` notation
    // (it hinges on `Init` recursion elaborators that the minimal prelude lacks) —
    // that surface gave `CoeFun`. The EXPLICIT kernel form `@<ind>.rec motive ...`
    // elaborates fine. WHICH inductive this is comes from the corpus: the cases'
    // constructors name a declared formation, and the recursor is that formation's.
    let family = cases
        .iter()
        .filter_map(|c| c.pattern.as_ref())
        .find_map(|p| ctx.carrier_of_ctor(p.constructor.primary as i64))
        .map(|s| s.lean_name.clone())
        .unwrap_or_else(|| tc.resolve(0));
    let mut out = format!("@{family}.rec\n  (fun {mvar} => {motive})\n");

    for c in cases {
        let binders = c
            .pattern
            .as_ref()
            .map(|p| p.binders.clone())
            .unwrap_or_default();
        let mut case_env = env.clone();
        for b in &binders {
            case_env.introduce(b);
        }

        // Reconstruct the constructor expression (e.g., `^+ a` or `\tau_0`)
        let ctor_ac = c
            .pattern
            .as_ref()
            .map(|p| p.constructor.clone())
            .unwrap_or_else(|| qubitick_model::AtomicComposite::new(0));
        // Read before `ctor_ac` is moved into the reconstructed expression below: the
        // declaration lookup keys on the constructor's own symbol id.
        let ctor_id = ctor_ac.primary;
        let ctor_ac = ctor_ac;
        let ctor_e = if binders.is_empty() {
            Expr::Sym(ctor_ac)
        } else {
            Expr::Apply(
                ctor_ac,
                binders.iter().map(|b| Expr::Var(b.clone())).collect(),
            )
        };

        // Substitute the induction variable in the goal for this case
        let case_goal = qubitick_pattern::expr_subst(outer_goal, &[(var_name.clone(), ctor_e)]);

        if !binders.is_empty() {
            let mut ih_goal = outer_goal.clone();
            // Strip ∀ binders
            while let Expr::Binder(_, _, inner) = ih_goal {
                ih_goal = *inner;
            }
            // Substitute the induction variable with the bare binder (e.g., `a`)
            let ih_goal = qubitick_pattern::expr_subst(
                &ih_goal,
                &[(var_name.clone(), Expr::Var(binders[0].clone()))],
            );
            case_env.note("^i", ih_goal);
        }

        let body = crate::compile::compile_steps_with_env(
            ctx,
            tc,
            &mut case_env,
            &case_goal,
            &c.body,
            rules,
            fn_names,
            ctors,
        )?;

        if binders.is_empty() {
            out.push_str(&format!("  ({body})\n"));
            continue;
        }
        // A `C.rec` branch takes the constructor's fields, plus an induction
        // hypothesis for each RECURSIVE field. Which fields are recursive is a fact
        // of the corpus's formation declaration, and this crate carries the symbol
        // table rather than the declarations, so the branch takes the hypothesis
        // exactly when the compiled body USES it: a proof over a recursive
        // constructor does, and a non-recursive constructor's branch has none to use.
        // A proof that ignores an available hypothesis therefore needs the
        // declaration-side arity, which is the induction path's known limit.
        /* HOW MANY HYPOTHESES THIS BRANCH TAKES IS THE DECLARATION'S ANSWER.
         *
         * `tau.rec`'s branch for a constructor is `fun <fields> ih1 … ihk =>`, with one
         * hypothesis per RECURSIVE field — a field whose declared sort is the carrier. This
         * used to be inferred from the compiled body mentioning the word `ih`, which is wrong
         * in both directions: a body that ignores an available hypothesis was emitted one
         * binder short (`… but is expected to have type Eq (oplus tau_0 a) a →
         * Eq (oplus tau_0 a.caret) a.caret`), and a body that merely mentions `ih` in a
         * non-recursive constructor would take a binder Lean does not offer. The emitter
         * passes the formation's declarations in; nothing here is guessed. */
        let recursive = ctors
            .get(&ctor_id)
            .map(|(fields, carrier)| fields.iter().filter(|f| *f == carrier).count())
            .unwrap_or(0);
        let mut params: Vec<String> = binders.clone();
        for k in 0..recursive {
            params.push(if k == 0 { "ih".to_string() } else { format!("ih{}", k + 1) });
        }
        out.push_str(&format!("  (fun {} => ({body}))\n", params.join(" ")));
    }
    out.push_str(&format!("  {scrut}\n"));
    Ok(out)
}

/// Fresh binder name helper.
pub fn fresh_binder(var: &str, i: usize) -> String {
    format!("{var}_{i}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_name() {
        assert_eq!(fresh_binder("x", 2), "x_2");
    }
}
