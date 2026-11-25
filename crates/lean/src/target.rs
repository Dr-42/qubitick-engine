//! `LeanTarget` — accumulator of used definitions/modules (CodeArch §5.5).

use std::collections::BTreeSet;

/// Accumulates which symbol names / modules a rendered expression references, so
/// the emitter can compute imports. Recomputed per emit; cheap to clone.
#[derive(Debug, Clone, Default)]
pub struct LeanTarget {
    /// Lean names of definitions referenced while rendering.
    used: BTreeSet<String>,
    /// Namespace of the module being emitted (e.g. `Qubitick.HL`).
    pub namespace: String,
}

impl LeanTarget {
    pub fn new(namespace: impl Into<String>) -> Self {
        LeanTarget {
            namespace: namespace.into(),
            used: BTreeSet::new(),
        }
    }

    /// Record that a definition name was referenced.
    pub fn note_use(&mut self, lean_name: &str) {
        self.used.insert(lean_name.to_string());
    }

    /// Record several definitions.
    pub fn note_uses<'a>(&mut self, names: impl IntoIterator<Item = &'a str>) {
        for n in names {
            self.note_use(n);
        }
    }

    /// The set of referenced definition names.
    pub fn used(&self) -> &BTreeSet<String> {
        &self.used
    }

    /// Set of names used but defined outside `self.namespace` (external imports).
    pub fn external_uses(&self) -> Vec<&str> {
        self.used
            .iter()
            .filter(|n| !n.starts_with(&self.namespace))
            .map(|s| s.as_str())
            .collect()
    }

    /// Whether anything has been recorded.
    pub fn is_empty(&self) -> bool {
        self.used.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_and_filters_uses() {
        let mut t = LeanTarget::new("Qubitick.HL");
        t.note_use("Qubitick.HL.mul");
        t.note_use("Mathlib.Alg");
        assert_eq!(t.used().len(), 2);
        assert_eq!(t.external_uses(), vec!["Mathlib.Alg"]);
    }
}