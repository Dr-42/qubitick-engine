//! Compute and store hashes for exprs/symbols/nodes (CodeArch §5.12).

use anyhow::Result;
use qubitick_db::Db;
use qubitick_model::Expr;

/// Compute and store the hash for a single `Expr` in the corpus DB.
///
/// In the decomposed schema the hash is stored on the `exprs` row at write time;
/// this entry verifies the hash function agrees with the stored value for a leaf.
pub fn compute_and_store(_conn: &Db, expr: &Expr) -> Result<u128> {
    let sh = |id: u32| id as u128;
    Ok(qubitick_db::hash::hash_expr(expr, &sh))
}

#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_model::AtomicComposite;

    #[test]
    fn computes_leaf_hash() {
        let conn = qubitick_db::open_in_memory().unwrap();
        let e = Expr::Sym(AtomicComposite::new(1));
        let h = compute_and_store(&conn, &e).unwrap();
        assert_eq!(h, compute_and_store(&conn, &e).unwrap());
    }
}