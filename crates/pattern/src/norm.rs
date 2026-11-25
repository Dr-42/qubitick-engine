//! Normalization: structural equality and parenthesis stripping (L1).
//!
//! These are the syntax-level operations shared by `rewrite` and `autocite`.
//! `expr_norm_eq` is the canonical structural-equality test: it ignores
//! redundant delimiters and compares subtrees recursively.

use qubitick_model::{Expr, Mods};

/// True if two expressions are structurally equal after normalizing redundant
/// delimiters. Symbols compare by `SymbolId`; bound variables by name.
pub fn expr_norm_eq(a: &Expr, b: &Expr) -> bool {
    let a = strip_delims(a);
    let b = strip_delims(b);
    match (a, b) {
        (Expr::Sym(x), Expr::Sym(y)) => mods_eq(&x.mods, &y.mods) && x.primary == y.primary,
        (Expr::Var(x), Expr::Var(y)) => x == y,
        (Expr::Apply(h1, a1), Expr::Apply(h2, a2)) => {
            atomic_eq(h1, h2) && a1.len() == a2.len() && list_eq(a1, a2)
        }
        (Expr::Bracket(b1, a1), Expr::Bracket(b2, a2)) => {
            b1 == b2 && a1.len() == a2.len() && list_eq(a1, a2)
        }
        (Expr::Binder(k1, b1, body1), Expr::Binder(k2, b2, body2)) => {
            k1 == k2 && binders_eq(b1, b2) && expr_norm_eq(body1, body2)
        }
        (Expr::Recursor(s1, c1), Expr::Recursor(s2, c2)) => {
            expr_norm_eq(s1, s2) && cases_eq(c1, c2)
        }
        (Expr::Matrix(m1), Expr::Matrix(m2)) => {
            m1.len() == m2.len()
                && m1.iter()
                    .zip(m2)
                    .all(|(r1, r2)| r1.len() == r2.len() && list_eq(r1, r2))
        }
        (Expr::Vector(v1), Expr::Vector(v2)) => list_eq(v1, v2),
        (Expr::Set(s1), Expr::Set(s2)) => list_eq(s1, s2),
        _ => false,
    }
}

/// Returns the expression with one redundant layer of delimiters removed, if any.
///
/// In the current model brackets are `Expr::Bracket`, so a single-element
/// `Bracket` is treated as the delimiter it wraps; this is where normalization
/// applies. Any other expression is returned unchanged.
pub fn expr_strip_parens(e: &Expr) -> &Expr {
    strip_delims(e)
}

fn strip_delims(e: &Expr) -> &Expr {
    match e {
        Expr::Bracket(_, elems) if elems.len() == 1 => strip_delims(&elems[0]),
        _ => e,
    }
}

fn atomic_eq(a: &qubitick_model::AtomicComposite, b: &qubitick_model::AtomicComposite) -> bool {
    a.primary == b.primary && mods_eq(&a.mods, &b.mods)
}

fn mods_eq(a: &Mods, b: &Mods) -> bool {
    mod_slot_eq(&a.l_sup, &b.l_sup)
        && mod_slot_eq(&a.r_sup, &b.r_sup)
        && mod_slot_eq(&a.l_sub, &b.l_sub)
        && mod_slot_eq(&a.r_sub, &b.r_sub)
}

fn mod_slot_eq(a: &Option<Box<qubitick_model::AtomicComposite>>, b: &Option<Box<qubitick_model::AtomicComposite>>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => x.primary == y.primary && mods_eq(&x.mods, &y.mods),
        (None, None) => true,
        _ => false,
    }
}

fn list_eq(a: &[Expr], b: &[Expr]) -> bool {
    a.iter().zip(b).all(|(x, y)| expr_norm_eq(x, y))
}

fn binders_eq(a: &[qubitick_model::Binder], b: &[qubitick_model::Binder]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            x.vars == y.vars && expr_norm_eq(&x.ty, &y.ty)
        })
}

fn cases_eq(
    a: &[(qubitick_model::Pattern, Expr)],
    b: &[(qubitick_model::Pattern, Expr)],
) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|((pa, ea), (pb, eb))| {
            pattern_eq(pa, pb) && expr_norm_eq(ea, eb)
        })
}

fn pattern_eq(a: &qubitick_model::Pattern, b: &qubitick_model::Pattern) -> bool {
    atomic_eq(&a.constructor, &b.constructor)
        && a.binders == b.binders
        && a.sub.len() == b.sub.len()
        && a.sub.iter().zip(&b.sub).all(|(x, y)| pattern_eq(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_model::{AtomicComposite, Binder, BinderKind};

    fn sym(id: u32) -> Expr {
        Expr::Sym(AtomicComposite::new(id))
    }

    #[test]
    fn equal_sym_vars() {
        assert!(expr_norm_eq(&sym(1), &sym(1)));
        assert!(!expr_norm_eq(&sym(1), &sym(2)));
        assert!(!expr_norm_eq(&sym(1), &Expr::Var("x".into())));
    }

    #[test]
    fn bracket_is_delim() {
        let inner = Expr::Apply(AtomicComposite::new(3), vec![sym(1), sym(2)]);
        let wrapped = Expr::Bracket(9, vec![inner.clone()]);
        assert!(expr_norm_eq(&inner, &wrapped));
    }

    #[test]
    fn binders_compare_structurally() {
        let a = Expr::Binder(
            BinderKind::Forall,
            vec![Binder {
                vars: vec!["x".into()],
                ty: sym(8),
            }],
            Box::new(sym(1)),
        );
        let b = Expr::Binder(
            BinderKind::Forall,
            vec![Binder {
                vars: vec!["x".into()],
                ty: sym(8),
            }],
            Box::new(sym(1)),
        );
        assert!(expr_norm_eq(&a, &b));
    }
}