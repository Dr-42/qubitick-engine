//! Free-variable extraction (L1).

use qubitick_model::Expr;

/// Collect the free variables of `e`, in first-encountered order, deduplicated.
/// Binder and Recursor bound names are excluded from the body they bind.
pub fn expr_idents(e: &Expr, out: &mut Vec<String>) {
    collect(e, &mut Vec::new(), out);
}

fn collect(e: &Expr, bound: &mut Vec<String>, out: &mut Vec<String>) {
    match e {
        Expr::Var(v) => {
            if !bound.contains(v) && !out.contains(v) {
                out.push(v.clone());
            }
        }
        Expr::Sym(_) => {}
        Expr::Apply(_, args) => {
            for a in args {
                collect(a, bound, out);
            }
        }
        Expr::Bracket(_, elems) => {
            for x in elems {
                collect(x, bound, out);
            }
        }
        Expr::Binder(_, binders, body) => {
            for b in binders {
                collect(&b.ty, bound, out);
                for v in &b.vars {
                    if !bound.contains(v) {
                        bound.push(v.clone());
                    }
                }
            }
            collect(body, bound, out);
            for b in binders {
                for v in &b.vars {
                    if let Some(idx) = bound.iter().rposition(|x| x == v) {
                        bound.remove(idx);
                    }
                }
            }
        }
        Expr::Recursor(s, cases) => {
            collect(s, bound, out);
            for (p, body) in cases {
                for v in &p.binders {
                    if !bound.contains(v) {
                        bound.push(v.clone());
                    }
                }
                collect(body, bound, out);
                for v in &p.binders {
                    if let Some(idx) = bound.iter().rposition(|x| x == v) {
                        bound.remove(idx);
                    }
                }
            }
        }
        Expr::Matrix(rows) => {
            for r in rows {
                for c in r {
                    collect(c, bound, out);
                }
            }
        }
        Expr::Vector(elts) | Expr::Set(elts) => {
            for e in elts {
                collect(e, bound, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_model::{AtomicComposite, Binder, BinderKind};

    fn sym(id: u32) -> Expr {
        Expr::Sym(AtomicComposite::new(id))
    }

    #[test]
    fn picks_free_only() {
        let e = Expr::Binder(
            BinderKind::Forall,
            vec![Binder {
                vars: vec!["x".into()],
                ty: sym(8),
            }],
            Box::new(Expr::Apply(
                AtomicComposite::new(1),
                vec![Expr::Var("x".into()), Expr::Var("y".into())],
            )),
        );
        let mut out = Vec::new();
        expr_idents(&e, &mut out);
        assert_eq!(out, vec!["y"]);
    }
}