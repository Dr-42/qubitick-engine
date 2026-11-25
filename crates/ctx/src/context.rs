//! The `Ctx` structure: a scope-qualified snapshot of the corpus symbol table.

use std::collections::HashMap;

use qubitick_db::queries::SymbolRow;
use qubitick_model::corpus::Facet;

/// The symbol context. Built once per corpus/scope snapshot and shared across
/// engines as `&Ctx`.
#[derive(Debug, Clone)]
pub struct Ctx {
    /// All visible symbols, keyed by `lean_name` → row list (scope-qualified).
    by_lean: HashMap<String, Vec<SymbolRow>>,
    /// All visible symbols, keyed by `tex` → row list (lexer lookup key).
    by_tex: HashMap<String, Vec<SymbolRow>>,
    /// All visible symbols, keyed by id → row.
    by_id: HashMap<i64, SymbolRow>,
    /// The node scope this context was built for (e.g. `Era0/Strata0`).
    scope: String,
    /// The ring carrier / event symbol `𝔈` (from the `Delta` signature), if present.
    pub event: Option<String>,
    /// The unit `𝟙` and the ring carrier `𝔈` glyphs.
    pub unit: String,
    pub carrier: String,
}

impl Ctx {
    /// Build a `Ctx` from the fully-resolved, scope-appropriate symbol rows.
    pub fn from_rows(scope: &str, rows: Vec<SymbolRow>) -> Self {
        let mut by_lean: HashMap<String, Vec<SymbolRow>> = HashMap::new();
        let mut by_tex: HashMap<String, Vec<SymbolRow>> = HashMap::new();
        let mut by_id: HashMap<i64, SymbolRow> = HashMap::new();
        for r in rows {
            by_lean.entry(r.lean_name.clone()).or_default().push(r.clone());
            by_tex.entry(r.tex.clone()).or_default().push(r.clone());
            by_id.insert(r.id, r);
        }
        // Determine the ring carrier from the `Delta` symbol's signature.
        let delta = by_lean.get("Delta").and_then(|l| l.first()).cloned();
        let event = delta.and_then(|d| {
            d.trigger.clone()
        });
        let unit = by_lean
            .get("full")
            .and_then(|l| l.first())
            .map(|r| r.display.clone())
            .unwrap_or_else(|| "𝟙".into());
        let carrier = by_lean
            .get("carrier")
            .and_then(|l| l.first())
            .map(|r| r.display.clone())
            .unwrap_or_else(|| "𝔈".into());
        Ctx {
            by_lean,
            by_tex,
            by_id,
            scope: scope.to_string(),
            event,
            unit,
            carrier,
        }
    }

    pub fn scope(&self) -> &str {
        &self.scope
    }

    /// Look up a symbol row by its numeric id.
    pub fn by_id(&self, id: i64) -> Option<&SymbolRow> {
        self.by_id.get(&id)
    }

    /// All rows declared under a `lean_name` (scope-shadowing may yield several).
    pub fn by_lean_name(&self, lean: &str) -> Option<&Vec<SymbolRow>> {
        self.by_lean.get(lean)
    }

    /// All rows declared under a `tex` (lexer lookup key).
    pub fn by_tex(&self, tex: &str) -> Option<&Vec<SymbolRow>> {
        self.by_tex.get(tex)
    }

    /// Iterate over all visible symbols.
    pub fn symbols(&self) -> impl Iterator<Item = &SymbolRow> {
        self.by_id.values()
    }

    /// The D-block ElabRule string for a symbol (`'Apply'`, `'Binder'`, …), from
    /// the DB — the single source of truth. Returns `None` when the symbol has no
    /// declared D-block (a use-only atom whose role is structural, not declared).
    pub fn elab_rule_of(&self, id: i64) -> Option<&str> {
        self.by_id.get(&id).and_then(|r| r.elab.as_deref())
    }

    /// The persisted BinderKind label for a symbol (Lambda/Integral/Limit/…), from
    /// the DB — the single source of truth for how a binder operator binds.
    pub fn binder_kind_of(&self, id: i64) -> Option<&str> {
        self.by_id.get(&id).and_then(|r| r.binder_kind.as_deref())
    }

    /// Check if a symbol (by its tex) has a given facet.
    ///
    /// D9: the corpus declaration is the answer. If the corpus has a DECLARED row
    /// for this tex (a definition with a D-block, or a primitive/formation/bracket
    /// kind, or an operator with a derived role), its stored facet bitmask is
    /// authoritative — the builtin table is ONLY a fallback for texes the corpus
    /// never declares (engine fixtures and the 330-file corpus rely on that
    /// fallback for symbols like `\Delta`, `\cap`, `\varnothing` which they use
    /// but never declare). A use-only row (kind="notation", no role, no elab)
    /// does NOT count as a declaration; for those we fall back to the builtin.
    pub fn has_facet(&self, tex: &str, facet: Facet) -> bool {
        if let Some(rows) = self.by_tex.get(tex) {
            let row = &rows[0];
            // A symbol is "declared by the corpus" if it has a D-block (elab),
            // or is a non-notation kind (primitive/formation/bracket), or has a
            // derived operator role (prefix/infix/collector). Use-only symbols
            // have kind="notation", role=None, elab=None.
            let is_declared = row.elab.is_some()
                || row.kind != "notation"
                || row.role.is_some();
            if is_declared {
                row.facets & facet.bits() != 0
            } else {
                qubitick_model::corpus::is_class(tex, facet)
            }
        } else {
            qubitick_model::corpus::is_class(tex, facet)
        }
    }
}

/// Re-exported construction helper (usable as a plain function too).
pub fn create_ctx_from_rows(scope: &str, rows: Vec<SymbolRow>) -> Ctx {
    Ctx::from_rows(scope, rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i64, tex: &str, lean: &str, kind: &str) -> SymbolRow {
        SymbolRow { facets: 0,
            id,
            scope: "Era0/Strata0".into(),
            tex: tex.into(),
            display: tex.into(),
            lean_name: lean.into(),
            trigger: None,
            kind: kind.into(),
            role: None,
            precedence: None,
            assoc: None,
            home_node: 1,
            elab: None,
            binder_kind: None,
        }
    }

    #[test]
    fn builds_lookup_indexes() {
        let rows = vec![row(1, "\\Delta", "Delta", "notation"), row(2, "\\cap", "Cap", "notation")];
        let ctx = Ctx::from_rows("Era0/Strata0", rows);
        assert_eq!(ctx.by_id(1).unwrap().lean_name, "Delta");
        assert!(ctx.by_tex("\\cap").is_some());
    }
}