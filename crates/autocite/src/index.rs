//! In-RAM discrimination index over candidate lemmas (CodeArch §5.10).

use std::collections::HashMap;

use crate::candidates::Candidate;

/// A discrimination tree keyed by LHS hash and by head symbol id.
#[derive(Debug, Default)]
pub struct AutociteIndex {
    by_lhs_hash: HashMap<u128, Vec<Candidate>>,
    by_head: HashMap<u32, Vec<Candidate>>,
}

impl AutociteIndex {
    pub fn new() -> Self {
        AutociteIndex::default()
    }

    /// Add a candidate lemma to the index.
    pub fn add(&mut self, candidate: Candidate) {
        let hash = lhs_hash(&candidate.lhs);
        self.by_lhs_hash.entry(hash).or_default().push(candidate.clone());
        if let Some(head) = head_id(&candidate.lhs) {
            self.by_head.entry(head).or_default().push(candidate);
        }
    }

    /// Candidates whose LHS hash matches exactly.
    pub fn by_hash(&self, hash: u128) -> &[Candidate] {
        self.by_lhs_hash.get(&hash).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Candidates whose LHS head symbol matches `head` (structural fallback).
    pub fn by_head(&self, head: u32) -> &[Candidate] {
        self.by_head.get(&head).map(Vec::as_slice).unwrap_or(&[])
    }

    /// All candidates sharing a head symbol with `target`.
    pub fn for_target(&self, target: &qubitick_model::Expr) -> Vec<Candidate> {
        match crate::candidates::head_symbol_id(target) {
            Some(head) => self.by_head(head).to_vec(),
            None => Vec::new(),
        }
    }
}

fn lhs_hash(e: &qubitick_model::Expr) -> u128 {
    let sh = |id: u32| id as u128;
    qubitick_db::hash::hash_expr(e, &sh)
}

fn head_id(e: &qubitick_model::Expr) -> Option<u32> {
    crate::candidates::head_symbol_id(e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_model::{AtomicComposite, Expr};

    fn sym(id: u32) -> Expr {
        Expr::Sym(AtomicComposite::new(id))
    }

    #[test]
    fn indexes_and_retrieves() {
        let mut idx = AutociteIndex::new();
        idx.add(Candidate { lhs: sym(3), rhs: sym(4) });
        assert_eq!(idx.for_target(&sym(3)).len(), 1);
        assert!(!idx.by_hash(lhs_hash(&sym(3))).is_empty());
    }
}