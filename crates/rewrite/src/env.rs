//! `Env` — the proof environment: binders and hypotheses in scope.

use qubitick_model::Expr;
use std::collections::{BTreeMap, BTreeSet};

/// Tracks variables introduced by `Fix` steps and hypotheses named by `Note`
/// steps, so citations can resolve to them.
#[derive(Debug, Clone, Default)]
pub struct Env {
    /// Local variable names currently in scope.
    pub vars: BTreeSet<String>,
    /// Local hypothesis names currently in scope, mapped to their propositions.
    pub hyps: BTreeMap<String, Expr>,
}

impl Env {
    pub fn new() -> Self {
        Env::default()
    }

    pub fn introduce(&mut self, name: &str) {
        self.vars.insert(name.to_string());
    }

    pub fn introduce_many<'a>(&mut self, names: impl IntoIterator<Item = &'a str>) {
        for n in names {
            self.introduce(n);
        }
    }

    pub fn note(&mut self, name: &str, prop: Expr) {
        self.hyps.insert(name.to_string(), prop);
    }

    pub fn has_var(&self, name: &str) -> bool {
        self.vars.contains(name)
    }

    pub fn has_hyp(&self, name: &str) -> bool {
        self.hyps.contains_key(name)
    }

    // Returning Option<Expr> as requested, avoiding lifetime issues.
    pub fn lookup(&self, name: &str) -> Option<Expr> {
        self.hyps.get(name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_fix_and_note_names() {
        let mut env = Env::new();
        env.introduce_many(["a", "b"]);
        assert!(env.has_var("a"));
        env.note("h0", Expr::var("P"));
        assert!(env.has_hyp("h0"));
        assert!(!env.has_hyp("h1"));
    }
}
