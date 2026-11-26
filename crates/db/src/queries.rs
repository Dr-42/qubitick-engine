//! Query API (SymbolDB §10, §14): symbols in scope, cite info, autocite candidates,
//! and usage counts. Consumed by `ctx`, `palette`, and `autocite` engines.

use anyhow::Result;
use qubitick_model::Scope;
use rusqlite::Connection;

/// A symbol row, as consumed by `Ctx::build` and the palette.
#[derive(Debug, Clone)]
pub struct SymbolRow {
    pub id: i64,
    pub scope: String,
    pub tex: String,
    pub display: String,
    pub lean_name: String,
    pub trigger: Option<String>,
    pub kind: String,
    pub role: Option<String>,
    pub precedence: Option<i64>,
    pub home_node: i64,
    /// The D-block ElabRule for this symbol (single source of truth, DataSchema
    /// §6.4): 'Identity'|'Apply'|'CurriedApply'|'Binder'|'Matrix'|'Vector'|'Set'|
    /// 'Wrap'. None when the symbol is a use-only atom with no declared D-block.
    pub elab: Option<String>,
    /// The BinderKind (Lambda/Integral/Limit/…) when elab is `Binder`.
    pub binder_kind: Option<String>,
    /// Facet bitmask: the classifications the corpus's own declarations state.
    pub facets: u32,
}

/// Every symbol the corpus has, in every scope.
///
/// The question "what does this project know" is not scope-shaped: the interface lists symbols,
/// parses a buffer it has not placed in a stratum yet, and answers autocite — none of which is a
/// node in one scope. `symbols_in_scope` is the scope-shaped question, and it is the one the build
/// asks, per file, from the file's own path.
pub fn symbols_all(conn: &Connection) -> Result<Vec<SymbolRow>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.scope, s.tex, s.display, s.lean_name, s.trigger, s.kind, s.role,
                s.precedence, s.home_node, s.facets, e.rule, e.binder_kind
         FROM symbols s
         LEFT JOIN symbol_elab_rules e ON e.symbol_id = s.id
         ORDER BY s.lean_name",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(SymbolRow {
            id: r.get(0)?,
            scope: r.get(1)?,
            tex: r.get(2)?,
            display: r.get(3)?,
            lean_name: r.get(4)?,
            trigger: r.get(5)?,
            kind: r.get(6)?,
            role: r.get(7)?,
            precedence: r.get(8)?,
            home_node: r.get(9)?,
            facets: r.get(10)?,
            elab: r.get(11)?,
            binder_kind: r.get(12)?,
        })
    })?;
    rows.map(|r| r.map_err(anyhow::Error::from))
        .collect::<Result<Vec<SymbolRow>, anyhow::Error>>()
}

/// All symbols visible at `scope` (own scope, global `''`, and ancestor scopes).
/// (SymbolDB §14.6)
pub fn symbols_in_scope(conn: &Connection, scope: &str) -> Result<Vec<SymbolRow>> {
    // DataSchema §2: visible = global `''`, own scope, ancestor path, or a symbol
    // in the node's era at an equal-or-earlier strata (within-era substrate).
    let node = Scope(scope.to_string());
    Ok(symbols_all(conn)?
        .into_iter()
        .filter(|s| node.visible_from(&Scope(s.scope.clone())))
        .collect())
}

/// Resolve a symbol by its LaTeX `tex` within `scope` (lexer lookup).
pub fn symbol_by_tex(conn: &Connection, scope: &str, tex: &str) -> Result<Option<SymbolRow>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.scope, s.tex, s.display, s.lean_name, s.trigger, s.kind, s.role,
                s.precedence, s.home_node, s.facets, e.rule, e.binder_kind
         FROM symbols s
         LEFT JOIN symbol_elab_rules e ON e.symbol_id = s.id
         WHERE s.tex = ?1 AND (s.scope = ?2 OR s.scope = '' OR s.scope LIKE ?2 || '/%')
         ORDER BY length(s.scope) DESC
         LIMIT 1",
    )?;
    let mut rows = stmt.query_map([tex, scope], |r| {
        Ok(SymbolRow {
            id: r.get(0)?,
            scope: r.get(1)?,
            tex: r.get(2)?,
            display: r.get(3)?,
            lean_name: r.get(4)?,
            trigger: r.get(5)?,
            kind: r.get(6)?,
            role: r.get(7)?,
            precedence: r.get(8)?,
            home_node: r.get(9)?,
            facets: r.get(10)?,
            elab: r.get(11)?,
            binder_kind: r.get(12)?,
        })
    })?;
    Ok(rows.next().transpose()?)
}

/// Cite-info for a citation text like `A1.4`, `T.BT.01`, `D.BT.04` (SymbolDB §14.4).
#[derive(Debug, Clone)]
pub struct CiteInfo {
    pub node_id: i64,
    pub line: Option<i64>,
    pub name: String,
    pub era: String,
    pub strata: i64,
}

pub fn cite_info(conn: &Connection, cite_text: &str) -> Result<Option<CiteInfo>> {
    let mut stmt = conn.prepare(
        "SELECT cn.node_id, cn.line, n.name, n.era, n.strata
         FROM cite_names cn JOIN nodes n ON n.id = cn.node_id
         WHERE cn.cite_text = ?1",
    )?;
    let mut rows = stmt.query_map([cite_text], |r| {
        Ok(CiteInfo {
            node_id: r.get(0)?,
            line: r.get(1)?,
            name: r.get(2)?,
            era: r.get(3)?,
            strata: r.get(4)?,
        })
    })?;
    Ok(rows.next().transpose()?)
}

/// A candidate lemma for autocite (SymbolDB §14.5, §16).
#[derive(Debug, Clone)]
pub struct AutociteCandidate {
    pub node_id: i64,
    pub lemma_line: i64,
    pub direction: String,
    pub pattern_expr_id: i64,
}

/// All lemmas whose LHS hash matches `lhs_hash` (both directions).
pub fn autocite_candidates(conn: &Connection, lhs_hash: u128) -> Result<Vec<AutociteCandidate>> {
    let mut stmt = conn.prepare(
        "SELECT node_id, lemma_line, direction, pattern_expr_id
         FROM lemma_lhs_patterns
         WHERE lhs_hash = ?1
         ORDER BY node_id, lemma_line",
    )?;
    let rows = stmt
        .query_map([lhs_hash.to_le_bytes().to_vec()], |r| {
            Ok(AutociteCandidate {
                node_id: r.get(0)?,
                lemma_line: r.get(1)?,
                direction: r.get(2)?,
                pattern_expr_id: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// The root expr id for a node's statement (SymbolDB §5.4).
pub fn statement_root(conn: &Connection, node_id: i64, line: i64) -> Result<Option<i64>> {
    let mut stmt = conn.prepare(
        "SELECT expr_id FROM node_statements WHERE node_id = ?1 AND line = ?2",
    )?;
    let mut rows = stmt.query_map([node_id, line], |r| r.get::<_, i64>(0))?;
    Ok(rows.next().transpose()?)
}

/// Per-symbol total usage counts across the whole corpus (SymbolDB §10.2), as
/// `(symbol_id, occurrence_count)` ordered by symbol id.
pub fn symbol_usage_counts(conn: &Connection) -> Result<Vec<(i64, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT symbol_id, SUM(occurrence_count) FROM symbol_usage GROUP BY symbol_id ORDER BY symbol_id",
    )?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?
        .map(|r| r.map_err(anyhow::Error::from))
        .collect::<Result<Vec<_>>>()?;
    Ok(rows)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_scope_returns_nothing() {
        let conn = crate::conn::open_in_memory().unwrap();
        let rows = symbols_in_scope(&conn, "Era0/Strata0").unwrap();
        assert!(rows.is_empty());
    }
}