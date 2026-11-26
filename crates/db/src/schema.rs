//! SQL schema — one schema, not a history.
//!
//! The database is a cache: `ingest_corpus` rebuilds every table from the `.qbit`
//! files on each boot, so nothing in it outlives the corpus it came from. A
//! migration chain exists to protect data that survives without the files, which
//! is not this. So there is one `DDL`, and a database that does not match it is
//! dropped and recreated — `SCHEMA_VERSION` is a compatibility stamp, not a
//! sequence, and a file stamped with anything else is simply stale.
//!
//! To change the schema: edit `DDL`, bump `SCHEMA_VERSION`. Existing databases
//! rebuild themselves on the next open and the corpus is re-ingested over them.

use anyhow::Result;
use rusqlite::Connection;

/// Compatibility stamp for `DDL`. Any other value means the file on disk was
/// built by a different schema, and since it is a cache, it is rebuilt.
pub const SCHEMA_VERSION: i64 = 3;

/// Bring the connection up to the current schema.
///
/// The only interesting case is "this database is not the schema I know": it is
/// dropped and recreated. Nothing is lost that a re-ingest cannot restore.
pub fn migrate(conn: &Connection) -> Result<()> {
    if query_version(conn)? == SCHEMA_VERSION && has_tables(conn)? {
        return Ok(());
    }
    drop_all(conn)?;
    conn.execute_batch(DDL)?;
    conn.execute(
        "INSERT INTO schema_version (version) VALUES (?1)",
        rusqlite::params![SCHEMA_VERSION],
    )?;
    Ok(())
}

/// True when the current schema's tables are actually present — a version row can
/// outlive a half-built file, so the stamp alone is not trusted.
fn has_tables(conn: &Connection) -> Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'nodes'",
        [],
        |r| r.get(0),
    )?;
    Ok(n == 1)
}

/// Drop every table (and its indices), leaving an empty database.
fn drop_all(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "foreign_keys", "OFF")?;
    let names: Vec<String> = {
        let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table'")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<std::result::Result<_, _>>()?
    };
    for name in names {
        let quoted = name.replace('"', "\"\"");
        conn.execute_batch(&format!("DROP TABLE IF EXISTS \"{quoted}\";"))?;
    }
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

/// The schema's version stamp, or 0 for a database that has none yet.
fn query_version(conn: &Connection) -> Result<i64> {
    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |r| r.get::<_, i64>(0),
    )
    .or_else(|_| Ok(0))
}

/// The corpus schema (SymbolDB §3, §4): node tables, symbol tables, AST tables,
/// statement/proof tables and calibration/census tables — fully decomposed, no
/// flat JSON IR anywhere.
pub const DDL: &str = r#"
CREATE TABLE schema_version ( version INTEGER PRIMARY KEY );

CREATE TABLE calibration_refs (
  node_id INTEGER NOT NULL,
  referenced_by_id INTEGER NOT NULL,
  PRIMARY KEY (node_id, referenced_by_id),
  FOREIGN KEY (node_id) REFERENCES nodes(id),
  FOREIGN KEY (referenced_by_id) REFERENCES nodes(id)
);

CREATE TABLE calibrations (
  node_id INTEGER PRIMARY KEY,
  symbol TEXT NOT NULL,
  q_units TEXT NOT NULL,
  si_dimensions TEXT NOT NULL,
  conversion_rate TEXT NOT NULL,
  FOREIGN KEY (node_id) REFERENCES nodes(id)
);

CREATE TABLE case_arms (
  step_id INTEGER NOT NULL,
  position INTEGER NOT NULL,
  pattern_id INTEGER NOT NULL,
  body_step_id INTEGER NOT NULL,
  PRIMARY KEY (step_id, position),
  FOREIGN KEY (step_id) REFERENCES proof_steps(id),
  FOREIGN KEY (pattern_id) REFERENCES patterns(id),
  FOREIGN KEY (body_step_id) REFERENCES proof_steps(id)
);

CREATE TABLE census_charter (
  node_id INTEGER PRIMARY KEY,
  charter TEXT NOT NULL CHECK (charter IN ('A','B')),
  FOREIGN KEY (node_id) REFERENCES nodes(id)
);

CREATE TABLE census_consumption (
  node_id INTEGER PRIMARY KEY,
  space TEXT NOT NULL,
  counts TEXT NOT NULL,
  baryonic TEXT NOT NULL,
  FOREIGN KEY (node_id) REFERENCES nodes(id)
);

CREATE TABLE chain_ref_args (
  step_id INTEGER NOT NULL,
  row_position INTEGER NOT NULL,
  ref_position INTEGER NOT NULL,
  arg_position INTEGER NOT NULL,
  arg_expr_id INTEGER NOT NULL,
  PRIMARY KEY (step_id, row_position, ref_position, arg_position),
  FOREIGN KEY (step_id, row_position, ref_position)
    REFERENCES chain_rewrite_refs(step_id, row_position, ref_position),
  FOREIGN KEY (arg_expr_id) REFERENCES exprs(id)
);

CREATE TABLE chain_rewrite_refs (
  step_id INTEGER NOT NULL,
  row_position INTEGER NOT NULL,
  ref_position INTEGER NOT NULL,
  rev INTEGER NOT NULL CHECK (rev IN (0,1)),
  cite_text TEXT NOT NULL,
  PRIMARY KEY (step_id, row_position, ref_position),
  FOREIGN KEY (step_id) REFERENCES proof_steps(id)
);

CREATE TABLE chain_rows (
  step_id INTEGER NOT NULL,
  position INTEGER NOT NULL,
  lhs_expr_id INTEGER,
  rhs_expr_id INTEGER NOT NULL,
  rel TEXT NOT NULL CHECK (rel IN ('Eq','Arrow')),
  just_kind TEXT NOT NULL CHECK (just_kind IN ('Rewrite','Sym','Exact','Rfl')),
  PRIMARY KEY (step_id, position),
  FOREIGN KEY (step_id) REFERENCES proof_steps(id),
  FOREIGN KEY (lhs_expr_id) REFERENCES exprs(id),
  FOREIGN KEY (rhs_expr_id) REFERENCES exprs(id)
);

CREATE TABLE cite_names (
  cite_text TEXT PRIMARY KEY,
  node_id INTEGER NOT NULL,
  line INTEGER,
  FOREIGN KEY (node_id) REFERENCES nodes(id)
);

CREATE TABLE conclude_terms (
  step_id INTEGER PRIMARY KEY,
  term_expr_id INTEGER NOT NULL,
  FOREIGN KEY (step_id) REFERENCES proof_steps(id),
  FOREIGN KEY (term_expr_id) REFERENCES exprs(id)
);

CREATE TABLE expr_binder_vars (
  expr_id INTEGER NOT NULL,
  position INTEGER NOT NULL,
  var_name TEXT NOT NULL,
  ty_expr_id INTEGER NOT NULL,
  PRIMARY KEY (expr_id, position),
  FOREIGN KEY (expr_id) REFERENCES exprs(id),
  FOREIGN KEY (ty_expr_id) REFERENCES exprs(id)
);

CREATE TABLE expr_children (
  parent_id INTEGER NOT NULL,
  position INTEGER NOT NULL,
  child_id INTEGER NOT NULL,
  PRIMARY KEY (parent_id, position),
  FOREIGN KEY (parent_id) REFERENCES exprs(id),
  FOREIGN KEY (child_id) REFERENCES exprs(id)
);

CREATE TABLE expr_matrix_cells (
  parent_expr_id INTEGER NOT NULL,
  row INTEGER NOT NULL,
  col INTEGER NOT NULL,
  cell_expr_id INTEGER NOT NULL,
  PRIMARY KEY (parent_expr_id, row, col),
  FOREIGN KEY (parent_expr_id) REFERENCES exprs(id),
  FOREIGN KEY (cell_expr_id) REFERENCES exprs(id)
);

CREATE TABLE expr_recursor_cases (
  parent_expr_id INTEGER NOT NULL,
  position INTEGER NOT NULL,
  pattern_id INTEGER NOT NULL,
  body_expr_id INTEGER NOT NULL,
  PRIMARY KEY (parent_expr_id, position),
  FOREIGN KEY (parent_expr_id) REFERENCES exprs(id),
  FOREIGN KEY (pattern_id) REFERENCES patterns(id),
  FOREIGN KEY (body_expr_id) REFERENCES exprs(id)
);

CREATE TABLE exprs (
  id INTEGER PRIMARY KEY,
  node_id INTEGER NOT NULL,
  parent_id INTEGER,
  position INTEGER,
  shape TEXT NOT NULL
    CHECK (shape IN ('Sym','Var','Apply','Bracket','Binder','Recursor','Matrix','Vector','Set')),
  symbol_id INTEGER,
  var_name TEXT,
  l_sup_id INTEGER,
  r_sup_id INTEGER,
  l_sub_id INTEGER,
  r_sub_id INTEGER,
  binder_kind TEXT
    CHECK (binder_kind IN ('Forall','Exists','Lambda','Integral','Limit') OR binder_kind IS NULL),
  expr_hash BLOB NOT NULL,
  FOREIGN KEY (node_id) REFERENCES nodes(id),
  FOREIGN KEY (parent_id) REFERENCES exprs(id),
  FOREIGN KEY (symbol_id) REFERENCES symbols(id),
  FOREIGN KEY (l_sup_id) REFERENCES exprs(id),
  FOREIGN KEY (r_sup_id) REFERENCES exprs(id),
  FOREIGN KEY (l_sub_id) REFERENCES exprs(id),
  FOREIGN KEY (r_sub_id) REFERENCES exprs(id),
  CHECK (
    (shape = 'Var' AND symbol_id IS NULL AND var_name IS NOT NULL)
    OR (shape = 'Binder' AND symbol_id IS NULL AND var_name IS NULL)
    OR (shape IN ('Matrix','Vector','Set','Recursor') AND symbol_id IS NULL AND var_name IS NULL)
    OR (shape NOT IN ('Var','Binder','Matrix','Vector','Set','Recursor')
        AND symbol_id IS NOT NULL AND var_name IS NULL)
  ),
  CHECK ((shape = 'Binder') = (binder_kind IS NOT NULL))
);

CREATE TABLE fix_vars (
  step_id INTEGER NOT NULL,
  position INTEGER NOT NULL,
  var_name TEXT NOT NULL,
  PRIMARY KEY (step_id, position),
  FOREIGN KEY (step_id) REFERENCES proof_steps(id)
);

CREATE TABLE form_bodies (
  symbol_id INTEGER PRIMARY KEY,
  form_kind TEXT NOT NULL
    CHECK (form_kind IN ('MacroExpr','Recursive','Formation','Bracket')),
  body_expr_id INTEGER,
  -- A FORMATION's constructors, as JSON: [{"constructor": <symbol id>, "args": [<symbol id>...]}].
  -- Without it the ctor list died with the .qbit text, so a formation loaded as CONTEXT
  -- (every axiom/theorem module) looked like a plain definition and emitted no inductive.
  ctors TEXT,
  open_delim TEXT,
  close_delim TEXT,
  FOREIGN KEY (symbol_id) REFERENCES symbols(id),
  FOREIGN KEY (body_expr_id) REFERENCES exprs(id),
  CHECK (
    (form_kind = 'Bracket' AND open_delim IS NOT NULL AND close_delim IS NOT NULL
                          AND body_expr_id IS NULL)
    OR (form_kind = 'Formation' AND open_delim IS NULL AND body_expr_id IS NULL)
    OR (form_kind IN ('MacroExpr','Recursive') AND open_delim IS NULL AND body_expr_id IS NOT NULL)
  )
);

CREATE TABLE form_recursive_cases (
  symbol_id INTEGER NOT NULL,
  position INTEGER NOT NULL,
  pattern_id INTEGER NOT NULL,
  body_expr_id INTEGER NOT NULL,
  PRIMARY KEY (symbol_id, position),
  FOREIGN KEY (symbol_id) REFERENCES symbols(id),
  FOREIGN KEY (pattern_id) REFERENCES patterns(id),
  FOREIGN KEY (body_expr_id) REFERENCES exprs(id)
);

CREATE TABLE induction_cases (
  step_id INTEGER NOT NULL,
  position INTEGER NOT NULL,
  pattern_id INTEGER,
  body_step_id INTEGER NOT NULL,
  PRIMARY KEY (step_id, position),
  FOREIGN KEY (step_id) REFERENCES proof_steps(id),
  FOREIGN KEY (pattern_id) REFERENCES patterns(id),
  FOREIGN KEY (body_step_id) REFERENCES proof_steps(id)
);

CREATE TABLE lemma_lhs_patterns (
  node_id INTEGER NOT NULL,
  lemma_line INTEGER NOT NULL,
  lhs_hash BLOB NOT NULL,
  rhs_hash BLOB NOT NULL,
  direction TEXT NOT NULL CHECK (direction IN ('fwd','rev')),
  pattern_expr_id INTEGER NOT NULL,
  PRIMARY KEY (node_id, lemma_line, direction),
  FOREIGN KEY (node_id) REFERENCES nodes(id),
  FOREIGN KEY (pattern_expr_id) REFERENCES exprs(id)
);

CREATE TABLE node_derived (
  source_node_id INTEGER NOT NULL,
  target_node_id INTEGER NOT NULL,
  reason TEXT NOT NULL CHECK (reason IN ('symbol','citation')),
  PRIMARY KEY (source_node_id, target_node_id, reason),
  FOREIGN KEY (source_node_id) REFERENCES nodes(id),
  FOREIGN KEY (target_node_id) REFERENCES nodes(id),
  CHECK (source_node_id <> target_node_id)
);

CREATE TABLE node_req (
  source_node_id INTEGER NOT NULL,
  target_node_id INTEGER NOT NULL,
  position INTEGER NOT NULL,
  PRIMARY KEY (source_node_id, position),
  FOREIGN KEY (source_node_id) REFERENCES nodes(id),
  FOREIGN KEY (target_node_id) REFERENCES nodes(id),
  CHECK (source_node_id <> target_node_id)
);

CREATE TABLE node_statements (
  node_id INTEGER NOT NULL,
  line INTEGER NOT NULL,
  expr_id INTEGER NOT NULL,
  PRIMARY KEY (node_id, line),
  FOREIGN KEY (node_id) REFERENCES nodes(id),
  FOREIGN KEY (expr_id) REFERENCES exprs(id)
);

CREATE TABLE "nodes" (
               id INTEGER PRIMARY KEY,
               node_type TEXT NOT NULL CHECK (node_type IN
                 ('Axiom','Definition','Theorem','Census','Calibration')),
               era TEXT NOT NULL,
               strata INTEGER NOT NULL CHECK (strata BETWEEN 0 AND 15),
               name TEXT NOT NULL,
               filename TEXT NOT NULL,
               tag TEXT CHECK (tag IN ('Phys','Math') OR tag IS NULL),
               falsify_expr_id INTEGER,
               node_hash BLOB NOT NULL,
               version INTEGER NOT NULL DEFAULT 1,
               godel INTEGER, authored INTEGER,
               UNIQUE (era, strata, filename),
               FOREIGN KEY (falsify_expr_id) REFERENCES exprs(id)
             );

CREATE TABLE note_bodies (
  note_step_id INTEGER NOT NULL,
  position INTEGER NOT NULL,
  child_step_id INTEGER NOT NULL,
  PRIMARY KEY (note_step_id, position),
  FOREIGN KEY (note_step_id) REFERENCES proof_steps(id),
  FOREIGN KEY (child_step_id) REFERENCES proof_steps(id)
);

CREATE TABLE pattern_binders (
  pattern_id INTEGER NOT NULL,
  position INTEGER NOT NULL,
  var_name TEXT NOT NULL,
  PRIMARY KEY (pattern_id, position),
  FOREIGN KEY (pattern_id) REFERENCES patterns(id)
);

CREATE TABLE patterns (
  id INTEGER PRIMARY KEY,
  parent_id INTEGER,
  position INTEGER,
  constructor_symbol_id INTEGER NOT NULL,
  FOREIGN KEY (parent_id) REFERENCES patterns(id),
  FOREIGN KEY (constructor_symbol_id) REFERENCES symbols(id)
);

CREATE TABLE proof_steps (
  id INTEGER PRIMARY KEY,
  node_id INTEGER NOT NULL,
  parent_step_id INTEGER,
  position INTEGER NOT NULL,
  step_kind TEXT NOT NULL
    CHECK (step_kind IN ('Chain','Conclude','Case','Induction','Recursor','Note','Rewrite','Fix','Discriminate')),
  scrutinee_expr_id INTEGER,
  var_name TEXT,
  note_name TEXT,
  note_prop_expr_id INTEGER,
  FOREIGN KEY (node_id) REFERENCES nodes(id),
  FOREIGN KEY (parent_step_id) REFERENCES proof_steps(id),
  FOREIGN KEY (scrutinee_expr_id) REFERENCES exprs(id),
  FOREIGN KEY (note_prop_expr_id) REFERENCES exprs(id)
);

CREATE TABLE recursor_proof_cases (
  step_id INTEGER NOT NULL,
  position INTEGER NOT NULL,
  pattern_id INTEGER NOT NULL,
  body_step_id INTEGER NOT NULL,
  PRIMARY KEY (step_id, position),
  FOREIGN KEY (step_id) REFERENCES proof_steps(id),
  FOREIGN KEY (pattern_id) REFERENCES patterns(id),
  FOREIGN KEY (body_step_id) REFERENCES proof_steps(id)
);

CREATE TABLE rewrite_ref_args (
  step_id INTEGER NOT NULL,
  ref_position INTEGER NOT NULL,
  arg_position INTEGER NOT NULL,
  arg_expr_id INTEGER NOT NULL,
  PRIMARY KEY (step_id, ref_position, arg_position),
  FOREIGN KEY (step_id, ref_position) REFERENCES rewrite_refs(step_id, position),
  FOREIGN KEY (arg_expr_id) REFERENCES exprs(id)
);

CREATE TABLE rewrite_refs (
  step_id INTEGER NOT NULL,
  position INTEGER NOT NULL,
  rev INTEGER NOT NULL,
  cite_text TEXT NOT NULL,
  PRIMARY KEY (step_id, position),
  FOREIGN KEY (step_id) REFERENCES proof_steps(id)
);

CREATE TABLE symbol_elab_rules (
  symbol_id INTEGER PRIMARY KEY,
  rule TEXT NOT NULL
    CHECK (rule IN ('Identity','Apply','CurriedApply','Binder','Matrix','Vector','Set','Wrap')),
  apply_form TEXT
    CHECK (apply_form IN ('Curried','Uncurried','Dependent') OR apply_form IS NULL),
  binder_kind TEXT
    CHECK (binder_kind IN ('Forall','Exists','Lambda','Integral','Limit') OR binder_kind IS NULL),
  FOREIGN KEY (symbol_id) REFERENCES symbols(id),
  CHECK ((rule = 'CurriedApply') = (apply_form IS NOT NULL))
);

CREATE TABLE symbol_mod_roles (
  symbol_id INTEGER NOT NULL,
  slot TEXT NOT NULL CHECK (slot IN ('l_sup','r_sup','l_sub','r_sub')),
  role TEXT NOT NULL CHECK (role IN (
    'Exponent','Index','Annotation','DerivativeOrder',
    'Transpose','Inverse','BinderVar','Bound','SuperscriptMark'
  )),
  PRIMARY KEY (symbol_id, slot, role),
  FOREIGN KEY (symbol_id) REFERENCES symbols(id)
);

CREATE TABLE symbol_usage (
  symbol_id INTEGER NOT NULL,
  node_id INTEGER NOT NULL,
  occurrence_count INTEGER NOT NULL,
  PRIMARY KEY (symbol_id, node_id),
  FOREIGN KEY (symbol_id) REFERENCES symbols(id),
  FOREIGN KEY (node_id) REFERENCES nodes(id)
);

CREATE TABLE symbols (
  id INTEGER PRIMARY KEY,
  scope TEXT NOT NULL,
  tex TEXT NOT NULL,
  display TEXT NOT NULL,
  lean_name TEXT NOT NULL,
  trigger TEXT,
  kind TEXT NOT NULL
    CHECK (kind IN ('formation','ctor','type','notation','macro','primitive','bracketfn')),
  role TEXT CHECK (role IN ('primary','ctor','notation-atom','extra','prefix','infix','collector','connective') OR role IS NULL),
  decl_order INTEGER,
  precedence INTEGER,
  home_node INTEGER NOT NULL,
  symbol_hash BLOB NOT NULL, facets INTEGER NOT NULL DEFAULT 0,
  UNIQUE (scope, tex),
  FOREIGN KEY (home_node) REFERENCES nodes(id)
);

CREATE INDEX idx_expr_children_child ON expr_children(child_id);

CREATE INDEX idx_exprs_hash ON exprs(expr_hash);

CREATE INDEX idx_exprs_node ON exprs(node_id);

CREATE INDEX idx_exprs_parent ON exprs(parent_id) WHERE parent_id IS NOT NULL;

CREATE INDEX idx_exprs_symbol ON exprs(symbol_id) WHERE symbol_id IS NOT NULL;

CREATE INDEX idx_lemma_lhs_hash ON lemma_lhs_patterns(lhs_hash);

CREATE INDEX idx_node_derived_source ON node_derived(source_node_id);

CREATE INDEX idx_node_req_target ON node_req(target_node_id);

CREATE INDEX idx_proof_steps_node ON proof_steps(node_id);

CREATE INDEX idx_proof_steps_parent ON proof_steps(parent_step_id) WHERE parent_step_id IS NOT NULL;

CREATE INDEX idx_symbol_mod_roles_slot ON symbol_mod_roles(slot);

CREATE INDEX idx_symbol_usage_symbol ON symbol_usage(symbol_id);

CREATE INDEX idx_symbols_home ON symbols(home_node);

CREATE INDEX idx_symbols_scope_kind ON symbols(scope, kind);

CREATE INDEX idx_symbols_tex ON symbols(tex);
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_schema_creates_every_table() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 36, "35 corpus tables plus schema_version");
        let idx: i64 = conn
            .query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND sql IS NOT NULL", [], |r| r.get(0))
            .unwrap();
        assert_eq!(idx, 15);
        assert_eq!(query_version(&conn).unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn a_stale_database_is_rebuilt_not_migrated() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO nodes (id, node_type, era, strata, name, filename, node_hash)
             VALUES (1, 'Axiom', 'Era0', 0, 'Old', 'A1.qbit', x'00')",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM schema_version", []).unwrap();
        conn.execute("INSERT INTO schema_version (version) VALUES (7)", []).unwrap();
        migrate(&conn).unwrap();
        assert_eq!(query_version(&conn).unwrap(), SCHEMA_VERSION);
        let rows: i64 = conn.query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 0, "a stale cache is rebuilt empty, not migrated");
    }

    #[test]
    fn a_stamped_but_incomplete_database_is_rebuilt() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn.execute("DROP TABLE nodes", []).unwrap();
        migrate(&conn).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'nodes'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "the missing table comes back");
    }

    #[test]
    fn the_authored_number_is_carried() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO nodes (id, node_type, era, strata, name, filename, node_hash, godel, authored)
             VALUES (1, 'Axiom', 'Era0', 0, 'Counting', 'A4_Counting.qbit', x'00', 9, 4)",
            [],
        )
        .unwrap();
        let row: (i64, Option<i64>) = conn
            .query_row("SELECT id, authored FROM nodes WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(row, (1, Some(4)));
    }
}
