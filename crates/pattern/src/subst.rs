//! Capture-avoiding-by-construction substitution (L1).

use qubitick_model::Expr;

/// Substitute the bound variable named `name` → `replacement` throughout `e`,
/// rebuilding the tree. Symbols are untouched; `Var` nodes matching `name` are
/// replaced. Binder/Recursor patterns shadow their bound names, so substitution
/// does not descend under a binding with the same name.
pub fn expr_subst_var(e: &Expr, name: &str, replacement: &Expr) -> Expr {
    match e {
        Expr::Var(v) if v == name => replacement.clone(),
        Expr::Var(_) => e.clone(),
        Expr::Sym(_) => e.clone(),
        Expr::Apply(h, args) => Expr::Apply(
            h.clone(),
            args.iter().map(|a| expr_subst_var(a, name, replacement)).collect(),
        ),
        Expr::Bracket(b, elems) => Expr::Bracket(
            *b,
            elems.iter().map(|x| expr_subst_var(x, name, replacement)).collect(),
        ),
        Expr::Binder(k, binders, body) => {
            let shadowed = binders.iter().any(|b| b.vars.iter().any(|v| v == name));
            if shadowed {
                Expr::Binder(
                    *k,
                    binders
                        .iter()
                        .map(|b| qubitick_model::Binder {
                            vars: b.vars.clone(),
                            ty: expr_subst_var(&b.ty, name, replacement),
                        })
                        .collect(),
                    body.clone(),
                )
            } else {
                Expr::Binder(
                    *k,
                    binders
                        .iter()
                        .map(|b| qubitick_model::Binder {
                            vars: b.vars.clone(),
                            ty: expr_subst_var(&b.ty, name, replacement),
                        })
                        .collect(),
                    Box::new(expr_subst_var(body, name, replacement)),
                )
            }
        }
        Expr::Recursor(s, cases) => Expr::Recursor(
            Box::new(expr_subst_var(s, name, replacement)),
            cases
                .iter()
                .map(|(p, body)| {
                    let shadowed = p.binders.iter().any(|v| v == name);
                    if shadowed {
                        (p.clone(), body.clone())
                    } else {
                        (p.clone(), expr_subst_var(body, name, replacement))
                    }
                })
                .collect(),
        ),
        Expr::Matrix(rows) => Expr::Matrix(
            rows.iter()
                .map(|r| r.iter().map(|x| expr_subst_var(x, name, replacement)).collect())
                .collect(),
        ),
        Expr::Vector(els) => Expr::Vector(
            els.iter().map(|x| expr_subst_var(x, name, replacement)).collect(),
        ),
        Expr::Set(els) => Expr::Set(
            els.iter().map(|x| expr_subst_var(x, name, replacement)).collect(),
        ),
    }
}

/// Apply a whole substitution map to a single expression.
pub fn expr_subst(e: &Expr, sub: &[(String, Expr)]) -> Expr {
    // SIMULTANEOUS substitution: every bound variable is replaced in ONE pass, so
    // a replacement VALUE that itself names a bound variable (law vars `a,b,c`
    // bound to row vars `b,c,d`) is not re-substituted by a later binding. The
    // old fold re-ran the whole list over the growing term, corrupting such
    // bindings (`plus (plus a b) c` -> `plus (plus d d) d`).
    match e {
        Expr::Var(v) => sub
            .iter()
            .find(|(n, _)| n == v)
            .map(|(_, r)| r.clone())
            .unwrap_or_else(|| e.clone()),
        Expr::Sym(_) => e.clone(),
        Expr::Apply(h, args) => Expr::Apply(
            h.clone(),
            args.iter().map(|a| expr_subst(a, sub)).collect(),
        ),
        Expr::Bracket(b, elems) => Expr::Bracket(
            *b,
            elems.iter().map(|x| expr_subst(x, sub)).collect(),
        ),
        Expr::Binder(k, binders, body) => {
            let bound: Vec<&String> = binders.iter().flat_map(|b| b.vars.iter()).collect();
            let filtered: Vec<(String, Expr)> = sub
                .iter()
                .filter(|(n, _)| !bound.contains(&n))
                .cloned()
                .collect();
            Expr::Binder(
                *k,
                binders
                    .iter()
                    .map(|b| qubitick_model::Binder {
                        vars: b.vars.clone(),
                        ty: expr_subst(&b.ty, &filtered),
                    })
                    .collect(),
                if filtered.is_empty() {
                    body.clone()
                } else {
                    Box::new(expr_subst(body, &filtered))
                },
            )
        }
        Expr::Recursor(s, cases) => Expr::Recursor(
            Box::new(expr_subst(s, sub)),
            cases
                .iter()
                .map(|(p, b)| (p.clone(), expr_subst(b, sub)))
                .collect(),
        ),
        Expr::Matrix(rows) => Expr::Matrix(
            rows.iter()
                .map(|r| r.iter().map(|x| expr_subst(x, sub)).collect())
                .collect(),
        ),
        Expr::Vector(elems) => Expr::Vector(
            elems.iter().map(|x| expr_subst(x, sub)).collect(),
        ),
        Expr::Set(elems) => Expr::Set(
            elems.iter().map(|x| expr_subst(x, sub)).collect(),
        ),
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_model::{AtomicComposite, Binder, BinderKind};

    fn sym(id: u32) -> Expr {
        Expr::Sym(AtomicComposite::new(id))
    }

    /// A metavariable appearing twice must be substituted at BOTH sites.
    ///
    /// Regression for the CapComm row: the A1.4 shape `a Δ a` rewritten in reverse
    /// must yield `s Δ s`, not `s Δ a`.
    #[test]
    fn substitutes_every_occurrence() {
        use qubitick_model::AtomicComposite;
        let app = |id: u32, args: Vec<Expr>| Expr::Apply(AtomicComposite::new(id), args);
        let pat = app(1, vec![Expr::var("a"), Expr::var("a")]);
        let s_term = app(2, vec![Expr::var("x"), Expr::var("y")]);
        // Direct substitution: both occurrences.
        let out = expr_subst_var(&pat, "a", &s_term);
        assert_eq!(out, app(1, vec![s_term.clone(), s_term]));
    }

    #[test]
    fn replaces_var() {
        let e = Expr::Apply(AtomicComposite::new(1), vec![Expr::Var("x".into()), sym(2)]);
        let out = expr_subst_var(&e, "x", &sym(9));
        assert_eq!(
            out,
            Expr::Apply(AtomicComposite::new(1), vec![sym(9), sym(2)])
        );
    }

    #[test]
    fn respects_binder_shadowing() {
        let e = Expr::Binder(
            BinderKind::Forall,
            vec![Binder {
                vars: vec!["x".into()],
                ty: sym(8),
            }],
            Box::new(Expr::Var("x".into())),
        );
        // The bound `x` must NOT be replaced.
        let out = expr_subst_var(&e, "x", &sym(9));
        assert!(matches!(out, Expr::Binder(_, _, body) if matches!(&*body, Expr::Var(v) if v == "x")));
    }
}