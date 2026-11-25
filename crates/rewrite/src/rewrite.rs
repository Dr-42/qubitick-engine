//! `Rewrite` step compilation and directed rewrite application (CodeArch §5.9).

use anyhow::Result;
use qubitick_ctx::Ctx;
use qubitick_model::{Expr, JustRef};

use crate::env::Env;
use crate::termctx::TermCtx;

/// Compile a `ProofStep::Rewrite` — apply rewrite lemmas to the goal.
pub fn compile_rewrite(
    _ctx: &Ctx,
    _tc: &TermCtx,
    _env: &Env,
    refs: &[JustRef],
) -> Result<String> {
    if refs.is_empty() {
        return Ok("rfl".to_string());
    }
    let rws = refs
        .iter()
        .map(|r| {
            if r.rev {
                format!("← {}", cite_name(&r.name))
            } else {
                cite_name(&r.name)
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!("rw [{rws}]"))
}

/// A directed rewrite attempt: apply a lemma `lhs → rhs` to `goal`.
///
/// This is the seam for the full `try_diff_apply`; the skeleton matches `goal`
/// against `lhs` via `qubitick_pattern::bind_pat` and substitutes `rhs`.
pub fn try_diff_apply(_ctx: &Ctx, goal: &Expr, lhs: &Expr, rhs: &Expr) -> Option<Expr> {
    let binds = qubitick_pattern::bind_pat(lhs, goal)?;
    Some(qubitick_pattern::expr_subst(rhs, &binds))
}

/// Human-readable form of a citation name.
pub fn cite_name(name: &qubitick_model::CiteName) -> String {
    use qubitick_model::CiteName;
    match name {
        CiteName::Local(v) => v.clone(),
        CiteName::AxiomLine { node, line } => format!("A{node}.{line}"),
        CiteName::TheoremId(id) => format!("T.{id}"),
        CiteName::DefId(id) => format!("D.{id}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_db::queries::SymbolRow;
    use qubitick_model::AtomicComposite;

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
    fn empty_rewrite_is_rfl() {
        let ctx = ctx();
        let tc = TermCtx::new(&ctx);
        let env = Env::new();
        assert_eq!(compile_rewrite(&ctx, &tc, &env, &[]).unwrap(), "rfl");
    }

    #[test]
    fn applies_lemma_by_binding() {
        let ctx = ctx();
        let goal = sym(1);
        let lhs = Expr::var("x");
        let rhs = sym(1);
        let out = try_diff_apply(&ctx, &goal, &lhs, &rhs);
        assert!(out.is_some());
    }
}