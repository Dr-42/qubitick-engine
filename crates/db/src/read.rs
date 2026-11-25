//! Read paths: node lookup, AST reconstruction, and cache integration (SymbolDB §14).
//!
//! `reconstruct_expr` walks `exprs` rows bottom-up and hash-conses each distinct
//! subtree in the shared `ExprCache`. Cache hits share an `Arc<Expr>`; misses are
//! reconstructed and inserted.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Result, bail};
use qubitick_model::{AtomicComposite, Binder, BinderKind, Expr, Mods, NodeKind};
use rusqlite::Connection;

use crate::cache::ExprCache;

/// A reconstructed expression root plus its cached pointer.
pub type CachedExpr = Arc<Expr>;

/// A node header row (SymbolDB §5.1, §14.2).
#[derive(Debug, Clone)]
pub struct NodeHeader {
    pub id: i64,
    pub node_type: NodeKind,
    pub era: String,
    pub strata: i64,
    pub name: String,
    pub filename: String,
    pub tag: Option<String>,
    pub version: i64,
    /// Godel code: the registration ordinal in the corpus ledger (D_BT.32).
    pub godel: Option<i64>,
    /// The number the author wrote in the node's header (`A4. Counting` -> 4).
    /// Distinct from `godel`/`id`, which are ordinals over the walk: a citation
    /// names THIS number, so a corpus may number its nodes with gaps and in any
    /// file order without moving what a citation points at.
    pub authored: Option<i64>,
}

fn parse_kind(s: &str) -> NodeKind {
    match s {
        "Axiom" => NodeKind::Axiom,
        "Definition" => NodeKind::Definition,
        "Theorem" => NodeKind::Theorem,
        "Census" => NodeKind::Census,
        "Calibration" => NodeKind::Calibration,
        _ => NodeKind::Theorem,
    }
}

/// List all node headers (`get_node_list`, SymbolDB §14.2).
pub fn get_node_list(conn: &Connection) -> Result<Vec<NodeHeader>> {
    let mut stmt = conn.prepare(
        "SELECT id, node_type, era, strata, name, filename, tag, version, godel, authored
         FROM nodes ORDER BY era, strata, godel",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(NodeHeader {
            id: r.get(0)?,
            node_type: parse_kind(&r.get::<_, String>(1)?),
            era: r.get(2)?,
            strata: r.get(3)?,
            name: r.get(4)?,
            filename: r.get(5)?,
            tag: r.get(6)?,
            version: r.get(7)?,
            godel: r.get(8)?,
            authored: r.get(9)?,
        })
    })?;
    rows.map(|r| r.map_err(anyhow::Error::from)).collect()
}

/// One row of the `exprs` table.
#[derive(Clone)]
struct ExprRow {
    id: i64,
    shape: String,
    symbol_id: Option<i64>,
    var_name: Option<String>,
    l_sup_id: Option<i64>,
    r_sup_id: Option<i64>,
    l_sub_id: Option<i64>,
    r_sub_id: Option<i64>,
    binder_kind: Option<String>,
}

/// Reconstruct the subtree rooted at `root_id` into an `Arc<Expr>`, hash-consing
/// via `cache`. `symbol_hash_of` maps symbol ids to their build-independent hash.
///
/// This resolves depth-first: children are assembled and inserted into the cache
/// before their parent, so `resolved[id]` always holds the final `Arc` for a row.
pub fn reconstruct_expr(
    conn: &Connection,
    cache: &ExprCache,
    root_id: i64,
    symbol_hash_of: &dyn Fn(u32) -> u128,
) -> Result<CachedExpr> {
    let mut resolver = Resolver::new(conn, cache, symbol_hash_of);
    resolver.resolve(root_id)
}

struct Resolver<'a> {
    conn: &'a Connection,
    cache: &'a ExprCache,
    symbol_hash_of: &'a dyn Fn(u32) -> u128,
    rows: HashMap<i64, ExprRow>,
    resolved: HashMap<i64, CachedExpr>,
}

impl<'a> Resolver<'a> {
    fn new(
        conn: &'a Connection,
        cache: &'a ExprCache,
        symbol_hash_of: &'a dyn Fn(u32) -> u128,
    ) -> Self {
        Resolver {
            conn,
            cache,
            symbol_hash_of,
            rows: HashMap::new(),
            resolved: HashMap::new(),
        }
    }

    fn resolve(&mut self, id: i64) -> Result<CachedExpr> {
        if let Some(cached) = self.resolved.get(&id) {
            return Ok(cached.clone());
        }
        let row = self.row(id)?;
        let expr = match row.shape.as_str() {
            "Sym" => Expr::Sym(self.atomic(&row)?),
            "Var" => Expr::Var(row.var_name.clone().unwrap_or_default()),
            "Apply" => Expr::Apply(
                self.atomic(&row)?,
                self.child_exprs(&row)?,
            ),
            "Bracket" => Expr::Bracket(row.symbol_id.unwrap_or(0) as u32, self.child_exprs(&row)?),
            "Binder" => Expr::Binder(
                self.binder_kind(&row),
                self.binder_vars(&row)?,
                Box::new(self.first_child_expr(&row)?),
            ),
            "Recursor" => {
                let scrut = match self.first_child_id(&row)? {
                    Some(c) => self.resolve(c)?.as_ref().clone(),
                    None => Expr::Var("<recursor>".into()),
                };
                let cases = self.recursor_cases(&row)?;
                Expr::Recursor(Box::new(scrut), cases)
            }
            "Matrix" => {
                let rows = self.matrix_rows(&row)?;
                if rows.is_empty() {
                    // Fall back to the 1-D child view (a single-row matrix written
                    // without explicit cells) for robustness.
                    Expr::Matrix(vec![self.child_exprs(&row)?])
                } else {
                    Expr::Matrix(rows)
                }
            }
            "Vector" => Expr::Vector(self.child_exprs(&row)?),
            "Set" => Expr::Set(self.child_exprs(&row)?),
            other => bail!("unknown expr shape {other}"),
        };
        let hash = crate::hash::hash_expr(&expr, self.symbol_hash_of);
        let arc = self.cache.insert(hash, Arc::new(expr));
        self.resolved.insert(id, arc.clone());
        Ok(arc)
    }

    /// Reconstruct a Recursor's `(pattern, body)` cases from `expr_recursor_cases`.
    fn recursor_cases(
        &mut self,
        row: &ExprRow,
    ) -> Result<Vec<(qubitick_model::Pattern, Expr)>> {
        let mut stmt = self.conn.prepare(
            "SELECT pattern_id, body_expr_id FROM expr_recursor_cases
             WHERE parent_expr_id = ?1 ORDER BY position",
        )?;
        let rows: Vec<(i64, i64)> = stmt
            .query_map([row.id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()
            .map_err(|e| anyhow::anyhow!("load recursor cases of {}: {e}", row.id))?;
        let mut out = Vec::new();
        for (pat_id, body_id) in rows {
            let pat = self.pattern(pat_id)?;
            let body = self.resolve(body_id)?.as_ref().clone();
            out.push((pat, body));
        }
        Ok(out)
    }

    /// Reconstruct a `Pattern` (constructor + binder vars + sub-patterns).
    fn pattern(&mut self, id: i64) -> Result<qubitick_model::Pattern> {
        let ctor: i64 = self.conn.query_row(
            "SELECT constructor_symbol_id FROM patterns WHERE id = ?1",
            [id],
            |r| r.get(0),
        )?;
        let mut bstmt = self.conn.prepare(
            "SELECT var_name FROM pattern_binders WHERE pattern_id = ?1 ORDER BY position",
        )?;
        let binders: Vec<String> = bstmt
            .query_map([id], |r| r.get::<_, String>(0))?
            .collect::<Result<_, _>>()
            .map_err(|e| anyhow::anyhow!("load pattern binders of {id}: {e}"))?;
        let mut sstmt = self
            .conn
            .prepare("SELECT id FROM patterns WHERE parent_id = ?1 ORDER BY position")?;
        let subs: Vec<i64> = sstmt
            .query_map([id], |r| r.get::<_, i64>(0))?
            .collect::<Result<_, _>>()
            .map_err(|e| anyhow::anyhow!("load subpatterns of {id}: {e}"))?;
        let mut sub = Vec::new();
        for s in subs {
            sub.push(self.pattern(s)?);
        }
        Ok(qubitick_model::Pattern {
            constructor: qubitick_model::AtomicComposite::new(ctor as u32),
            binders,
            sub,
        })
    }

    fn row(&mut self, id: i64) -> Result<ExprRow> {
        if let Some(r) = self.rows.get(&id) {
            return Ok(r.clone());
        }
        let mut stmt = self.conn.prepare(
            "SELECT id, shape, symbol_id, var_name, l_sup_id, r_sup_id, l_sub_id, r_sub_id, binder_kind
             FROM exprs WHERE id = ?1",
        )?;
        let row = stmt
            .query_row([id], |r| {
                Ok(ExprRow {
                    id: r.get(0)?,
                    shape: r.get(1)?,
                    symbol_id: r.get(2)?,
                    var_name: r.get(3)?,
                    l_sup_id: r.get(4)?,
                    r_sup_id: r.get(5)?,
                    l_sub_id: r.get(6)?,
                    r_sub_id: r.get(7)?,
                    binder_kind: r.get(8)?,
                })
            })
            .map_err(|e| anyhow::anyhow!("load expr {id}: {e}"))?;
        self.rows.insert(id, row.clone());
        Ok(row)
    }

    fn atomic(&mut self, row: &ExprRow) -> Result<AtomicComposite> {
        Ok(AtomicComposite {
            primary: row.symbol_id.unwrap_or(0) as u32,
            mods: Mods {
                l_sup: self.mod_of(row.l_sup_id)?,
                r_sup: self.mod_of(row.r_sup_id)?,
                l_sub: self.mod_of(row.l_sub_id)?,
                r_sub: self.mod_of(row.r_sub_id)?,
            },
        })
    }

    fn mod_of(&mut self, id: Option<i64>) -> Result<Option<Box<AtomicComposite>>> {
        match id {
            Some(i) => {
                let arc = self.resolve(i)?;
                if let Expr::Sym(ac) = arc.as_ref() {
                    Ok(Some(Box::new(ac.clone())))
                } else {
                    Ok(Some(Box::new(AtomicComposite::new(0))))
                }
            }
            None => Ok(None),
        }
    }

    fn child_ids(&mut self, row: &ExprRow) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT child_id FROM expr_children WHERE parent_id = ?1 ORDER BY position",
        )?;
        let ids = stmt
            .query_map([row.id], |r| r.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!("load children of {}: {e}", row.id))?;
        Ok(ids)
    }

    fn child_exprs(&mut self, row: &ExprRow) -> Result<Vec<Expr>> {
        let ids = self.child_ids(row)?;
        let mut out = Vec::new();
        for i in ids {
            out.push(self.resolve(i)?.as_ref().clone());
        }
        Ok(out)
    }

    /// Reconstruct a Matrix's rows from `expr_matrix_cells`, ordered by (row,col).
    fn matrix_rows(&mut self, row: &ExprRow) -> Result<Vec<Vec<Expr>>> {
        let mut stmt = self.conn.prepare(
            "SELECT row, col, cell_expr_id FROM expr_matrix_cells
             WHERE parent_expr_id = ?1 ORDER BY row, col",
        )?;
        let cells: Vec<(i64, i64, i64)> = stmt
            .query_map([row.id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<_, _>>()
            .map_err(|e| anyhow::anyhow!("load matrix cells of {}: {e}", row.id))?;
        let mut rows: Vec<Vec<Expr>> = Vec::new();
        let mut cur_row = 0i64;
        let mut cur: Vec<Expr> = Vec::new();
        for (r, _c, cell_id) in cells {
            if r != cur_row {
                rows.push(std::mem::take(&mut cur));
                cur_row = r;
            }
            cur.push(self.resolve(cell_id)?.as_ref().clone());
        }
        if !cur.is_empty() {
            rows.push(cur);
        }
        Ok(rows)
    }

    fn first_child_id(&mut self, row: &ExprRow) -> Result<Option<i64>> {
        Ok(self.child_ids(row)?.into_iter().next())
    }

    fn first_child_expr(&mut self, row: &ExprRow) -> Result<Expr> {
        match self.first_child_id(row)? {
            Some(i) => Ok(self.resolve(i)?.as_ref().clone()),
            None => Ok(Expr::Var("<body>".into())),
        }
    }

    fn binder_vars(&mut self, row: &ExprRow) -> Result<Vec<Binder>> {
        let mut stmt = self.conn.prepare(
            "SELECT var_name, ty_expr_id FROM expr_binder_vars WHERE expr_id = ?1 ORDER BY position",
        )?;
        let vars = stmt
            .query_map([row.id], |r| {
                let names: String = r.get(0)?;
                let ty_id: i64 = r.get(1)?;
                Ok((names, ty_id))
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!("load binder vars of {}: {e}", row.id))?;
        let mut binders = Vec::new();
        for (names, ty_id) in vars {
            let ty = self.resolve(ty_id)?.as_ref().clone();
            binders.push(Binder {
                vars: names.split(',').map(|s| s.to_string()).collect(),
                ty,
            });
        }
        Ok(binders)
    }

    fn binder_kind(&self, row: &ExprRow) -> BinderKind {
        match row.binder_kind.as_deref() {
            Some("Forall") => BinderKind::Forall,
            Some("Exists") => BinderKind::Exists,
            Some("Lambda") => BinderKind::Lambda,
            Some("Integral") => BinderKind::Integral,
            Some("Limit") => BinderKind::Limit,
            _ => BinderKind::Lambda,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{Connection, Transaction};

    fn setup() -> (Connection, i64) {
        let conn = crate::conn::open_in_memory().unwrap();
        let mut tx = conn.unchecked_transaction().unwrap();
        write_test_rows(&mut tx);
        tx.commit().unwrap();
        (conn, 1)
    }

    fn write_test_rows(tx: &mut Transaction) {
        // minimal storable expr: Var("x")
        tx.execute(
            "INSERT INTO nodes (id, node_type, era, strata, name, filename, node_hash)
             VALUES (1, 'Theorem', 'Era0', 0, 'Test', 't.qbit', x'00')",
            [],
        )
        .unwrap();
        tx.execute(
            "INSERT INTO exprs (id, node_id, shape, var_name, expr_hash)
             VALUES (10, 1, 'Var', 'x', x'00')",
            [],
        )
        .unwrap();
    }

    #[test]
    fn reconstructs_var() {
        let (conn, node_id) = setup();
        let cache = ExprCache::new();
        let h = |_id: u32| 0_u128;
        let root = reconstruct_expr(&conn, &cache, 10, &h).unwrap();
        assert!(matches!(root.as_ref(), Expr::Var(v) if v == "x"));
        assert_eq!(cache.len(), 1);
        let _ = node_id;
    }

    // Write each Expr form via the real write path and read it back equal.
    // Symbols 200/201 (a, b) are shared across the cases.
    fn seed_symbols(tx: &mut Transaction) {
        for (id, tex, nm) in [(200u32, "/a", "a"), (201u32, "/b", "b")] {
            tx.execute(
                "INSERT INTO symbols (id, scope, tex, display, lean_name, kind, home_node, symbol_hash)
                 VALUES (?1, 'Era0/Strata0', ?2, ?3, ?4, 'notation', 1, x'00')",
                rusqlite::params![id, tex, nm, nm],
            )
            .unwrap();
        }
    }
    fn write_and_read(e: &Expr) -> Expr {
        let conn = crate::conn::open_in_memory().unwrap();
        let root;
        {
            let mut tx = conn.unchecked_transaction().unwrap();
            tx.execute(
                "INSERT INTO nodes (id, node_type, era, strata, name, filename, node_hash)
                 VALUES (1, 'Definition', 'Era0', 0, 'N', 'n.qbit', x'00')",
                [],
            )
            .unwrap();
            seed_symbols(&mut tx);
            let h = |id: u32| id as u128;
            root = crate::write::write_expr(&tx, e, 1, None, None, &h).unwrap();
            tx.commit().unwrap();
        }
        let cache = ExprCache::new();
        let h = |id: u32| id as u128;
        reconstruct_expr(&conn, &cache, root, &h).unwrap().as_ref().clone()
    }
    macro_rules! sym { () => { Expr::Sym(qubitick_model::AtomicComposite::new(200)) } }

    #[test]
    fn roundtrips_apply() {
        let e = Expr::Apply(qubitick_model::AtomicComposite::new(200), vec![sym!(), Expr::Var("x".into())]);
        assert_eq!(write_and_read(&e), e);
    }
    #[test]
    fn roundtrips_vector() {
        let e = Expr::Vector(vec![sym!(), sym!(), sym!()]);
        assert_eq!(write_and_read(&e), e);
    }
    #[test]
    fn roundtrips_set() {
        let e = Expr::Set(vec![sym!(), Expr::Var("y".into())]);
        assert_eq!(write_and_read(&e), e);
    }
    #[test]
    fn roundtrips_binder() {
        use qubitick_model::BinderKind;
        let e = Expr::Binder(
            BinderKind::Forall,
            vec![qubitick_model::Binder { vars: vec!["a".into(), "b".into()], ty: sym!() }],
            Box::new(Expr::Apply(qubitick_model::AtomicComposite::new(201), vec![sym!(), sym!()])),
        );
        assert_eq!(write_and_read(&e), e);
    }
    #[test]
    fn roundtrips_bracket() {
        let e = Expr::Bracket(201, vec![sym!()]);
        assert_eq!(write_and_read(&e), e);
    }

    #[test]
    fn roundtrips_recursor_cases() {
        // A Recursor with patterns + bodies must preserve its cases through the DB.
        let e = Expr::Recursor(
            Box::new(sym!()),
            vec![
                (
                    qubitick_model::Pattern {
                        constructor: qubitick_model::AtomicComposite::new(200),
                        binders: vec![],
                        sub: vec![],
                    },
                    sym!(),
                ),
                (
                    qubitick_model::Pattern {
                        constructor: qubitick_model::AtomicComposite::new(201),
                        binders: vec!["k".into()],
                        sub: vec![],
                    },
                    Expr::Var("k".into()),
                ),
            ],
        );
        let got = write_and_read(&e);
        let Expr::Recursor(_, cases) = got else {
            panic!("expected Recursor")
        };
        assert_eq!(cases.len(), 2, "two cases preserved");
        assert_eq!(cases[1].0.binders, vec!["k".to_string()]);
    }

    #[test]
    fn reconstructs_matrix_cells() {
        // Write a 2x2 Matrix via the real write path, then read it back and
        // assert the 2-D rows are preserved (expr_matrix_cells round-trip).
        let conn = crate::conn::open_in_memory().unwrap();
        let root_id;
        {
            let tx = conn.unchecked_transaction().unwrap();
            tx.execute(
                "INSERT INTO nodes (id, node_type, era, strata, name, filename, node_hash)
                 VALUES (1, 'Definition', 'Era0', 0, 'M', 'm.qbit', x'00')",
                [],
            )
            .unwrap();
            tx.execute(
                "INSERT INTO symbols (id, scope, tex, display, lean_name, kind, home_node, symbol_hash)
                 VALUES (200, 'Era0/Strata0', '/a', 'a', 'a', 'notation', 1, x'00')",
                [],
            )
            .unwrap();
            tx.execute(
                "INSERT INTO symbols (id, scope, tex, display, lean_name, kind, home_node, symbol_hash)
                 VALUES (201, 'Era0/Strata0', '/b', 'b', 'b', 'notation', 1, x'00')",
                [],
            )
            .unwrap();
            let m = qubitick_model::Expr::Matrix(vec![
                vec![Expr::Sym(qubitick_model::AtomicComposite::new(200)),
                     Expr::Sym(qubitick_model::AtomicComposite::new(201))],
                vec![Expr::Sym(qubitick_model::AtomicComposite::new(200)),
                     Expr::Sym(qubitick_model::AtomicComposite::new(201))],
            ]);
            let h = |id: u32| id as u128;
            root_id = crate::write::write_expr(&tx, &m, 1, None, None, &h).unwrap();
            tx.commit().unwrap();
        }
        let cache = ExprCache::new();
        let h = |id: u32| id as u128;
        let root = reconstruct_expr(&conn, &cache, root_id, &h).unwrap();
        let Expr::Matrix(rows) = root.as_ref() else {
            panic!("expected Matrix");
        };
        assert_eq!(rows.len(), 2, "two rows");
        assert_eq!(rows[0].len(), 2, "two columns");
    }
}