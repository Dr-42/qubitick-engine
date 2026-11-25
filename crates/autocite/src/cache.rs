//! RAM cache of autocite results (CodeArch §5.10).

use std::collections::HashMap;

use qubitick_model::Expr;

use crate::AutociteResult;

/// Memoizes autocite results keyed by the structural hashes of `(lhs, rhs)`.
#[derive(Debug, Default)]
pub struct AutociteCache {
    by_key: HashMap<(u128, u128), AutociteResult>,
}

impl AutociteCache {
    pub fn new() -> Self {
        AutociteCache::default()
    }

    fn key(lhs: &Expr, rhs: &Expr) -> (u128, u128) {
        // Build-independent hash using the symbol-id hash as the identity.
        let sh = |id: u32| id as u128;
        (
            qubitick_db::hash::hash_expr(lhs, &sh),
            qubitick_db::hash::hash_expr(rhs, &sh),
        )
    }

    pub fn get(&self, lhs: &Expr, rhs: &Expr) -> Option<&AutociteResult> {
        self.by_key.get(&Self::key(lhs, rhs))
    }

    pub fn insert(&mut self, lhs: &Expr, rhs: &Expr, result: AutociteResult) {
        self.by_key.insert(Self::key(lhs, rhs), result);
    }

    pub fn clear(&mut self) {
        self.by_key.clear();
    }

    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_model::AtomicComposite;

    fn result(e: &Expr) -> AutociteResult {
        AutociteResult {
            lhs: e.clone(),
            rhs: e.clone(),
            chain: vec![e.clone()],
            reached: true,
            closest: None,
            gap: None,
        }
    }

    #[test]
    fn memoizes_identical_queries() {
        let mut c = AutociteCache::new();
        let e = Expr::Sym(AtomicComposite::new(1));
        c.insert(&e, &e, result(&e));
        assert_eq!(c.len(), 1);
        assert!(c.get(&e, &e).is_some());
    }
}