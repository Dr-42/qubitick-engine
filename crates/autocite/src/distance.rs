//! Structural distance and diff reporting (CodeArch §5.10).

use qubitick_model::Expr;

/// Structural distance between two expressions: the number of differing leaves
/// after normalizing. Returns 0 for structurally-equal trees.
pub fn structural_distance(a: &Expr, b: &Expr) -> usize {
    if qubitick_pattern::expr_norm_eq(a, b) {
        return 0;
    }
    // Simple proxy: count leaves of the smaller tree that are not shared.
    let leaf_a = leaves(a);
    let leaf_b = leaves(b);
    let mut common = 0;
    for x in &leaf_a {
        if leaf_b.iter().any(|y| qubitick_pattern::expr_norm_eq(x, y)) {
            common += 1;
        }
    }
    (leaf_a.len().max(leaf_b.len())).saturating_sub(common)
}

/// Report the differing leaf strings between two expressions (for the gap).
pub fn diff_pairs(a: &Expr, b: &Expr) -> Vec<(String, String)> {
    let la = leaves(a);
    let lb = leaves(b);
    let mut out = Vec::new();
    for x in &la {
        if !lb.iter().any(|y| qubitick_pattern::expr_norm_eq(x, y)) {
            out.push((leaf_label(x), String::from("—")));
        }
    }
    for y in &lb {
        if !la.iter().any(|x| qubitick_pattern::expr_norm_eq(x, y)) {
            out.push((String::from("—"), leaf_label(y)));
        }
    }
    out
}

fn leaf_label(e: &Expr) -> String {
    match e {
        Expr::Sym(ac) => format!("sym#{}", ac.primary),
        Expr::Var(v) => v.clone(),
        _ => "…".to_string(),
    }
}

/// Collect leaf expressions (atomic or var).
fn leaves(e: &Expr) -> Vec<Expr> {
    let mut out = Vec::new();
    collect_leaves(e, &mut out);
    out
}

fn collect_leaves(e: &Expr, out: &mut Vec<Expr>) {
    match e {
        Expr::Sym(_) | Expr::Var(_) => out.push(e.clone()),
        _ => {
            for c in e.children() {
                collect_leaves(c, out);
            }
        }
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
    fn identical_is_zero() {
        assert_eq!(structural_distance(&sym(1), &sym(1)), 0);
    }

    #[test]
    fn different_is_positive() {
        assert!(structural_distance(&sym(1), &sym(2)) > 0);
    }

    #[test]
    fn reports_gap() {
        let pairs = diff_pairs(&sym(1), &sym(2));
        assert_eq!(pairs.len(), 2);
    }
}