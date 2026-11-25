//! Write paths: decompose `model` values into per-occurrence SQL rows (SymbolDB §12).
//!
//! Step 4 of the build/autosave path decomposes every `Expr` into one `exprs`
//! row plus child rows. Symbol rows persist across builds; AST and proof rows are
//! replaced atomically per node.

use anyhow::{bail, Result};
use qubitick_model::{AtomicComposite, BinderKind, Expr, Mods};
use rusqlite::{Connection, Transaction};

use crate::hash::hash_expr;

/// The four mod-slot row ids returned by [`write_mods`] (l_sup, r_sup, l_sub, r_sub).
type ModIds = (Option<i64>, Option<i64>, Option<i64>, Option<i64>);
/// Decompose `expr` and write it as rows under `node_id`, returning the root row id.
///
/// `symbol_hash_of` maps a `SymbolId` to its build-independent hash (for the
/// stored `expr_hash`). Children are written before the parent so FKs resolve.
pub fn write_expr(
    tx: &Transaction,
    expr: &Expr,
    node_id: i64,
    parent_id: Option<i64>,
    position: Option<i64>,
    symbol_hash_of: &dyn Fn(u32) -> u128,
) -> Result<i64> {
    // Write mods first (recursion to obtain their row ids).
    let (l_sup, r_sup, l_sub, r_sub) = match expr {
        Expr::Sym(ac) => write_mods(tx, &ac.mods, node_id, symbol_hash_of)?,
        Expr::Apply(ac, _) => write_mods(tx, &ac.mods, node_id, symbol_hash_of)?,
        Expr::Bracket(_, _) => {
            // Bracket keeps its own SymbolId; mods are not modeled for it.
            (None, None, None, None)
        }
        _ => (None, None, None, None),
    };

    let shape = shape_tag(expr);
    let symbol_id = symbol_id_of(expr);
    let var_name = match expr {
        Expr::Var(v) => Some(v.clone()),
        _ => None,
    };
    let binder_kind = match expr {
        Expr::Binder(k, _, _) => Some(binder_kind_tag(*k)),
        _ => None,
    };

    let eh = hash_expr(expr, symbol_hash_of);

    tx.execute(
        "INSERT INTO exprs (node_id, parent_id, position, shape, symbol_id, var_name,
                            l_sup_id, r_sup_id, l_sub_id, r_sub_id, binder_kind, expr_hash)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        rusqlite::params![
            node_id,
            parent_id,
            position,
            shape,
            symbol_id,
            var_name,
            l_sup,
            r_sup,
            l_sub,
            r_sub,
            binder_kind,
            eh.to_le_bytes().to_vec(),
        ],
    )?;
    let id = tx.last_insert_rowid();

    // Write one-dimensional children (Apply args, Bracket args, Binder body,
    // Recursor scrutinee, Vector/Set elements).
    for (pos, child) in children_of(expr).iter().enumerate() {
        let child_id = write_expr(tx, child, node_id, Some(id), Some(pos as i64), symbol_hash_of)?;
        tx.execute(
            "INSERT INTO expr_children (parent_id, position, child_id) VALUES (?1,?2,?3)",
            rusqlite::params![id, pos as i64, child_id],
        )?;
    }

    // Binder variables.
    if let Expr::Binder(_, binders, _) = expr {
        for (pos, b) in binders.iter().enumerate() {
            let ty_id =
                write_expr(tx, &b.ty, node_id, None, None, symbol_hash_of)?;
            tx.execute(
                "INSERT INTO expr_binder_vars (expr_id, position, var_name, ty_expr_id)
                 VALUES (?1,?2,?3,?4)",
                rusqlite::params![id, pos as i64, b.vars.join(","), ty_id],
            )?;
        }
    }

    // Recursor cases: patterns + bodies (DataSchema §7). Children above wrote the
    // scrutinee at position 0; each case links a pattern row to a body expr row.
    if let Expr::Recursor(_scrut, cases) = expr {
        for (pos, (pat, body)) in cases.iter().enumerate() {
            let pat_id = write_pattern(tx, pat, None, Some(pos as i64))?;
            let body_id = write_expr(tx, body, node_id, None, None, symbol_hash_of)?;
            tx.execute(
                "INSERT INTO expr_recursor_cases
                   (parent_expr_id, position, pattern_id, body_expr_id)
                 VALUES (?1,?2,?3,?4)",
                rusqlite::params![id, pos as i64, pat_id, body_id],
            )?;
        }
    }

    // Matrix cells (2D).
    if let Expr::Matrix(rows) = expr {
        for (r, row) in rows.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                let cell_id =
                    write_expr(tx, cell, node_id, None, None, symbol_hash_of)?;
                tx.execute(
                    "INSERT INTO expr_matrix_cells (parent_expr_id, row, col, cell_expr_id)
                     VALUES (?1,?2,?3,?4)",
                    rusqlite::params![id, r as i64, c as i64, cell_id],
                )?;
            }
        }
    }

    Ok(id)
}

/// Write a `Pattern` as `patterns` + `pattern_binders` rows (recursively for
/// sub-patterns), returning the pattern id.
fn write_pattern(
    tx: &Transaction,
    pat: &qubitick_model::Pattern,
    parent_id: Option<i64>,
    position: Option<i64>,
) -> Result<i64> {
    tx.execute(
        "INSERT INTO patterns (parent_id, position, constructor_symbol_id)
         VALUES (?1,?2,?3)",
        rusqlite::params![parent_id, position, pat.constructor.primary as i64],
    )?;
    let pid = tx.last_insert_rowid();
    for (i, v) in pat.binders.iter().enumerate() {
        tx.execute(
            "INSERT INTO pattern_binders (pattern_id, position, var_name) VALUES (?1,?2,?3)",
            rusqlite::params![pid, i as i64, v],
        )?;
    }
    for (i, sub) in pat.sub.iter().enumerate() {
        write_pattern(tx, sub, Some(pid), Some(i as i64))?;
    }
    Ok(pid)
}

fn write_mods(
    tx: &Transaction,
    m: &Mods,
    node_id: i64,
    symbol_hash_of: &dyn Fn(u32) -> u128,
) -> Result<ModIds> {
    let write_one = |ac: &Option<Box<AtomicComposite>>| -> Result<Option<i64>> {
        match ac {
            Some(inner) => Ok(Some(write_expr(
                tx,
                &Expr::Sym(inner.as_ref().clone()),
                node_id,
                None,
                None,
                symbol_hash_of,
            )?)),
            None => Ok(None),
        }
    };
    Ok((
        write_one(&m.l_sup)?,
        write_one(&m.r_sup)?,
        write_one(&m.l_sub)?,
        write_one(&m.r_sub)?,
    ))
}

fn shape_tag(e: &Expr) -> &'static str {
    match e {
        Expr::Sym(_) => "Sym",
        Expr::Var(_) => "Var",
        Expr::Apply(_, _) => "Apply",
        Expr::Bracket(_, _) => "Bracket",
        Expr::Binder(_, _, _) => "Binder",
        Expr::Recursor(_, _) => "Recursor",
        Expr::Matrix(_) => "Matrix",
        Expr::Vector(_) => "Vector",
        Expr::Set(_) => "Set",
    }
}

fn symbol_id_of(e: &Expr) -> Option<i64> {
    match e {
        Expr::Sym(ac) => Some(ac.primary as i64),
        Expr::Apply(ac, _) => Some(ac.primary as i64),
        Expr::Bracket(id, _) => Some(*id as i64),
        _ => None,
    }
}

fn binder_kind_tag(k: BinderKind) -> &'static str {
    match k {
        BinderKind::Forall => "Forall",
        BinderKind::Exists => "Exists",
        BinderKind::Lambda => "Lambda",
        BinderKind::Integral => "Integral",
        BinderKind::Limit => "Limit",
    }
}

/// Return the one-dimensional child expressions of `e` (SymbolDB §7.2).
fn children_of(e: &Expr) -> Vec<&Expr> {
    match e {
        Expr::Apply(_, args) => args.iter().collect(),
        Expr::Bracket(_, args) => args.iter().collect(),
        Expr::Binder(_, _, body) => vec![body.as_ref()],
        Expr::Recursor(scrut, _) => vec![scrut.as_ref()],
        Expr::Vector(els) | Expr::Set(els) => els.iter().collect(),
        Expr::Matrix(_) | Expr::Sym(_) | Expr::Var(_) => Vec::new(),
    }
}

/// Validate that `expr` is storable (used as a pre-commit invariant check).
pub fn validate_expr(e: &Expr) -> Result<()> {
    if let Expr::Binder(_, binders, _) = e {
        for b in binders {
            if b.vars.is_empty() {
                bail!("invariant: Binder must bind at least one VarName");
            }
        }
    }
    Ok(())
}


/// Public connection alias so callers don't need rusqlite directly at the edge.
pub type Db = Connection;