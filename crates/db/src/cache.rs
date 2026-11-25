//! RAM hash-consed expression cache (SymbolDB §15).
//!
//! A single map keyed by `H_expr` (`u128`) storing `Arc<Expr>`. Identical
//! subtrees across different statements share one in-memory node. Lifecycle:
//! lazily built on read, persists across requests, invalidated on node save.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use qubitick_model::Expr;

/// The shared, lock-free-ish expression cache.
#[derive(Default)]
pub struct ExprCache {
    inner: RwLock<HashMap<u128, Arc<Expr>>>,
    hits: Mutex<u64>,
    misses: Mutex<u64>,
}

impl ExprCache {
    pub fn new() -> Self {
        ExprCache::default()
    }

    /// Look up a cached expression by its structural hash.
    pub fn get(&self, hash: u128) -> Option<Arc<Expr>> {
        let got = self.inner.read().ok()?.get(&hash).cloned();
        if got.is_some() {
            *self.hits.lock().unwrap() += 1;
        } else {
            *self.misses.lock().unwrap() += 1;
        }
        got
    }

    /// Insert (or return the existing) expression for `hash`, hash-consing.
    pub fn insert(&self, hash: u128, expr: Arc<Expr>) -> Arc<Expr> {
        let mut guard = self.inner.write().unwrap();
        guard.entry(hash).or_insert(expr).clone()
    }

    /// Remove hash-consed entries for the given hashes (used on node save/build).
    pub fn invalidate(&self, hashes: &[u128]) {
        let mut guard = self.inner.write().unwrap();
        for h in hashes {
            guard.remove(h);
        }
    }

    pub fn clear(&self) {
        self.inner.write().unwrap().clear();
    }

    pub fn len(&self) -> usize {
        self.inner.read().map(|g| g.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn stats(&self) -> (u64, u64) {
        (*self.hits.lock().unwrap(), *self.misses.lock().unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_model::AtomicComposite;

    #[test]
    fn hash_consing_deduplicates() {
        let cache = ExprCache::new();
        let e = Arc::new(Expr::Var("x".into()));
        let a = cache.insert(7, e.clone());
        let b = cache.insert(7, e.clone());
        assert!(Arc::ptr_eq(&a, &b));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn invalidate_removes() {
        let cache = ExprCache::new();
        cache.insert(5, Arc::new(Expr::Sym(AtomicComposite::new(1))));
        assert_eq!(cache.len(), 1);
        cache.invalidate(&[5]);
        assert_eq!(cache.len(), 0);
    }
}