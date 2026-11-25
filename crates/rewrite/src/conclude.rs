//! `Conclude` step compilation.

use anyhow::Result;
use qubitick_ctx::Ctx;
use qubitick_lean::render as lean_render;
use qubitick_model::Expr;

use crate::termctx::TermCtx;

/// Compile a `ProofStep::Conclude` — the exact proof term for the goal.
pub fn compile_conclude(_ctx: &Ctx, tc: &TermCtx, term: &Expr) -> Result<String> {
    Ok(format!("exact {}", lean_render(term, &|id| tc.resolve(id))))
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
                assoc: None,
                home_node: 1,
            elab: None,
            binder_kind: None,
            }],
        )
    }

    #[test]
    fn emits_exact() {
        let ctx = ctx();
        let tc = TermCtx::new(&ctx);
        let out = compile_conclude(&ctx, &tc, &Expr::Sym(AtomicComposite::new(1))).unwrap();
        assert!(out.starts_with("exact"));
        assert!(out.contains("Delta"));
    }
}