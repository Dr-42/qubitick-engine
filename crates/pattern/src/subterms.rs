//! Subterm collection and traversal helpers (L1).

use qubitick_model::Expr;

/// Collect `e` and every proper subexpression into `out` (preorder, self first).
pub fn expr_subterms(e: &Expr, out: &mut Vec<Expr>) {
    out.push(e.clone());
    match e {
        Expr::Sym(_) | Expr::Var(_) => {}
        Expr::Apply(_, args) => {
            for a in args {
                expr_subterms(a, out);
            }
        }
        Expr::Bracket(_, elems) => {
            for x in elems {
                expr_subterms(x, out);
            }
        }
        Expr::Binder(_, binders, body) => {
            for b in binders {
                expr_subterms(&b.ty, out);
            }
            expr_subterms(body, out);
        }
        Expr::Recursor(s, cases) => {
            expr_subterms(s, out);
            for (_, body) in cases {
                expr_subterms(body, out);
            }
        }
        Expr::Matrix(rows) => {
            for r in rows {
                for c in r {
                    expr_subterms(c, out);
                }
            }
        }
        Expr::Vector(elts) | Expr::Set(elts) => {
            for e in elts {
                expr_subterms(e, out);
            }
        }
    }
}

/// Replace the subterm at `pos` with `new`, returning the rebuilt root.
///
/// `pos` is a `Pos` path as produced by [`crate::find::find_all`]: the empty path
/// is the root, and each step is a child index into the preorder walk that
/// `Expr::children` and the recursive helpers above share — so the two functions
/// agree by construction.
///
/// This is THE missing primitive behind both engines that consume it: `rewrite`
/// needs it to apply a cited law at the subterm a proof chain actually changes,
/// and `autocite` needs it to generate real one-step neighbours instead of the
/// placeholder that rewrote the last argument of an application.
pub fn replace_at(e: &Expr, pos: &[usize], new: Expr) -> Expr {
    if pos.is_empty() {
        return new;
    }
    let (i, rest) = (pos[0], &pos[1..]);
    match e {
        Expr::Sym(_) | Expr::Var(_) => e.clone(),
        Expr::Apply(h, args) => {
            let mut args = args.clone();
            if i < args.len() {
                args[i] = replace_at(&args[i], rest, new);
            }
            Expr::Apply(h.clone(), args)
        }
        Expr::Bracket(id, elems) => {
            let mut elems = elems.clone();
            if i < elems.len() {
                elems[i] = replace_at(&elems[i], rest, new);
            }
            Expr::Bracket(*id, elems)
        }
        Expr::Binder(kind, binders, body) => {
            let mut binders = binders.clone();
            if i < binders.len() {
                binders[i].ty = replace_at(&binders[i].ty, rest, new);
                return Expr::Binder(*kind, binders, body.clone());
            }
            let j = i - binders.len();
            if j == 0 {
                Expr::Binder(*kind, binders, Box::new(replace_at(body, rest, new)))
            } else {
                e.clone()
            }
        }
        Expr::Recursor(s, cases) => {
            if i == 0 {
                return Expr::Recursor(Box::new(replace_at(s, rest, new)), cases.clone());
            }
            let mut cases = cases.clone();
            let j = i - 1;
            if j < cases.len() {
                cases[j].1 = replace_at(&cases[j].1, rest, new);
            }
            Expr::Recursor(s.clone(), cases)
        }
        Expr::Matrix(rows) => {
            let mut rows = rows.clone();
            let mut idx = 0;
            for r in &mut rows {
                for c in r.iter_mut() {
                    if idx == i {
                        *c = replace_at(c, rest, new);
                        return Expr::Matrix(rows);
                    }
                    idx += 1;
                }
            }
            Expr::Matrix(rows)
        }
        Expr::Vector(elts) | Expr::Set(elts) => {
            let mut elts = elts.clone();
            if i < elts.len() {
                elts[i] = replace_at(&elts[i], rest, new);
            }
            match e {
                Expr::Vector(_) => Expr::Vector(elts),
                _ => Expr::Set(elts),
            }
        }
    }
}

/// Depth of the expression tree (a bare atomic or variable has depth 1).
pub fn expr_depth(e: &Expr) -> usize {
    1 + e
        .children()
        .iter()
        .map(|c| expr_depth(c))
        .max()
        .unwrap_or(0)
}

/// Visit every descendant (including `e`) with a callback, preorder.
pub fn walk<F>(e: &Expr, f: &mut F)
where
    F: FnMut(&Expr),
{
    f(e);
    for c in e.children() {
        walk(c, f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_model::AtomicComposite;

    fn sym(id: u32) -> Expr {
        Expr::Sym(AtomicComposite::new(id))
    }

    /// Every position `find_all` reports must be a position `replace_at` accepts,
    /// and the replacement must land where the finder said. The two are the shared
    /// contract the rewrite/autocite engine is built on.
    #[test]
    fn replace_at_round_trips_with_find_all() {
        use crate::find::find_all;

        let x = || Expr::Var("x".into());
        let deep = || {
            Expr::Apply(
                AtomicComposite::new(1),
                vec![sym(2), Expr::Apply(AtomicComposite::new(1), vec![sym(3), x()])],
            )
        };

        for pat in [sym(3), x(), sym(2)] {
            for p in find_all(&deep(), &pat) {
                let marker = sym(99);
                let out = replace_at(&deep(), &p, marker.clone());
                // The marker must sit exactly at the reported path.
                let mut got = out.clone();
                for i in &p {
                    got = got.children()[*i].clone();
                }
                assert_eq!(got, marker, "pos {p:?} did not round-trip");
            }
        }
    }

    #[test]
    fn replace_at_empty_path_replaces_the_root() {
        let e = Expr::Apply(AtomicComposite::new(1), vec![sym(2)]);
        assert_eq!(replace_at(&e, &[], sym(7)), sym(7));
    }

    #[test]
    fn collects_subterms() {
        let e = Expr::Apply(AtomicComposite::new(1), vec![sym(2), Expr::Var("x".into())]);
        let mut out = Vec::new();
        expr_subterms(&e, &mut out);
        assert_eq!(out.len(), 3);
        assert_eq!(expr_depth(&e), 2);
    }
}