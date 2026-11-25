//! Leftmost-subterm finding (L1).

use qubitick_model::Expr;

use crate::bind::bind_pat;

/// A path to a subterm: the sequence of child indices descending from the root.
pub type Pos = Vec<usize>;

/// Find the leftmost subterm of `e` that matches pattern `pat` (as a whole-tree
/// match, i.e. `bind_pat(pat, sub) == Some` for some subterm), returning its
/// `Pos`. The root itself is checked first, then children left-to-right.
pub fn find_pos(e: &Expr, pat: &Expr) -> Option<Pos> {
    if bind_pat(pat, e).is_some() {
        return Some(Vec::new());
    }
    // Descend into children in order, tracking indices.
    let mut path: Pos = Vec::new();
    find_in(e, pat, &mut path)
}

fn find_in(e: &Expr, pat: &Expr, path: &mut Pos) -> Option<Pos> {
    for (i, child) in e.children().iter().enumerate() {
        path.push(i);
        if bind_pat(pat, child).is_some() {
            return Some(path.clone());
        }
        if let Some(found) = find_in(child, pat, path) {
            return Some(found);
        }
        path.pop();
    }
    None
}

/// Collect every `Pos` (in left-to-right order) where `pat` matches a subterm.
pub fn find_all(e: &Expr, pat: &Expr) -> Vec<Pos> {
    let mut out = Vec::new();
    if bind_pat(pat, e).is_some() {
        out.push(Vec::new());
    }
    collect(e, pat, &mut Vec::new(), &mut out);
    out
}

fn collect(e: &Expr, pat: &Expr, path: &mut Pos, out: &mut Vec<Pos>) {
    for (i, child) in e.children().iter().enumerate() {
        path.push(i);
        if bind_pat(pat, child).is_some() {
            out.push(path.clone());
        }
        collect(child, pat, path, out);
        path.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_model::AtomicComposite;

    fn sym(id: u32) -> Expr {
        Expr::Sym(AtomicComposite::new(id))
    }

    #[test]
    fn root_match_is_empty_path() {
        let e = sym(1);
        assert_eq!(find_pos(&e, &sym(1)), Some(vec![]));
    }

    #[test]
    fn finds_deep_child() {
        let e = Expr::Apply(AtomicComposite::new(1), vec![sym(2), sym(3)]);
        let p = find_pos(&e, &sym(3)).expect("found");
        assert_eq!(p, vec![1]);
    }
}