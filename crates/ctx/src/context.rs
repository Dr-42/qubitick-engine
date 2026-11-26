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
    /// D9: the corpus's declaration is the answer, and there is no fallback. A
    /// DECLARED row (a D-block, a non-notation kind, a role, or a facet bit) carries
    /// its facets; a use-only row is the mention of a glyph nothing declares and
    /// names no facet.
    pub fn has_facet(&self, tex: &str, facet: Facet) -> bool {
        // The DECLARATION is the only answer. A row the corpus never declared says
        // nothing, and the engine owns no table to consult instead.
        self.by_tex
            .get(tex)
            .map(|rows| {
                let row = &rows[0];
                Self::row_is_declared(row) && row.facets & facet.bits() != 0
            })
            .unwrap_or(false)
    }

    /// Whether the corpus DECLARES this glyph, rather than only using it.
    ///
    /// A declaration is a node that says what the glyph is: a D-block (`Sema:`,
    /// `Type:`), a non-notation symbol kind, a role, or a facet. A use-only row is
    /// the mention of a glyph nothing declares.
    pub fn is_declared(&self, tex: &str) -> bool {
        self.by_tex
            .get(tex)
            .map(|rows| Self::row_is_declared(&rows[0]))
            .unwrap_or(false)
    }

    fn row_is_declared(row: &SymbolRow) -> bool {
        // A facet is a claim about the glyph, so a row carrying one is a declaration
        // too: that is what ingestion writes from a corpus's `Role:`.
        row.elab.is_some() || row.kind != "notation" || row.role.is_some() || row.facets != 0
    }

    /// Every sort the corpus DECLARES: a formation (an inductive the emitter generates)
    /// or a `Type:` declaration (an opaque carrier). Deterministic by id.
    ///
    /// This is what the emitter means by "a sort" — there is no glyph list behind it.
    pub fn declared_sorts(&self) -> impl Iterator<Item = &SymbolRow> {
        let mut rows: Vec<&SymbolRow> = self
            .by_id
            .values()
            .filter(|r| matches!(r.kind.as_str(), "formation" | "type"))
            .collect();
        rows.sort_by_key(|r| r.id);
        rows.into_iter()
    }

    /// Every constructor the corpus DECLARES: a ctor of one of its formations.
    pub fn declared_ctors(&self) -> impl Iterator<Item = &SymbolRow> {
        let mut rows: Vec<&SymbolRow> = self
            .by_id
            .values()
            .filter(|r| r.kind == "ctor")
            .collect();
        rows.sort_by_key(|r| r.id);
        rows.into_iter()
    }

    /// The sort a ctor belongs to: the declared formation that declares it.
    ///
    /// Ingestion records a formation's constructors with the formation node as their
    /// `home_node`, so the family is the declared sort sharing it.
    pub fn carrier_of_ctor(&self, ctor_id: i64) -> Option<&SymbolRow> {
        let ctor = self.by_id(ctor_id)?;
        self.declared_sorts()
            .find(|s| s.kind == "formation" && s.home_node == ctor.home_node)
    }

    /// Whether `id` is a declared sort (formation or `Type:` carrier).
    pub fn is_declared_sort(&self, id: i64) -> bool {
        self.by_id(id)
            .is_some_and(|r| matches!(r.kind.as_str(), "formation" | "type"))
    }

    /// The lowest-id DECLARED row carrying `facet`.
    ///
    /// The rewriter asks this for the corpus's own nullity operator and empty
    /// element; it reads what the corpus declared, and the engine names no glyph
    /// for either. Deterministic (lowest id wins) because the row order of a
    /// `HashMap` is not.
    pub fn declared_with_facet(&self, facet: Facet) -> Option<&SymbolRow> {
        self.by_id
            .values()
            .filter(|r| Self::row_is_declared(r) && r.facets & facet.bits() != 0)
            .min_by_key(|r| r.id)
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