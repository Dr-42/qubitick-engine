//! First-order pattern matching (L1).
//!
//! A *pattern* is an `Expr` in which `Expr::Var(name)` nodes act as metavariables
//! (holes). `bind_pat` matches a pattern against a concrete `Expr` and returns the
//! substitution that makes them equal. This is the shared binding semantics used
//! by both `rewrite` and `autocite`.

use qubitick_model::{Expr, Pattern};

/// Result of a successful pattern match: metavariable name → matched subterm.
pub type Bindings = Vec<(String, Expr)>;

/// Match `pat` against `target`, producing `Bindings`.
///
/// Returns `None` if the match fails. `Expr::Var(n)` in `pat` is a metavariable:
/// the corresponding subtree of `target` is bound (first occurrence wins; later
/// occurrences must match the already-bound value). Structural nodes must agree
/// on shape, symbols by `SymbolId`, concrete vars by name.
pub fn bind_pat(pat: &Expr, target: &Expr) -> Option<Bindings> {
    let mut binds = Bindings::new();
    match_one(pat, target, &mut binds)?;
    Some(binds)
}

fn match_one(pat: &Expr, target: &Expr, binds: &mut Bindings) -> Option<()> {
    match pat {
        Expr::Var(name) => {
            match binds.iter().find(|(n, _)| n == name) {
                Some((_, existing)) => {
                    if same_value(existing, target) {
                        Some(())
                    } else {
                        None
                    }
                }
                None => {
                    binds.push((name.clone(), target.clone()));
                    Some(())
                }
            }
        }
        Expr::Sym(p) => match target {
            Expr::Sym(t) if p.primary == t.primary => {
                if mods_match(&p.mods, &t.mods, binds) {
                    Some(())
                } else {
                    None
                }
            }
            _ => None,
        }
        Expr::Apply(ph, pargs) => match target {
            Expr::Apply(th, targs) => {
                if ph.primary != th.primary || pargs.len() != targs.len() {
                    return None;
                }
                if !mods_match(&ph.mods, &th.mods, binds) {
                    return None;
                }
                for (p, t) in pargs.iter().zip(targs) {
                    match_one(p, t, binds)?;
                }
                Some(())
            }
            _ => None,
        },
        Expr::Bracket(pb, pelems) => match target {
            Expr::Bracket(tb, telems) => {
                if pb != tb || pelems.len() != telems.len() {
                    return None;
                }
                for (p, t) in pelems.iter().zip(telems) {
                    match_one(p, t, binds)?;
                }
                Some(())
            }
            _ => None,
        },
        Expr::Binder(pk, pbinders, pbody) => match target {
            Expr::Binder(tk, tbinders, tbody) => {
                if pk != tk || pbinders.len() != tbinders.len() {
                    return None;
                }
                for (pb, tb) in pbinders.iter().zip(tbinders) {
                    if pb.vars != tb.vars {
                        return None;
                    }
                    match_one(&pb.ty, &tb.ty, binds)?;
                }
                match_one(pbody, tbody, binds)?;
                Some(())
            }
            _ => None,
        },
        Expr::Recursor(ps, pcases) => match target {
            Expr::Recursor(ts, tcases) => {
                if pcases.len() != tcases.len() {
                    return None;
                }
                match_one(ps, ts, binds)?;
                for ((pp, pe), (tp, te)) in pcases.iter().zip(tcases) {
                    if !pattern_match(pp, tp) {
                        return None;
                    }
                    match_one(pe, te, binds)?;
                }
                Some(())
            }
            _ => None,
        },
        Expr::Matrix(prows) => match target {
            Expr::Matrix(trows) => grid_match(prows, trows, binds),
            _ => None,
        },
        Expr::Vector(pels) => match target {
            Expr::Vector(tels) => list_match(pels, tels, binds),
            _ => None,
        },
        Expr::Set(pels) => match target {
            Expr::Set(tels) => list_match(pels, tels, binds),
            _ => None,
        },
    }
}

fn grid_match(p: &[Vec<Expr>], t: &[Vec<Expr>], binds: &mut Bindings) -> Option<()> {
    if p.len() != t.len() {
        return None;
    }
    for (pr, tr) in p.iter().zip(t) {
        if pr.len() != tr.len() {
            return None;
        }
        for (pe, te) in pr.iter().zip(tr) {
            match_one(pe, te, binds)?;
        }
    }
    Some(())
}

fn list_match(p: &[Expr], t: &[Expr], binds: &mut Bindings) -> Option<()> {
    if p.len() != t.len() {
        return None;
    }
    for (pe, te) in p.iter().zip(t) {
        match_one(pe, te, binds)?;
    }
    Some(())
}

// Modifier matching — recurse into the four mod slots.
fn mods_match(
    p: &qubitick_model::Mods,
    t: &qubitick_model::Mods,
    binds: &mut Bindings,
) -> bool {
    slot_match(&p.l_sup, &t.l_sup, binds)
        && slot_match(&p.r_sup, &t.r_sup, binds)
        && slot_match(&p.l_sub, &t.l_sub, binds)
        && slot_match(&p.r_sub, &t.r_sub, binds)
}

fn slot_match(
    p: &Option<Box<qubitick_model::AtomicComposite>>,
    t: &Option<Box<qubitick_model::AtomicComposite>>,
    binds: &mut Bindings,
) -> bool {
    match (p, t) {
        (Some(pa), Some(ta)) => {
            pa.primary == ta.primary && mods_match(&pa.mods, &ta.mods, binds)
        }
        (None, None) => true,
        _ => false,
    }
}

fn pattern_match(p: &Pattern, t: &Pattern) -> bool {
    p.constructor.primary == t.constructor.primary
        && p.binders == t.binders
        && p.sub.len() == t.sub.len()
        && p.sub.iter().zip(&t.sub).all(|(a, b)| pattern_match(a, b))
}

/// Whether two subterms bound to the same metavariable agree.
///
/// Structural, not atom-only: a repeated metavariable is routinely bound to a
/// COMPOUND term — matching `a Δ a` against `(a Δ b) Δ (a Δ b)` binds `a` to
/// `a Δ b`, and the second occurrence must agree structurally.
fn same_value(a: &Expr, b: &Expr) -> bool {
    crate::norm::expr_norm_eq(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_model::AtomicComposite;

    fn sym(id: u32) -> Expr {
        Expr::Sym(AtomicComposite::new(id))
    }

    #[test]
    fn binds_metavariable() {
        let pat = Expr::Apply(AtomicComposite::new(1), vec![Expr::Var("x".into()), sym(2)]);
        let target = Expr::Apply(AtomicComposite::new(1), vec![sym(7), sym(2)]);
        let b = bind_pat(&pat, &target).expect("match");
        assert_eq!(b, vec![("x".to_string(), sym(7))]);
    }

    #[test]
    fn fails_on_shape_mismatch() {
        let pat = Expr::Apply(AtomicComposite::new(1), vec![Expr::Var("x".into())]);
        let target = Expr::Apply(AtomicComposite::new(1), vec![sym(2), sym(3)]);
        assert!(bind_pat(&pat, &target).is_none());
    }

    #[test]
    fn inconsistent_binding_fails() {
        let pat = Expr::Apply(
            AtomicComposite::new(1),
            vec![Expr::Var("x".into()), Expr::Var("x".into())],
        );
        let target = Expr::Apply(AtomicComposite::new(1), vec![sym(2), sym(3)]);
        assert!(bind_pat(&pat, &target).is_none());
    }
}