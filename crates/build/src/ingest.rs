//! Corpus ingestion: parse `project/**/*.qbit` and (re)populate the corpus DB.
//!
//! Parses every `.qbit` file under a project root and writes its nodes,
//! declared symbols, statement expressions, and `req` edges into the SQLite
//! `qubitick-db` schema (SymbolDB §5, §12). Each node is assigned a **Godel
//! code** equal to its registration ordinal in the corpus ledger (A4
//! `\Vert x \Vert`, D_BT.32 `\ulcorner x \urcorner`), so `/api/stats` and
//! `/api/symbols` can enumerate the corpus arithmetized.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use qubitick_db::hash::hash_symbol;
use qubitick_db::write::write_expr;
use qubitick_model::{Node, NodeKind};
use qubitick_model::corpus::is_operator_punct;

use rusqlite::Connection;

/// Outcome counters of one corpus ingestion.
#[derive(Debug, Clone, Default)]
pub struct IngestReport {
    pub files: usize,
    pub nodes: usize,
    pub symbols: usize,
    pub statements: usize,
    pub req_edges: usize,
    /// Two nodes of one kind, in one scope, claiming the same authored number or
    /// the same name. Both make a reference ambiguous — `[A4.2]` cannot say which
    /// axiom it means, and a name lookup cannot say which node — so they are
    /// reported rather than resolved silently. See `IngestReport::duplicates`.
    pub duplicates: Vec<Duplicate>,
}

/// What a corpus node was claimed twice by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claimed {
    /// The number in the header (`A4. Counting` -> 4): a citation names it, and
    /// nothing else does — so it must be unique across the whole ERA, because a
    /// stratum can cite the strata before it.
    Number,
    /// The node's name (`Counting`): a name lookup names it.
    Name,
}

impl Claimed {
    /// How the violation reads in a report.
    pub fn label(self) -> &'static str {
        match self {
            Claimed::Number => "number",
            Claimed::Name => "name",
        }
    }
}

/// One ambiguity in the corpus: a number or a name claimed by more than one node
/// of the same kind in the same scope.
#[derive(Debug, Clone)]
pub struct Duplicate {
    pub claimed: Claimed,
    /// The node kind (`Axiom`, `Definition`, `Theorem`, …).
    pub node_kind: String,
    /// Where the claim was made: the era for a number (citations are era-wide),
    /// the scope for a name (a name is read where it is written).
    pub scope: String,
    /// The number or the name that was claimed twice.
    pub value: String,
    /// Every file claiming it, in ledger order (the first one wins a lookup).
    pub files: Vec<String>,
}

impl Duplicate {
    /// `two Theorem nodes in Era0/Strata0 claim the name 'IntNegOne': a, b`
    pub fn message(&self) -> String {
        format!(
            "two {} nodes in {} claim the {} '{}': {}",
            self.node_kind,
            self.scope,
            self.claimed.label(),
            self.value,
            self.files.join(", ")
        )
    }
}

impl IngestReport {
    /// A corpus whose references are unambiguous.
    pub fn is_clean(&self) -> bool {
        self.duplicates.is_empty()
    }
}

/// A single corpus file's parse summary, in canonical ledger order.
struct Entry {
    path: PathBuf,
    kind: NodeKind,
    name: String,
    /// The number the author wrote in the header (`A4. Counting` -> 4, `D.BT.43`
    /// -> 43). This, not the ledger ordinal, is what a citation names.
    authored: u32,
    /// (glyph tex, SymbolKind as_str) — only present for Definitions.
    glyph: Option<(String, String)>,
    /// Distinct LaTeX control words used in the file.
    words: Vec<String>,
}

/// Parse every `.qbit` under `project_root` and (re)populate the corpus DB.
///
/// The DB is wiped of corpus rows and reseeded so a fresh server boot always
/// mirrors the on-disk corpus. Deterministic: sorted file order, node ids and
/// Godel codes 1..=N, symbols ordered by tex.
///
/// THERE IS NO SCOPE ARGUMENT. Every node's scope comes from its own path
/// (`pipeline::scope_of_path`), so a caller that passed one was describing a decision it did not
/// make — the parameter sat there unused, and reading `ingest_corpus(&mut conn, "Era0/Strata0",
/// …)` at a call site suggested the whole corpus was being ingested into one stratum.
/// One claim a node's text makes on a symbol: the glyph it DECLARES, or a control word it USES.
///
/// Kept as one shape so the corpus ingest and the save path (`ingest_file_symbols`, below) cannot
/// disagree about what a symbol row is — the derivation lives in `claims_of` and nowhere else.
struct SymClaim {
    /// The era the row's dedup key uses: one tex is one symbol within an era, shared by its strata.
    era: String,
    tex: String,
    lean: String,
    kind: String,
    /// `symbols.home_node` — the authoring node's ledger id.
    home: i64,
    /// The row's `scope` column: `{era}/Strata{strata}`, from the file's own path.
    scope: String,
}

/// What one file claims, in the order ingest takes it: declared glyph first, then every control word
/// the text uses (a use-only claim is what makes an undeclared command a `notation`).
fn claims_of(e: &Entry, node_id: i64, project_root: &Path) -> Vec<SymClaim> {
    let (era, strata) = era_strata_of(project_root, &e.path);
    let entry_scope = format!("{era}/Strata{strata}");
    let mut out = Vec::with_capacity(e.words.len() + 1);
    // A declarative glyph line is an EXPRESSION (`a \leftrightarrow b`), not a bare glyph, so its head
    // `\command` is the declared symbol. Registering the whole line would create a bogus symbol named
    // after the expression (and `control_words` below already registers every `\command`).
    if let Some((line, kind)) = &e.glyph
        && let Some(tex) = head_control_word(line)
    {
        out.push(SymClaim {
            era: era.clone(),
            lean: lean_name(&tex),
            kind: kind.clone(),
            home: node_id,
            scope: entry_scope.clone(),
            tex,
        });
    }
    for w in &e.words {
        out.push(SymClaim {
            era: era.clone(),
            lean: lean_name(w),
            kind: "notation".to_string(),
            home: node_id,
            scope: entry_scope.clone(),
            tex: w.clone(),
        });
    }
    out
}

/// What a save put in the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileSymbols {
    /// Rows written (a declared glyph upserts; a use-only word inserts only when the tex is new).
    pub written: usize,
    /// The file's node is in the ledger, so its symbols have a `home_node` to hang on.
    pub in_ledger: bool,
}

/**
 * THE SYMBOLS ONE SAVED FILE PUTS IN THE DB.
 *
 * A save happens while someone types, and a whole-corpus ingest is the wrong shape for it: the ledger
 * walk re-parses all 330 files and renumbers every node (`godel` IS the ledger ordinal — measured at
 * ~10 s in a debug build, and a rebuild is `POST /api/build`'s job anyway). What a save can honestly do
 * is inject what THIS file knows, so the engine can parse the notation immediately:
 *
 *   · the glyph a definition DECLARES, upserted with its kind (`Type:` only -> `primitive`, a `Sema:`
 *     -> `notation`/`formation`, crates/parse/src/node.rs) and its lean name — saving is how an author
 *     edits their own definition, so the row has to follow;
 *   · every control word the file USES, insert-only: ingest gives a DECLARED glyph precedence over a
 *     use-only claim, and one file cannot see the corpus, but it must not demote a symbol another file
 *     has already declared.
 *
 * `symbols.home_node` is NOT NULL and foreign-keyed to `nodes`, so a file the engine has not ingested
 * yet has nowhere to hang its symbols: it reports `in_ledger: false` and writes nothing. Guessing a node
 * id would attach an author's notation to someone else's node; the remedy for that case is the build
 * (which is the "rebuild project on save" mode), not a guess.
 *
 * A later ingest DELETEs every table before it re-derives them (see the `DELETE` sweep above), so what
 * this writes is REPLACED by the build rather than merged with it — which is why running the two in
 * either order cannot collide or double-count.
 */
pub fn ingest_file_symbols(
    conn: &Connection,
    project_root: &Path,
    path: &Path,
) -> Result<FileSymbols> {
    /* The node row is found by the path ingest stored (`nodes.filename`, written as
       `path.display().to_string()`), so this is the same file — not a second way to name it. */
    let key = path.display().to_string();
    let home: Option<i64> = conn
        .query_row("SELECT id FROM nodes WHERE filename = ?1", [&key], |r| r.get(0))
        .ok();
    let src = std::fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))?;
    let empty = qubitick_ctx::Ctx::from_rows("", vec![]);
    let nodes = qubitick_parse::parse_content(&empty, &src)
        .with_context(|| format!("parse {}", path.display()))?;
    let node = nodes.first().context("empty parse result")?;
    let glyph = match node {
        Node::Definition(d) => Some((first_glyph_line(&src), d.d_block.kind.as_str().to_string())),
        _ => None,
    };
    let entry = Entry {
        path: path.to_path_buf(),
        kind: node.base().kind,
        name: node.base().name.clone(),
        authored: node.base().id,
        glyph,
        words: control_words(&src),
    };
    let Some(home) = home else {
        return Ok(FileSymbols { written: 0, in_ledger: false });
    };

    let mut written = 0usize;
    for (decl, claim) in claims_of(&entry, home, project_root).into_iter().enumerate() {
        let sh = hash_symbol(&claim.scope, &claim.tex, &claim.kind);
        let declared = decl == 0 && entry.glyph.is_some();
        let sql = if declared {
            "INSERT INTO symbols
               (scope, tex, display, lean_name, trigger, kind, role,
                decl_order, precedence, home_node, symbol_hash, facets)
             VALUES (?1,?2,?3,?4,NULL,?5,NULL,?6,NULL,?7,?8,?9)
             ON CONFLICT(scope, tex) DO UPDATE SET
               lean_name = excluded.lean_name, kind = excluded.kind,
               home_node = excluded.home_node, symbol_hash = excluded.symbol_hash,
               facets = excluded.facets"
        } else {
            "INSERT INTO symbols
               (scope, tex, display, lean_name, trigger, kind, role,
                decl_order, precedence, home_node, symbol_hash, facets)
             VALUES (?1,?2,?3,?4,NULL,?5,NULL,?6,NULL,?7,?8,?9)
             ON CONFLICT(scope, tex) DO NOTHING"
        };
        written += conn.execute(
            sql,
            rusqlite::params![
                claim.scope,
                claim.tex,
                claim.tex,
                claim.lean,
                claim.kind,
                decl as i64,
                claim.home,
                sh.to_le_bytes().to_vec(),
                0i64
            ],
        )?;
    }
    Ok(FileSymbols { written, in_ledger: true })
}

/// The per-file contexts and symbol-hash maps, rebuilt after the metadata pass.
fn rebuild_entry_ctxs(
    conn: &qubitick_db::Db,
    entries: &[Entry],
    project_root: &Path,
) -> Result<Vec<(qubitick_ctx::Ctx, HashMap<u32, u128>)>> {
    let mut out = Vec::with_capacity(entries.len());
    for e in entries {
        let (er, st) = era_strata_of(project_root, &e.path);
        let scope = format!("{er}/Strata{st}");
        let c = qubitick_ctx::build_ctx(conn, &scope)?;
        let sh: HashMap<u32, u128> = c
            .symbols()
            .map(|r| (r.id as u32, hash_symbol(&r.scope, &r.tex, &r.kind)))
            .collect();
        out.push((c, sh));
    }
    Ok(out)
}

pub fn ingest_corpus(conn: &mut Connection, project_root: &Path) -> Result<IngestReport> {
    let files = walk_qbit(project_root);

    // ---- Phase A: canonical ledger order + declared/used symbols ------------
    let empty = qubitick_ctx::Ctx::from_rows("", vec![]);
    let mut entries: Vec<Entry> = Vec::new();
    for path in &files {
        let src = std::fs::read_to_string(path)
            .with_context(|| format!("read {}", path.display()))?;
        let nodes = qubitick_parse::parse_content(&empty, &src)
            .with_context(|| format!("parse {}", path.display()))?;
        let node = nodes.first().context("empty parse result")?;
        let glyph = match node {
            Node::Definition(d) => {
                Some((first_glyph_line(&src), d.d_block.kind.as_str().to_string()))
            }
            _ => None,
        };
        entries.push(Entry {
            path: path.clone(),
            kind: node.base().kind,
            name: node.base().name.clone(),
            authored: node.base().id,
            glyph,
            words: control_words(&src),
        });
    }

    // Assign node ids 1..=N; a node's Godel code IS its ledger ordinal.
    // Collect symbols (dedup by tex): declared glyphs take precedence over
    // use-only control words for the kind.
    let mut syms: BTreeMap<(String, String), SymClaim> = BTreeMap::new();
    for (i, e) in entries.iter().enumerate() {
        let node_id = (i + 1) as i64;
        // in the order the file states them: its declared glyph, then the words it uses, so
        // `or_insert` gives a declaration precedence over a use of the same tex
        for claim in claims_of(e, node_id, project_root) {
            syms.entry((claim.era.clone(), claim.tex.clone())).or_insert(claim);
        }
    }

    // A citation names the number the AUTHOR wrote and a lookup names the node's
    // NAME, so neither may be claimed twice by one kind in one scope: the second
    // node would be unreachable and the first would answer for it. Collect both
    // violations (in ledger order, so the first claimant is the one a lookup
    // finds) instead of resolving them silently.
    let mut duplicates: Vec<Duplicate> = Vec::new();
    for claimed in [Claimed::Number, Claimed::Name] {
        let mut seen: std::collections::HashMap<(String, &'static str, String), String> =
            std::collections::HashMap::new();
        for e in &entries {
            let (era, strata) = era_strata_of(project_root, &e.path);
            // A NUMBER is a citation's whole identity (`[A4.2]` names nothing else),
            // so it must be unique across the era: a stratum can cite the strata
            // before it, and two strata both numbering from 1 would make `[T.2]`
            // mean whichever one the resolver preferred. A NAME is a label, and a
            // label only has to be unique where it is read — its own scope.
            let scope = match claimed {
                Claimed::Number => era.clone(),
                Claimed::Name => format!("{era}/Strata{strata}"),
            };
            let value = match claimed {
                Claimed::Number if e.authored == 0 => continue,
                Claimed::Number => e.authored.to_string(),
                Claimed::Name => e.name.clone(),
            };
            let key = (scope.clone(), e.kind.as_str(), value.clone());
            let file = e.path.display().to_string();
            match seen.get_mut(&key) {
                Some(first) => {
                    if let Some(d) = duplicates.iter_mut().find(|d| {
                        d.claimed == claimed
                            && d.scope == scope
                            && d.value == value
                            && d.node_kind == e.kind.as_str()
                    }) {
                        d.files.push(file);
                    } else {
                        duplicates.push(Duplicate {
                            claimed,
                            node_kind: e.kind.as_str().to_string(),
                            scope,
                            value,
                            files: vec![first.clone(), file],
                        });
                    }
                }
                None => {
                    seen.insert(key, file);
                }
            }
        }
    }

    // ---- Phase B: truncate corpus rows; insert nodes + symbols --------------
    conn.pragma_update(None, "foreign_keys", "OFF")?;
    let tx = conn.unchecked_transaction()?;
    for table in user_table_names(conn)? {
        if table == "schema_version" {
            continue;
        }
        tx.execute(&format!("DELETE FROM \"{table}\""), [])?;
    }
    for (i, e) in entries.iter().enumerate() {
        let node_id = (i + 1) as i64;
        let node_hash = hash_symbol("", &e.name, e.kind.as_str());
        let (era, strata) = era_strata_of(project_root, &e.path);
        tx.execute(
            "INSERT INTO nodes
               (id, node_type, era, strata, name, filename, node_hash, godel, authored)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            rusqlite::params![
                node_id,
                e.kind.as_str(),
                era,
                strata,
                e.name,
                e.path.display().to_string(),
                node_hash.to_le_bytes().to_vec(),
                node_id,
                // 0 means the header carried no number at all; store NULL so the
                // readers fall back to the ledger ordinal rather than to "zero".
                if e.authored == 0 {
                    None
                } else {
                    Some(e.authored as i64)
                }
            ],
        )?;
    }
    for (decl, (_key, c)) in syms.iter().enumerate() {
        let sh = hash_symbol(&c.scope, &c.tex, &c.kind);
        tx.execute(
            "INSERT INTO symbols
               (scope, tex, display, lean_name, trigger, kind, role,
                decl_order, precedence, home_node, symbol_hash, facets)
             VALUES (?1,?2,?3,?4,NULL,?5,NULL,?6,NULL,?7,?8,?9)",
            rusqlite::params![
                c.scope,
                c.tex,
                c.tex,
                c.lean,
                c.kind,
                decl as i64,
                c.home,
                sh.to_le_bytes().to_vec(),
                0i64
            ],
        )?;
    }
    // Stamp the GENERATED sort constructors that are used prefix-style (`^+ x`,
    // Reserve the model-level equality symbol (`model::EQ_SYM`) so structured
    // `=` expressions can be stored (symbols.id FK on exprs.symbol_id). It is
    // GLOBAL scope so every era's statements can use it.
    let eq_hash = hash_symbol("", "=", "notation");
    tx.execute(
        "INSERT INTO symbols
            (id, scope, tex, display, lean_name, trigger, kind, role,
             decl_order, precedence, home_node, symbol_hash)
         VALUES (?1,?2,'=','=','Eq',NULL,'notation',NULL,?3,NULL,?4,?5)
         ON CONFLICT(id) DO NOTHING",
        rusqlite::params![
            qubitick_model::EQ_SYM,
            "",
            0i64,
            1i64,
            eq_hash.to_le_bytes().to_vec()
        ],
    )?;
    tx.commit()?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    // The per-file contexts the two passes read: rebuilt between them, because a
    // declaration is classified against the declarations already written.
    let mut entry_ctxs: Vec<(qubitick_ctx::Ctx, HashMap<u32, u128>)> =
        rebuild_entry_ctxs(conn, &entries, project_root)?;

    // ---- Phase C: re-parse with the full context; write exprs + req ---------
    // Each node is re-parsed with a symbol context for its OWN scope (own + global
    // + ancestor + within-era earlier strata), so a file can cite symbols declared
    // in a prior stratum/era. Hashes are scope-independent, so reconstruction at
    // read time agrees with these write-time hashes regardless of build scope.
    // ---- Pass 1 of Phase C: persist D-block METADATA (elab/binder_kind/
    // mod_roles/role) from every Definition. This must run BEFORE the parse
    // contexts are (re)built, so the parser's dispatch — which reads the DB —
    // sees the metadata for the very corpus it is ingesting. Without this the
    // contexts were built before the metadata existed and dispatch silently fell
    // back to structural heuristics.
    {
        // THE CHARTER'S MASTER PRECEDENCE TABLE (D10). Every declared glyph the
        // charter orders gets its level here; a glyph the table does not name keeps
        // the shared loosest level, which is what a corpus without a table has.
        let precedence: Vec<Vec<String>> = crate::canon::read(project_root)?
            .map(|c| c.precedence)
            .unwrap_or_default();
        // TWO ROUNDS, because a declaration is classified against OTHER declarations:
        // a formation is recognisable only when the connective separating its
        // constructors is already declared, and that connective may live in another
        // file. Round one writes every node's roles and facets; round two re-parses
        // with them in the context, which is when a formation becomes a formation and
        // its constructors become ctors.
        for round in 0..2 {
            if round > 0 {
                entry_ctxs = rebuild_entry_ctxs(conn, &entries, project_root)?;
            }
            let tx = conn.unchecked_transaction()?;
            for (i, e) in entries.iter().enumerate() {
                let (ctx, _h) = &entry_ctxs[i];
                let src = std::fs::read_to_string(&e.path)?;
                let nodes = qubitick_parse::parse_content(ctx, &src)?;
                if let Some(Node::Definition(d)) = nodes.first() {
                    persist_definition_metadata(&tx, ctx, &src, d, &precedence)?;
                }
            }
            tx.commit()?;
        }
    }
    // Rebuild the contexts so they carry the freshly persisted D-block metadata.
    entry_ctxs = rebuild_entry_ctxs(conn, &entries, project_root)?;

    // ---- Pass 2: DECLARATION FACTS (the declaring node owns its glyph) ------
    //
    // Two facts the earlier passes cannot state, because both are cross-file:
    // - a glyph's HOME is the node that DECLARES it, not the first node that used it
    //   (Phase B registers whichever use came first, which made the declaring node
    //   unattributable);
    // - a formation's constructors are ctors of that formation — including a ctor
    //   that also has a declaration of its own (`\tau_0` with `Type: \tau`), whose
    //   node would otherwise leave it looking like a primitive.
    {
        let tx = conn.unchecked_transaction()?;
        for (i, e) in entries.iter().enumerate() {
            let node_id = (i + 1) as i64;
            let (ctx, _h) = &entry_ctxs[i];
            let src = std::fs::read_to_string(&e.path)?;
            let nodes = qubitick_parse::parse_content(ctx, &src)?;
            let Some(Node::Definition(d)) = nodes.first() else {
                continue;
            };
            if d.symbol.primary == 0 {
                continue;
            }
            let sid = d.symbol.primary as i64;
            let Some(row) = ctx.by_id(sid) else { continue };
            let hash = hash_symbol(&row.scope, &row.tex, d.d_block.kind.as_str());
            tx.execute(
                "UPDATE symbols SET kind = ?1, home_node = ?2, symbol_hash = ?3 WHERE id = ?4",
                rusqlite::params![
                    d.d_block.kind.as_str(),
                    node_id,
                    hash.to_le_bytes().to_vec(),
                    sid
                ],
            )?;
            // A BOUND glyph takes the Lean name the declaration binds it to, so every
            // use renders that name — the ground's, not a sanitised spelling of the
            // corpus's. The symbol hash stays as it is: it is derived from the scope,
            // the tex and the kind, never from the name.
            if let Some(bound) = &d.d_block.lean_binding {
                tx.execute(
                    "UPDATE symbols SET lean_name = ?1 WHERE id = ?2",
                    rusqlite::params![bound, sid],
                )?;
            }
        }
        // THE CONSTRUCTORS LAST: a ctor may have a declaration node of its own
        // (`\tau_0` with `Type: \tau`), and that node's kind was just written above.
        // Reading the formations again, after every glyph's own declaration has been
        // recorded, is what makes "this glyph is a constructor of that formation" win.
        for (i, e) in entries.iter().enumerate() {
            let node_id = (i + 1) as i64;
            let (ctx, _h) = &entry_ctxs[i];
            let src = std::fs::read_to_string(&e.path)?;
            let nodes = qubitick_parse::parse_content(ctx, &src)?;
            let Some(Node::Definition(d)) = nodes.first() else {
                continue;
            };
            let Some(qubitick_model::FormBody::Formation(ctors)) = &d.d_block.form else {
                continue;
            };
            for c in ctors {
                let cid = c.constructor.primary as i64;
                if cid == 0 {
                    continue;
                }
                let chash = tx
                    .query_row("SELECT scope, tex FROM symbols WHERE id = ?1", [cid], |r| {
                        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                    })
                    .ok()
                    .map(|(scope, tex)| hash_symbol(&scope, &tex, "ctor"));
                tx.execute(
                    "UPDATE symbols SET kind = 'ctor', home_node = ?1, symbol_hash = ?2 WHERE id = ?3",
                    rusqlite::params![node_id, chash.unwrap_or(0).to_le_bytes().to_vec(), cid],
                )?;
            }
        }
        tx.commit()?;
    }
    // The facts above change kinds and hashes, so the contexts are rebuilt once more
    // before any expr is written.
    entry_ctxs = rebuild_entry_ctxs(conn, &entries, project_root)?;


    let mut report = IngestReport {
        files: entries.len(),
        nodes: entries.len(),
        symbols: syms.len(),
        statements: 0,
        req_edges: 0,
        duplicates,
    };

    let tx = conn.unchecked_transaction()?;
    for (i, e) in entries.iter().enumerate() {
        let node_id = (i + 1) as i64;
        let (ctx, sym_hashes) = &entry_ctxs[i];
        let h = |id: u32| sym_hashes.get(&id).copied().unwrap_or(0);
        let src = std::fs::read_to_string(&e.path)?;
        let nodes = qubitick_parse::parse_content(ctx, &src)?;
        match &nodes[0] {
            Node::Axiom(a) => {
                for (line, stmt) in a.statements.iter().enumerate() {
                    let eid = write_expr(&tx, stmt, node_id, None, None, &h)?;
                    tx.execute(
                        "INSERT INTO node_statements (node_id, line, expr_id)
                         VALUES (?1,?2,?3)",
                        rusqlite::params![node_id, line as i64, eid],
                    )?;
                    report.statements += 1;
                }
            }
            Node::Theorem(t) => {
                let eid = write_expr(&tx, &t.statement, node_id, None, None, &h)?;
                tx.execute(
                    "INSERT INTO node_statements (node_id, line, expr_id)
                     VALUES (?1,0,?2)",
                    rusqlite::params![node_id, eid],
                )?;
                report.statements += 1;
                for (pos, step) in t.proof.iter().enumerate() {
                    write_proof_step(&tx, node_id, step, None, pos as i64, &h)?;
                }
            }
            Node::Definition(d) => {
                let mut seen = HashSet::new();
                let mut pos = 0i64;
                for &req_id in &d.base.req {
                    let tgt = req_id as i64;
                    if tgt == node_id || !seen.insert(req_id) {
                        continue; // no self-loops, no duplicates
                    }
                    tx.execute(
                        "INSERT INTO node_req (source_node_id, target_node_id, position)
                         VALUES (?1,?2,?3)",
                        rusqlite::params![node_id, tgt, pos],
                    )?;
                    pos += 1;
                    report.req_edges += 1;
                }
                // Store the Sema so a definition's unfolding can be reconstructed
                // (a proof citing `[D.BT.<n>]` needs it to justify the step).
                let mut sema_eid: Option<i64> = None;
                if let Some(qubitick_model::FormBody::MacroExpr(sema)) = &d.d_block.form {
                    let eid = write_expr(&tx, sema, node_id, None, None, &h)?;
                    tx.execute(
                        "INSERT INTO node_statements (node_id, line, expr_id)
                         VALUES (?1,0,?2)",
                        rusqlite::params![node_id, eid],
                    )?;
                    report.statements += 1;
                    sema_eid = Some(eid);
                }
                // The D-block metadata (elab rule, apply_form, binder_kind,
                // mod_roles, operator role) was already persisted in the earlier
                // metadata pass, so the contexts used HERE can see it. Only the
                // FormBody row needs this pass's Sema expr id.
                if d.d_block.elab_rule.is_some() && d.symbol.primary != 0 {
                    let sid = d.symbol.primary as i64;
                    persist_form_body(&tx, sid, &d.d_block.form, sema_eid)?;
                }
            }
            Node::Calibration(c) => {
                tx.execute(
                    "INSERT INTO calibrations
                        (node_id, symbol, q_units, si_dimensions, conversion_rate)
                     VALUES (?1,?2,?3,?4,?5)",
                    rusqlite::params![
                        node_id,
                        c.symbol,
                        c.q_units,
                        c.si_dimensions,
                        c.conversion_rate
                    ],
                )?;
            }
            _ => {}
        }
    }
    tx.commit()?;
    Ok(report)
}

/// Persist a definition's D-block `ModRoleTable` into `symbol_mod_roles`
/// (DataSchema §6.3): one row per (slot, permitted role). No behaviour change —
/// it makes the DB carry the mod-role metadata explicitly.
/// Pass 1: persist a Definition's D-block METADATA (elab rule, apply_form,
/// binder_kind, mod_roles, and the derived surface ROLE). Runs BEFORE the main
/// context build so the parser's contexts carry this metadata — the DB is then
/// the single source of truth for dispatch within the same ingest.
fn persist_definition_metadata(
    tx: &rusqlite::Transaction,
    ctx: &qubitick_ctx::Ctx,
    src: &str,
    d: &qubitick_model::Definition,
    precedence: &[Vec<String>],
) -> anyhow::Result<()> {
    let Some(rule) = &d.d_block.elab_rule else {
        return Ok(());
    };
    if d.symbol.primary == 0 {
        return Ok(()); // unresolved symbol: the FK would fail
    }
    let sid = d.symbol.primary as i64;
    let apply_form = d.d_block.apply_form.map(|a| a.as_str());
    // A binder's KIND is the parser's structural read of its subscript
    // (`_{i : T}` binds a `fun`, `_{x \to 0}` a limit); the engine no longer picks a
    // label out of the glyph's spelling, so nothing is recorded here.
    let binder_kind: Option<&str> = None;
    tx.execute(
        "INSERT INTO symbol_elab_rules (symbol_id, rule, apply_form, binder_kind)
         VALUES (?1,?2,?3,?4)
         ON CONFLICT(symbol_id) DO UPDATE SET rule=?2, apply_form=?3, binder_kind=?4",
        rusqlite::params![sid, rule.as_str(), apply_form, binder_kind],
    )?;
    persist_mod_roles(tx, sid, &d.d_block.mod_roles)?;
    // TWO AXES, TWO PLACES. The glyph line states the FIXITY (where the operands
    // sit) and that is what the `role` column carries; a `Role:` states the
    // CLASSIFICATION (relation, connective, null, nullity) and that is a facet bit.
    // A glyph can be both — a binary operator that is also the nullity op — so the
    // two must not compete for one column.
    let declared = d.d_block.role;
    let fixity: Option<&'static str> = match declared {
        Some(qubitick_model::DeclaredRole::Infix) => Some("infix"),
        Some(qubitick_model::DeclaredRole::Prefix) => Some("prefix"),
        Some(qubitick_model::DeclaredRole::Collector) => Some("collector"),
        _ => derive_role(ctx, src, sid),
    };
    if fixity.is_some() || declared.is_some() {
        use qubitick_model::corpus::Facet;
        use qubitick_model::DeclaredRole;
        let mut facets: u32 = tx
            .query_row("SELECT facets FROM symbols WHERE id = ?1", [sid], |r| r.get(0))
            .unwrap_or(0);
        let bit = |f: Facet| 1u32 << (f as u8);
        if fixity == Some("infix") {
            facets |= bit(Facet::BinOp);
        } else {
            // A declaration outranks the glyph line, so a glyph the shape calls
            // binary can still be declared unary — that is the point of D9.
            facets &= !bit(Facet::BinOp);
        }
        match declared {
            Some(DeclaredRole::Connective) => facets |= bit(Facet::Connective),
            Some(DeclaredRole::Relation) => facets |= bit(Facet::Relation),
            Some(DeclaredRole::Collector) => facets |= bit(Facet::Collector),
            Some(DeclaredRole::Null) => facets |= bit(Facet::Null),
            Some(DeclaredRole::Nullity) => facets |= bit(Facet::Nullity),
            _ => {}
        }
        // The charter's level for this glyph, 1 = loosest; NULL when the table does
        // not name it (the parser then gives it the one shared loosest level).
        let level: Option<i64> = ctx.by_id(sid).and_then(|r| {
            precedence
                .iter()
                .position(|l| l.contains(&r.tex))
                .map(|i| i as i64 + 1)
        });
        tx.execute(
            "UPDATE symbols SET role = ?1, facets = ?2, precedence = ?3 WHERE id = ?4",
            rusqlite::params![fixity, facets as i64, level, sid],
        )?;
    }
    // THE DECLARED KIND: a glyph can be a formation, a primitive or plain notation,
    // and `d.d_block.kind` says which ONCE the node parses against a context that
    // knows the corpus's declarations (a formation is only recognisable when the
    // connective separating its constructors is declared). The symbol hash is derived
    // from the kind, so both are written together, before any expr is stored.
    if let Some(row) = ctx.by_id(sid) {
        let kind = d.d_block.kind.as_str();
        let hash = hash_symbol(&row.scope, &row.tex, kind);
        tx.execute(
            "UPDATE symbols SET kind = ?1, symbol_hash = ?2 WHERE id = ?3",
            rusqlite::params![kind, hash.to_le_bytes().to_vec(), sid],
        )?;
    }
    Ok(())
}

fn persist_mod_roles(
    tx: &rusqlite::Transaction,
    symbol_id: i64,
    roles: &qubitick_model::ModRoleTable,
) -> anyhow::Result<()> {
    let slots = [
        ("l_sup", &roles.l_sup),
        ("r_sup", &roles.r_sup),
        ("l_sub", &roles.l_sub),
        ("r_sub", &roles.r_sub),
    ];
    for (slot, set) in slots {
        for role in set.0.iter() {
            tx.execute(
                "INSERT INTO symbol_mod_roles (symbol_id, slot, role) VALUES (?1,?2,?3)
                 ON CONFLICT(symbol_id, slot, role) DO NOTHING",
                rusqlite::params![symbol_id, slot, mod_role_tag(*role)],
            )?;
        }
    }
    Ok(())
}

/// Derive an operator's surface ROLE from its GLYPH LINE shape (not its name, and
/// not its Sema — the Sema of a prefix op cannot parse until prefix-ness is known).
///
/// Lex the glyph line and locate the declared symbol: if it is the FIRST token
/// (nothing to its left) it is a PREFIX operator (`\lnot a`, `-_{\mathbb Z} x`);
/// if it is flanked by operands it is INFIX (`a \cap b`). A single bare atom
/// (e.g. a sort `\mathbb{Z}`) yields no role.
fn derive_role(ctx: &qubitick_ctx::Ctx, src: &str, sym_id: i64) -> Option<&'static str> {
    let line = first_glyph_line(src);
    if line.is_empty() {
        return None;
    }
    let toks = qubitick_parse::lex(&line, ctx).ok()?;
    let mut idx = None;
    for (i, t) in toks.iter().enumerate() {
        if let qubitick_parse::Token::Symbol(id, _) = t
            && *id as i64 == sym_id
        {
            idx = Some(i);
            break;
        }
    }
    let idx = idx?;
    // A BRACKET glyph (`\Vert x \Vert`, `\ulcorner x \urcorner`): the glyph line
    // opens with the declared symbol and CLOSES with a symbol token (the bracket
    // bar). Symmetric (`\Vert … \Vert`) or paired (`\ulcorner … \urcorner`).
    let sym_at = |t: Option<&qubitick_parse::Token>| {
        matches!(t, Some(qubitick_parse::Token::Symbol(_, _)))
    };
    if idx == 0 && toks.len() >= 3 && sym_at(toks.get(toks.len() - 2)) {
        return Some("collector");
    }
    // An APPLICATION glyph (`f(x)`, `\mathfrak{L}(t)`) applies via parentheses; its
    // symbol is followed by `(`, which is not a bare-operand prefix form.
    if matches!(toks.get(idx + 1), Some(qubitick_parse::Token::LParen(_))) {
        return None;
    }
    // Operand token kinds (things that can be a bare operand).
    let is_operand = |t: &qubitick_parse::Token| {
        matches!(
            t,
            qubitick_parse::Token::Symbol(_, _) | qubitick_parse::Token::Var(_, _)
        )
    };
    let left = toks[..idx].iter().any(is_operand);
    let right = toks.get(idx + 1).is_some_and(is_operand);
    match (left, right) {
        (false, true) => Some("prefix"),
        (true, true) => Some("infix"),
        _ => None,
    }
}

/// Persist a definition's D-block `FormBody` kind into `form_bodies`
/// (DataSchema §6.2). `MacroExpr` links the Sema expr row; `Formation` and
/// `Bracket` record their shape (Bracket its delimiters).
fn persist_form_body(
    tx: &rusqlite::Transaction,
    symbol_id: i64,
    form: &Option<qubitick_model::FormBody>,
    sema_eid: Option<i64>,
) -> anyhow::Result<()> {
    use qubitick_model::FormBody as F;
    let (kind, body, open, close, ctors) = match form {
        Some(F::MacroExpr(_)) => ("MacroExpr", sema_eid, None, None, None),
        // A Recursive form must reference a body expr (schema CHECK); skip it if
        // none was written rather than violate the constraint.
        Some(F::Recursive(_, _)) => match sema_eid {
            Some(id) => ("Recursive", Some(id), None, None, None),
            None => return Ok(()),
        },
        Some(F::Formation(ctors)) => (
            "Formation",
            None,
            None,
            None,
            Some(serde_json::to_string(ctors)?),
        ),
        Some(F::Bracket(o, c)) => ("Bracket", None, Some(o.clone()), Some(c.clone()), None),
        None => return Ok(()),
    };
    tx.execute(
        "INSERT INTO form_bodies
            (symbol_id, form_kind, body_expr_id, open_delim, close_delim, ctors)
         VALUES (?1,?2,?3,?4,?5,?6)
         ON CONFLICT(symbol_id) DO NOTHING",
        rusqlite::params![symbol_id, kind, body, open, close, ctors],
    )?;
    Ok(())
}

fn mod_role_tag(r: qubitick_model::ModRoleKind) -> &'static str {
    use qubitick_model::ModRoleKind as M;
    match r {
        M::Exponent => "Exponent",
        M::Index => "Index",
        M::Annotation => "Annotation",
        M::DerivativeOrder => "DerivativeOrder",
        M::Transpose => "Transpose",
        M::Inverse => "Inverse",
        M::BinderVar => "BinderVar",
        M::Bound => "Bound",
        M::SuperscriptMark => "SuperscriptMark",
    }
}

/// Walk a root for `.qbit` files, sorted for deterministic ordering.
pub fn walk_qbit(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().map(|x| x == "qbit").unwrap_or(false) {
                // `Canon.qbit` is the corpus CHARTER, not a node: it names the
                // project in prose (LaTeX in the name is the point), so it has no
                // `<kind> <name>` header and must not be ingested or built. It is
                // read by `canon::read`, which is the only thing that parses it.
                if p.file_name().map(|n| n == "Canon.qbit").unwrap_or(false) {
                    continue;
                }
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Derive the node `(era, strata)` from a file's path under the project root,
/// e.g. `project/Era1/Strata2/...` → `("Era1", 2)`. Defaults to `("Era0", 0)`.
fn era_strata_of(root: &Path, path: &Path) -> (String, i64) {
    let mut era = "Era0".to_string();
    let mut strata = 0i64;
    if let Ok(rel) = path.strip_prefix(root) {
        for comp in rel.components() {
            let s = comp.as_os_str().to_string_lossy();
            if let Some(n) = s.strip_prefix("Era").and_then(|r| r.parse::<i64>().ok()) {
                era = format!("Era{n}");
            } else if let Some(m) = s.strip_prefix("Strata").and_then(|r| r.parse::<i64>().ok()) {
                strata = m;
            }
        }
    }
    (era, strata)
}

/// Render a citation reference as the `cite_text` column value (DataSchema §7).
fn cite_text(name: &qubitick_model::CiteName) -> String {
    use qubitick_model::CiteName;
    match name {
        CiteName::AxiomLine { node, line } => format!("A{node}.{line}"),
        CiteName::TheoremId(n) => format!("T.{n}"),
        CiteName::DefId(n) => format!("D.{n}"),
        CiteName::Local(v) => v.clone(),
    }
}

/// Write one proof step (and recursively its children) into `proof_steps`,
/// `chain_rows`, `chain_rewrite_refs` and `conclude_terms` (DataSchema §7).
/// Returns the new `proof_steps.id` so parents can link it.
fn write_proof_step(
    tx: &rusqlite::Transaction,
    node_id: i64,
    step: &qubitick_model::ProofStep,
    parent_step_id: Option<i64>,
    position: i64,
    h: &dyn Fn(u32) -> u128,
) -> Result<i64> {
    use qubitick_model::{ChainJust, ProofStep, StepRel};
    let (step_kind, scrutinee_id, var_name, note_name, note_prop_id) = match step {
        ProofStep::Chain { .. } => ("Chain", None, None, None, None),
        ProofStep::Conclude { .. } => ("Conclude", None, None, None, None),
        ProofStep::Case { scrutinee, .. } => (
            "Case",
            Some(write_expr(tx, scrutinee, node_id, None, None, h)?),
            None,
            None,
            None,
        ),
        ProofStep::Induction { var, .. } => ("Induction", None, Some(var.clone()), None, None),
        ProofStep::Recursor { scrutinee, .. } => (
            "Recursor",
            Some(write_expr(tx, scrutinee, node_id, None, None, h)?),
            None,
            None,
            None,
        ),
        ProofStep::Note { name, prop, .. } => (
            "Note",
            None,
            None,
            Some(name.clone()),
            Some(write_expr(tx, prop, node_id, None, None, h)?),
        ),
        ProofStep::Rewrite { .. } => ("Rewrite", None, None, None, None),
        ProofStep::Fix { vars } => ("Fix", None, vars.first().cloned(), None, None),
        ProofStep::Discriminate { hyp } => ("Discriminate", None, Some(hyp.clone()), None, None),
    };
    tx.execute(
        "INSERT INTO proof_steps
            (node_id, parent_step_id, position, step_kind, scrutinee_expr_id, var_name, note_name, note_prop_expr_id)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        rusqlite::params![
            node_id,
            parent_step_id,
            position,
            step_kind,
            scrutinee_id,
            var_name,
            note_name,
            note_prop_id
        ],
    )?;
    let step_id = tx.last_insert_rowid();
    match step {
        ProofStep::Chain { rows } => {
            for (rp, row) in rows.iter().enumerate() {
                let lhs_id = match &row.lhs {
                    Some(e) => Some(write_expr(tx, e, node_id, None, None, h)?),
                    None => None,
                };
                let rhs_id = write_expr(tx, &row.rhs, node_id, None, None, h)?;
                let rel = match row.rel {
                    StepRel::Eq => "Eq",
                    StepRel::Arrow => "Arrow",
                };
                let just = match &row.just {
                    ChainJust::Rewrite { .. } => "Rewrite",
                    ChainJust::Sym(_) => "Sym",
                    ChainJust::Exact { .. } => "Exact",
                    ChainJust::Rfl => "Rfl",
                };
                tx.execute(
                    "INSERT INTO chain_rows (step_id, position, lhs_expr_id, rhs_expr_id, rel, just_kind)
                     VALUES (?1,?2,?3,?4,?5,?6)",
                    rusqlite::params![step_id, rp as i64, lhs_id, rhs_id, rel, just],
                )?;
                if let ChainJust::Rewrite { refs } = &row.just {
                    for (fp, r) in refs.iter().enumerate() {
                        tx.execute(
                            "INSERT INTO chain_rewrite_refs (step_id, row_position, ref_position, rev, cite_text)
                             VALUES (?1,?2,?3,?4,?5)",
                            rusqlite::params![
                                step_id,
                                rp as i64,
                                fp as i64,
                                r.rev as i64,
                                cite_text(&r.name)
                            ],
                        )?;
                    }
                }
            }
        }
        ProofStep::Conclude { term } => {
            let tid = write_expr(tx, term, node_id, None, None, h)?;
            tx.execute(
                "INSERT INTO conclude_terms (step_id, term_expr_id) VALUES (?1,?2)",
                rusqlite::params![step_id, tid],
            )?;
        }
        ProofStep::Case { arms, .. } => {
            for (ap, arm) in arms.iter().enumerate() {
                let mut children: Vec<i64> = Vec::new();
                for (bp, s) in arm.body.iter().enumerate() {
                    children.push(write_proof_step(tx, node_id, s, Some(step_id), bp as i64, h)?);
                }
                if let Some(&body_id) = children.first() {
                    tx.execute(
                        "INSERT INTO case_arms (step_id, position, pattern_id, body_step_id) VALUES (?1,?2,NULL,?3)",
                        rusqlite::params![step_id, ap as i64, body_id],
                    )?;
                }
            }
        }
        ProofStep::Induction { cases, .. } | ProofStep::Recursor { cases, .. } => {
            for (cp, case) in cases.iter().enumerate() {
                let mut children: Vec<i64> = Vec::new();
                for (bp, s) in case.body.iter().enumerate() {
                    children.push(write_proof_step(tx, node_id, s, Some(step_id), bp as i64, h)?);
                }
                if let Some(&body_id) = children.first() {
                    tx.execute(
                        "INSERT INTO induction_cases (step_id, position, pattern_id, body_step_id) VALUES (?1,?2,NULL,?3)",
                        rusqlite::params![step_id, cp as i64, body_id],
                    )?;
                }
            }
        }
        ProofStep::Note { body, .. } => {
            for (bp, s) in body.iter().enumerate() {
                write_proof_step(tx, node_id, s, Some(step_id), bp as i64, h)?;
            }
        }
        _ => {}
    }
    Ok(step_id)
}

/// All non-SQLite-admin table names in the corpus schema.
fn user_table_names(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .map(|r| r.map_err(anyhow::Error::from))
        .collect::<Result<Vec<_>>>()?;
    Ok(rows)
}

/// The surface glyph line of a Definition: the first bare (non-keyword) line.
fn first_glyph_line(src: &str) -> String {
    for line in src.lines().skip(1) {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.split_once(':').is_some_and(|(k, _)| is_field_keyword(k)) {
            continue;
        }
        return t.to_string();
    }
    String::new()
}

/// The head `\command` (with optional braced argument) of a surface glyph line.
fn head_control_word(line: &str) -> Option<String> {
    let trimmed = line.trim();
    match trimmed.find('\\') {
        None => {
            // No backslash: the glyph line is a bare expression. It may be:
            //  - a single bare identifier (`S`, `T`, `U`) → the identifier is the head
            //  - an infix expression with a punctuation operator (`a + b`) → the operator is the head
            //  - a bare identifier with mods (`T_{jk}`) → the leading identifier is the head
            // Heuristic: split by whitespace. If exactly 3 tokens and the middle is
            // all OPERATOR_PUNCT characters, it's an infix operator declaration.
            let tokens: Vec<&str> = trimmed.split_whitespace().collect();
            if tokens.len() == 3 {
                let middle = tokens[1];
                let middle_is_punct = !middle.is_empty()
                    && middle.chars().all(is_operator_punct);
                if middle_is_punct {
                    return Some(middle.to_string());
                }
            }
            // Otherwise, take the leading alphanumeric run (e.g. the canon's `S`, `T`, `U`).
            let lead_alpha: String = trimmed
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            if !lead_alpha.is_empty() {
                return Some(lead_alpha);
            }
            // Next, try a leading OPERATOR PUNCTUATION run: only OPERATOR_PUNCT characters.
            // Structural punctuation (brackets, comma, period, semicolon, quotes, backslash)
            // is EXCLUDED — it is grammar, not notation.
            let lead_punct: String = trimmed
                .chars()
                .take_while(|c| is_operator_punct(*c))
                .collect();
            if !lead_punct.is_empty() {
                return Some(lead_punct);
            }
            // Fallback: empty or only whitespace/backslash.
            if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
        }
        Some(pos) => {
            let rest = &trimmed[pos + 1..];
            let word: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphabetic())
                .collect();
            if word.is_empty() {
                return None;
            }
            let mut tex = format!("\\{word}");
            let after = &rest[word.len()..];
            // A braced-ARGUMENT macro (`\frac`) heads an application; its `{…}`
            // groups are arguments, so register the BARE command (no glue). Font/
            // decoration commands (`\mathbb{Z}`, `\hat{B}`) glue the single braced
            // group into the symbol tex as before. Mirrors `lexer::lex` plus the
            // `command_tex` below; all three must agree byte for byte or a texture
            // the lexer emits never resolves (raw-text fallback → Lean reject).
            // Structural count of consecutive braced groups, shared with the lexer.
            let after_bytes = after.as_bytes();
            let n2 = after_bytes.len();
            let is_braced_apply =
                qubitick_model::corpus::braced_group_count(after_bytes, 0, n2) >= 2;
            if !is_braced_apply
                && let Some(end) = after.strip_prefix('{').and_then(|r| r.find('}'))
            {
                tex.push_str(&after[..=end + 1]);
            }
            Some(tex)
        }
    }
}

/// A `.qbit` surface field keyword (never a glyph/statement).
///
/// THIS LIST IS `crates/parse/src/node.rs::split_fields`'S, word for word. It was wider — `sig`,
/// `form`, `shape`, `role`, `thm`, `tex`, `prec`, `assoc`, `trigger`, `req`, `kind` were here and
/// absent from the parser — which made such a line a FIELD to this scan and a STATEMENT to the
/// parser: skipped for symbols, then parsed as an expression. One list, or the two disagree about
/// what a line is.
fn is_field_keyword(k: &str) -> bool {
    matches!(
        k.trim().to_ascii_lowercase().as_str(),
        "symbol" | "qunits" | "dims" | "rate"
            | "sema" | "type" | "role" | "lean"
            | "proof"
            | "falsify"
            | "math" | "phys"
    )
}

/// Distinct LaTeX control words (`\word`) used in a source text.
/// Lean's reserved words — a generated symbol name must never be one.
const LEAN_KEYWORDS: &[&str] = &[
    "in", "at", "by", "def", "do", "elab", "else", "end", "example", "export", "extends",
    "for", "forall", "from", "fun", "have", "if", "import", "inductive", "instance", "let",
    "local", "macro", "match", "mutual", "namespace", "noncomputable", "open", "partial", "prelude",
    "private", "protected", "section", "show", "structure", "syntax", "then", "theorem", "universe",
    "unsafe", "variable", "where", "with", "axiom", "class", "abbrev", "deriving", "scoped",
    "calc", "suffices", "attribute", "set_option", "termination_by", "decreasing_by",
];

/// Distinct LaTeX control words (`\word`) used in a source text.
/// used in a source text. Mirrors the lexer's command reconstruction so every
/// on-command the parser will look up is registered as a symbol.
/// Absorb trailing `_`/`^` mods into `tex`, e.g. `_{\tau}` or `_{\mathbb{Z}^{+}}`.
///
/// Braces are matched with a DEPTH COUNTER so a mod may contain its own braced group
/// (`\uparrow_{\mathbb{Z}^{-}}`, `\ge_{0}^{\mathbb{Z}}`, `+_{\mathbb{Z}^{+}}`).
/// A naive first-`}` scan cuts the symbol short, the registered tex then disagrees
/// with the one the lexer reconstructs, the lookup misses, and the whole statement
/// degrades to raw text that Lean rejects.
fn hand_mods(src: &str, i: &mut usize, tex: &mut String) {
    let b = src.as_bytes();
    let n = b.len();
    while *i < n && (b[*i] == b'_' || b[*i] == b'^') {
        tex.push(b[*i] as char);
        *i += 1;
        if *i < n && b[*i] == b'{' {
            tex.push('{');
            *i += 1;
            let mut depth = 1usize;
            while *i < n {
                let c = b[*i];
                if c == b'{' {
                    depth += 1;
                } else if c == b'}' {
                    depth -= 1;
                    if depth == 0 {
                        tex.push('}');
                        *i += 1;
                        break;
                    }
                }
                tex.push(c as char);
                *i += 1;
            }
        } else if *i < n && b[*i] == b'\\' {
            // `\mathcal{U}_\odot`: the whole command belongs to the glyph.
            tex.push('\\');
            *i += 1;
            while *i < n && b[*i].is_ascii_alphabetic() {
                tex.push(b[*i] as char);
                *i += 1;
            }
            if *i < n && b[*i] == b'{' {
                tex.push('{');
                *i += 1;
                while *i < n && b[*i] != b'}' {
                    tex.push(b[*i] as char);
                    *i += 1;
                }
                if *i < n {
                    tex.push('}');
                    *i += 1;
                }
            }
        } else if *i < n {
            tex.push(b[*i] as char);
            *i += 1;
        }
    }
}

/// Multi-letter alpha runs inside a BRACED mod argument (`T_{jk}`, `R^{ij}`)
/// are tensor/index LABELS: register each as a symbol so the lexer resolves
/// `jk` (one index label) from the DB instead of erroring on a multi-letter
/// bare identifier. Data-driven: the label comes from the .qbit text itself.
fn braced_index_labels(src: &str) -> Vec<String> {
    let b = src.as_bytes();
    let n = b.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        // Look for `_` or `^` immediately followed by `{`.
        if (b[i] == b'_' || b[i] == b'^') && i + 1 < n && b[i + 1] == b'{' {
            let mut j = i + 2;
            let mut depth = 1usize;
            let mut cur = String::new();
            let push = |cur: &mut String, out: &mut Vec<String>| {
                if cur.len() > 1 && cur.chars().all(|c| c.is_ascii_alphabetic() || c.is_ascii_digit()) {
                    out.push(cur.clone());
                }
                cur.clear();
            };
            while j < n && depth > 0 {
                let c = b[j];
                match c {
                    b'{' => {
                        depth += 1;
                        push(&mut cur, &mut out);
                    }
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        push(&mut cur, &mut out);
                    }
                    b',' => push(&mut cur, &mut out),
                    c if c.is_ascii_alphanumeric() => cur.push(c as char),
                    _ => push(&mut cur, &mut out),
                }
                j += 1;
            }
            push(&mut cur, &mut out);
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out
}

fn control_words(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < src.len() {
        if src.as_bytes()[i] == b'\\' {
            if let Some(tex) = command_tex(src, &mut i) {
                out.push(tex);
            }
        } else if src.as_bytes()[i].is_ascii_digit()
            || (src.as_bytes()[i] == b'-'
                && src.as_bytes().get(i + 1).is_some_and(|b| b.is_ascii_digit()))
        {
            // Numeric literals (`0`, `1`, `-1`) are symbols on the surface.
            let start = i;
            if src.as_bytes()[i] == b'-' {
                i += 1;
            }
            while i < src.len() && src.as_bytes()[i].is_ascii_digit() {
                i += 1;
            }
            out.push(src[start..i].to_string());
        } else if matches!(src.as_bytes()[i], b'+' | b'*' | b'/')
            || (src.as_bytes()[i] == b'-'
                && !src.as_bytes().get(i + 1).is_some_and(|b| b.is_ascii_digit()))
        {
            // ASCII OPERATOR GLYPHS are symbols in their own right.
            //
            // The `.qbit` surface is ASCII LaTeX only (SurfaceFormat §5: no unicode
            // glyphs, no English-word atoms), so an operator that carries its sort is
            // authored literally and puts the sort in a mod:
            //
            //     a +_{\tau} \tau_0      b -_{\mathbb{Z}} c      x +_{\mathbb{Z}^{+}} y
            //
            // `+` appears 599 times and `-` 239 times in the corpus. The lexer matches
            // any REGISTERED tex by longest prefix, so these only need registering
            // here; without it the lex reports "unknown", the statement falls back to
            // raw text, and Lean rejects it with `expected token`.
            //
            // `-` followed by a digit is a negative numeric literal, handled above.
            let mut tex = String::new();
            tex.push(src.as_bytes()[i] as char);
            i += 1;
            hand_mods(src, &mut i, &mut tex);
            out.push(tex);
        } else if src.as_bytes()[i] == b'^'
            && src
                .as_bytes()
                .get(i + 1)
                .is_some_and(|b| is_operator_punct(*b as char))
        {
            // `^` + an operator character is ONE symbol the corpus may declare (`^+`,
            // `^*`, ...). The rule is the format's; the glyph is the corpus's.
            out.push(format!("^{}", src.as_bytes()[i + 1] as char));
            i += 2;
        } else if is_operator_punct(src.as_bytes()[i] as char) {
            // BARE OPERATOR PUNCTUATION RUN: a maximal sequence of OPERATOR_PUNCT characters.
            // This catches `=`, `**`, `==`, `!=`, `+=`, `->`, etc. as declarable symbols.
            // Structural punctuation (brackets, comma, period, semicolon, quotes, backslash)
            // is EXCLUDED — it is grammar, not notation.
            let start = i;
            while i < src.len() && is_operator_punct(src.as_bytes()[i] as char) {
                i += 1;
            }
            out.push(src[start..i].to_string());
        } else {
            i += 1;
        }
    }
    out.extend(braced_index_labels(src));
    out.sort();
    out.dedup();
    out
}

/// Reconstruct a full LaTeX command token (command + optional braced arg +
/// optional underscore mod) starting at the backslash at `i`, advancing `i`
/// past the token exactly as the lexer does.
fn command_tex(src: &str, i: &mut usize) -> Option<String> {
    let b = src.as_bytes();
    let start = *i;
    *i += 1; // the '\\'
    let mut word = String::new();
    while *i < b.len() && (b[*i].is_ascii_alphabetic() || b[*i] == b'@') {
        word.push(b[*i] as char);
        *i += 1;
    }
    if word.is_empty() {
        *i = start + 1;
        return None;
    }
    let mut tex = format!("\\{word}");
    // A braced-ARGUMENT macro (`\frac{n}{d}`) registers the BARE command — its
    // `{…}` groups are application args, not part of the symbol tex. Font
    // commands (`\mathbb{Z}`) keep the old one-brace glue. STRUCTURAL (count of
    // consecutive braced groups), shared with the lexer so both agree.
    let is_braced_apply = qubitick_model::corpus::braced_group_count(b, *i, b.len()) >= 2;
    // Optional braced argument, e.g. ``\mathbb{Z}``.
    if !is_braced_apply && *i < b.len() && b[*i] == b'{' {
        *i += 1;
        let mut arg = String::new();
        while *i < b.len() && b[*i] != b'}' {
            arg.push(b[*i] as char);
            *i += 1;
        }
        if *i < b.len() {
            *i += 1; // consume '}'
        }
        tex.push('{');
        tex.push_str(&arg);
        tex.push('}');
    }
    // Optional mod suffixes (`_`/`^`): `\\le_{\\tau}`, `\\ge_{0}^{\\mathbb{Z}}`,
    // `\\uparrow_{\\mathbb{Z}^{-}}`. Nesting-aware and shared with
    // `lexer::absorb_mods`; the two must agree on the tex byte for byte or the
    // registered glyph is never matched and the statement degrades to raw text.
    hand_mods(src, i, &mut tex);
    if tex == "\\" {
        None
    } else {
        Some(tex)
    }
}

/// Derive a valid Lean identifier from a LaTeX tex token.
fn lean_name(tex: &str) -> String {
    // A numeric literal sanitizes to itself, but must not start a Lean ident.
    if tex.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return format!("num_{tex}");
    }
    if let Some(rest) = tex.strip_prefix('-')
        && !rest.is_empty()
        && rest.chars().all(|c| c.is_ascii_digit())
    {
        return format!("neg_{rest}");
    }
    // An ASCII OPERATOR GLYPH must keep its operator in the generated name.
    //
    // The generic sanitisation below maps every non-alphanumeric byte to `_`, so
    // `+_{\tau}` collapses to `tau` — colliding with the SORT `\tau`. The sort is
    // declared `Type`, so applying the operator then applies a type to arguments and
    // Lean reports `unknown constant 'CoeFun'` (and the statement is silently
    // wrong: `Eq (tau a tau_0) a`). Likewise `-_{\mathbb{Z}}` would become
    // `mathbb_Z`, colliding with the set `\mathbb{Z}`.
    //
    // The surface is ASCII LaTeX only, so the operator byte is always present and
    // is a faithful, unique part of the glyph's identity.
    // A `^{+}` / `^{-}` MOD must survive sanitisation.
    //
    // The generic mapping below turns every non-alphanumeric byte into `_` and then
    // trims, so `\mathbb{Z}^{+}` and `\mathbb{Z}^{-}` both collapse to `mathbb_Z` —
    // colliding with the set `\mathbb{Z}` itself. The statement then reads
    // `plus_mathbb_Z (mathbb_Z tau_0) …`, applying a `Prop` to arguments, which Lean
    // reports as `unknown constant 'CoeFun'`. Same class as the `+_{\tau}` collision.
    let tex = &tex
        .replace("^{+}", "_plus")
        .replace("^{-}", "_minus");
    let tex = tex.as_str();
    let sanitize = |t: &str| -> String {
        let trimmed = t
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '\'' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();
        trimmed.trim_matches('_').to_string()
    };
    // Handle single-char punctuation operators first, then multi-char runs.
    // The match on the first char gives us the base name; the tail (rest of the
    // punctuation run) is sanitized and appended. This covers `+` -> `plus`,
    // `=` -> `eq`, `**` -> `star_star`, `!=` -> `bang_eq`, etc.
    // Use char indices to handle multi-byte UTF-8 like `×`.
    let first_char = tex.chars().next();
    if let Some((word, tail_char_count)) = match first_char {
        Some('+') => Some(("plus", 1)),
        Some('-') => Some(("minus", 1)),
        Some('*') => Some(("star", 1)),
        Some('/') => Some(("slash", 1)),
        Some('=') => Some(("eq", 1)),
        Some('!') => Some(("bang", 1)),
        Some('<') => Some(("lt", 1)),
        Some('>') => Some(("gt", 1)),
        Some('%') => Some(("pct", 1)),
        Some('&') => Some(("amp", 1)),
        Some('|') => Some(("bar", 1)),
        Some('^') => Some(("caret", 1)),
        Some('~') => Some(("tilde", 1)),
        Some('×') => Some(("times", 1)),
        _ => None,
    } {
        // `-` followed by digits was already returned as `neg_<n>` above.
        let tail: String = tex.chars().skip(tail_char_count).collect();
        let tail = sanitize(&tail);
        return if tail.is_empty() {
            word.to_string()
        } else {
            format!("{word}_{tail}")
        };
    }
    let mut s = sanitize(tex);
    if s.is_empty() {
        // A glyph written only in punctuation the map above does not know still needs
        // a name of its own: derive it from the codepoints, so two distinct glyphs
        // never collide and the engine picks no word for either.
        let mut suffix = String::new();
        for c in tex.chars() {
            suffix.push_str(&format!("{:x}", c as u32));
        }
        s = format!("sym_{suffix}");
    }
    if s.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        s.insert(0, '_');
    }
    // Never emit a Lean keyword as a generated name: `\in` sanitizes to `in`,
    // which is a hard parse error in the emitted module.
    if LEAN_KEYWORDS.contains(&s.as_str()) {
        s.push('_');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("qubitick_ingest_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn ingests_a_mini_corpus() {
        let dir = scratch_dir("mini");
        fs::write(
            dir.join("D_BT_08_Capacity.qbit"),
            "D.BT.08. Capacity\n\\mathsf{C}\nType: \\mathfrak{V} \\to \\mathfrak{V} \\to \\mathfrak{E}\n",
        )
        .unwrap();
        fs::write(
            dir.join("T_BT_01_DeltaComm.qbit"),
            "T.BT.01. DeltaComm\n\\forall a b : \\mathfrak{E}, a \\Delta b = b \\Delta a\n",
        )
        .unwrap();

        let mut conn = qubitick_db::open_in_memory().unwrap();
        let report = ingest_corpus(&mut conn, &dir).unwrap();
        assert_eq!(report.nodes, 2, "one definition + one theorem");
        assert!(report.symbols >= 2, "Capacity glyph + control words");

        let node_list = qubitick_db::read::get_node_list(&conn).unwrap();
        assert_eq!(node_list.len(), 2);
        // Godel codes are the ledger ordinals, ordered by sort.
        let godels: Vec<i64> = node_list.iter().filter_map(|n| n.godel).collect();
        assert_eq!(godels, vec![1, 2]);

        let syms = qubitick_db::queries::symbols_in_scope(&conn, "Era0/Strata0").unwrap();
        assert!(
            syms.iter().any(|r| r.tex == "\\mathsf{C}"),
            "Capacity glyph must be a declared symbol"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn real_corpus_persists_dblock_metadata() {
        // Evidence that the DB (from .qbit only) carries D-block metadata for the
        // REAL corpus, not just hand-made fixtures.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap().parent().unwrap().join("project");
        if !root.join("Era0/Strata0").is_dir() {
            return; // corpus not present in this checkout
        }
        let mut conn = qubitick_db::open_in_memory().unwrap();
        ingest_corpus(&mut conn, &root).unwrap();
        let elab: i64 = conn
            .query_row("SELECT COUNT(*) FROM symbol_elab_rules", [], |r| r.get(0))
            .unwrap();
        assert!(elab > 0, "real corpus must persist D-block elab rows, got {elab}");
        // Every persisted rule must be one of the ElabRule vocabulary.
        let bad: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM symbol_elab_rules WHERE rule NOT IN \
                 ('Identity','Apply','CurriedApply','Binder','Matrix','Vector','Set','Wrap')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(bad, 0, "all persisted elab rules must be valid");
        // Spot-check a known braced-arg macro and a known binder glyph.
        let has_binder: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM symbol_elab_rules e JOIN symbols s ON s.id=e.symbol_id \
                 WHERE e.rule='Binder'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(has_binder > 0, "the corpus binder (bigcup) must persist elab=Binder");
        // D-block mod-roles are persisted explicitly too (DataSchema §6.3).
        let mods: i64 = conn
            .query_row("SELECT COUNT(*) FROM symbol_mod_roles", [], |r| r.get(0))
            .unwrap();
        assert!(mods > 0, "mod roles must be persisted, got {mods}");
        let bad_mod: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM symbol_mod_roles WHERE slot NOT IN \
                 ('l_sup','r_sup','l_sub','r_sub')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(bad_mod, 0, "all mod-role slots must be valid");
        // D-block FormBody rows are persisted too (DataSchema §6.2).
        let forms: i64 = conn
            .query_row("SELECT COUNT(*) FROM form_bodies", [], |r| r.get(0))
            .unwrap();
        assert!(forms > 0, "form bodies must be persisted, got {forms}");
        let bad_form: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM form_bodies WHERE form_kind NOT IN \
                 ('MacroExpr','Recursive','Formation','Bracket')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(bad_form, 0, "all form kinds must be valid");
    }

    /* A SAVE INJECTS THE FILE'S SYMBOLS, AND A LATER INGEST REPLACES THEM RATHER THAN COLLIDING.
     *
     * The save path exists because a corpus ingest is the wrong shape for a save (whole ledger, node
     * ids renumbered — ~10 s for 330 files). What it must NOT do is disagree with the ingest about what
     * a symbol row is: this test saves a definition whose kind CHANGES (`Type:` only -> primitive, then
     * a `Sema:` -> notation), so an upsert that forgot the kind, or an insert that hit the unique
     * `(scope, tex)` and bailed, is visible here.
     */
    #[test]
    fn a_saved_file_injects_its_symbols_and_a_later_ingest_replaces_them() {
        let dir = scratch_dir("save_symbols");
        let def = dir.join("D_A_01_Twist.qbit");
        let user = dir.join("T_A_01_Use.qbit");
        fs::write(
            &def,
            "D.A.01. Twist\n\\twist a\nType: \\mathfrak{E} \\to \\mathfrak{E}\n",
        )
        .unwrap();
        fs::write(&user, "T.A.01. Use\n\\forall a : \\mathfrak{E}, \\twist a = a\n").unwrap();
        let mut conn = qubitick_db::open_in_memory().unwrap();
        ingest_corpus(&mut conn, &dir).unwrap();

        let kind_of = |conn: &Connection| -> String {
            conn.query_row(
                "SELECT kind FROM symbols WHERE tex = '\\twist'",
                [],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(kind_of(&conn), "primitive", "Type: only is a primitive");

        /* the author saves the same glyph WITH a Sema: — it is a definition now, and the row follows
           (`written` counts changes: the file's own word is its own glyph, so the use-only claim
           conflicts with the declaration and changes nothing) */
        fs::write(&def, "D.A.01. Twist\n\\twist a\nSema:\n\\twist a = a\n").unwrap();
        let report = ingest_file_symbols(&conn, &dir, &def).unwrap();
        assert!(report.in_ledger, "the file was ingested, so it has a node row");
        assert!(report.written >= 1, "the declaration was written: {report:?}");
        assert_eq!(kind_of(&conn), "notation", "the Sema: makes it a definition, on save");

        /* AND A FILE THAT ONLY USES A SYMBOL CANNOT DEMOTE IT. Ingest gives a DECLARED glyph
           precedence over a use-only claim; one file cannot see the corpus, so the save path inserts
           use-only claims only when the tex is new. Saving the theorem must leave the kind alone. */
        let used = ingest_file_symbols(&conn, &dir, &user).unwrap();
        assert!(used.in_ledger);
        assert_eq!(kind_of(&conn), "notation", "a use did not demote a declaration");

        /* AND THE BUILD STILL RUNS: ingest DELETEs every table before it re-derives, so the two write
           paths cannot collide on `UNIQUE (scope, tex)` or double-count */
        let again = ingest_corpus(&mut conn, &dir).unwrap();
        assert!(again.nodes >= 2, "the ledger is rebuilt over what the save wrote");
        assert_eq!(kind_of(&conn), "notation", "and it agrees with the save");
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM symbols WHERE tex = '\\twist'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "one row per (scope, tex), not one per writer");
    }

    /* A FILE THE ENGINE HAS NOT INGESTED HAS NOWHERE TO HANG A SYMBOL. `symbols.home_node` is NOT NULL
     * and foreign-keyed to `nodes`, so the honest answer is "not in the ledger yet" — guessing a node id
     * would attach the author's notation to someone else's node. The remedy is a build. */
    #[test]
    fn a_file_outside_the_ledger_injects_nothing_and_says_so() {
        let dir = scratch_dir("save_symbols_unledgered");
        fs::create_dir_all(&dir).unwrap();
        let def = dir.join("D_A_02_Loose.qbit");
        fs::write(&def, "D.A.02. Loose\n\\loose a\nType: \\mathfrak{E} \\to \\mathfrak{E}\n").unwrap();
        let mut conn = qubitick_db::open_in_memory().unwrap();
        ingest_corpus(&mut conn, &dir).unwrap();

        let new_file = dir.join("D_A_03_New.qbit");
        fs::write(&new_file, "D.A.03. New\n\\fresh a\nType: \\mathfrak{E} \\to \\mathfrak{E}\n").unwrap();
        let report = ingest_file_symbols(&conn, &dir, &new_file).unwrap();
        assert!(!report.in_ledger, "no node row for a file the walk has not seen");
        assert_eq!(report.written, 0);
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM symbols WHERE tex = '\\fresh'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "nothing was hung on an unrelated node");
    }

    #[test]
    fn user_prefix_operator_dispatches_from_db() {
        // A user's own unary operator must parse as an APPLICATION purely via the
        // DB role the two-pass ingest derived — no engine edit, no tex fallback.
        let dir = scratch_dir("dbdispatch");
        fs::write(
            dir.join("D_A.qbit"),
            "D.A.01. Twist\n\\twist a\nSema:\n\\twist a = a\n",
        )
        .unwrap();
        fs::write(
            dir.join("D_B.qbit"),
            "D.A.02. Use\n\\omega\nSema:\n\\omega = \\twist \\omega\n",
        )
        .unwrap();
        let mut conn = qubitick_db::open_in_memory().unwrap();
        ingest_corpus(&mut conn, &dir).unwrap();
        // The derived role is in the DB ...
        let role: Option<String> = conn
            .query_row("SELECT role FROM symbols WHERE tex = '\\twist'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(role.as_deref(), Some("prefix"), "user unary op -> prefix in DB");
        // ... and the parser uses it: `\twist \tau2` is an application.
        let rows = qubitick_db::queries::symbols_in_scope(&conn, "Era0/Strata0").unwrap();
        let ctx = qubitick_ctx::Ctx::from_rows("Era0/Strata0", rows);
        let e = qubitick_parse::parse_expr("\\twist \\omega", &ctx).unwrap();
        match e {
            qubitick_model::Expr::Apply(_, args) => assert_eq!(args.len(), 1, "unary"),
            other => panic!("expected an application from the DB role, got {other:?}"),
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn binder_kind_persisted_from_data_table() {
        // A user's `\\sum` binder persists binder_kind = Integral in the DB (the
        // binder-kind axis is DB-carried, not hardcoded in the parser).
        let dir = scratch_dir("bk");
        fs::write(
            dir.join("D_DEMO_01.qbit"),
            "D.A.01. Sum\n\\sum_{i : \\tau} a_i\nSema:\n\\sum_{i : \\tau} a_i = \\mathbb{1}\n",
        )
        .unwrap();
        let mut conn = qubitick_db::open_in_memory().unwrap();
        ingest_corpus(&mut conn, &dir).unwrap();
        // The elaboration rule IS recorded (the corpus declared a binder), and the
        // KIND is not: no glyph list decides whether a binder reads as a fold, a
        // limit or a plain `fun` — the parser reads that off the subscript's shape.
        let rule: String = conn
            .query_row(
                "SELECT rule FROM symbol_elab_rules WHERE rule = 'Binder' LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(rule, "Binder");
        let labelled: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM symbol_elab_rules WHERE binder_kind IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(labelled, 0, "no binder kind is a glyph's spelling");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn real_corpus_operators_carry_db_roles() {
        // Evidence the corpus DB is the source of truth for operator dispatch:
        // the real corpus derives prefix/infix/collector roles from the .qbit text.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap().parent().unwrap().join("project");
        if !root.join("Era0/Strata0").is_dir() { return; }
        let mut conn = qubitick_db::open_in_memory().unwrap();
        ingest_corpus(&mut conn, &root).unwrap();
        let count = |role: &str| -> i64 {
            conn.query_row("SELECT COUNT(*) FROM symbols WHERE role = ?1", [role], |r| r.get(0))
                .unwrap()
        };
        assert!(count("prefix") > 0, "corpus unary operators must get role=prefix");
        assert!(count("infix") > 0, "corpus binary operators must get role=infix");
        assert!(count("collector") > 0, "corpus bracket glyph must get role=collector");
        let forms: i64 = conn.query_row("SELECT COUNT(*) FROM form_bodies", [], |r| r.get(0)).unwrap();
        assert!(forms > 0, "form bodies persisted");
        let mods: i64 = conn.query_row("SELECT COUNT(*) FROM symbol_mod_roles", [], |r| r.get(0)).unwrap();
        assert!(mods > 0, "mod roles persisted");
    }

    #[test]
    fn plug_in_full_form_taxonomy() {
        // CAPSTONE: a user authors NEW axioms exercising every loader form, and the
        // engine ingests them with correct D-block metadata — zero engine edits.
        let dir = scratch_dir("taxonomy");
        let units: [(&str, &str); 6] = [
            // braced-arg macro
            ("D_A.qbit", "D.A.01. Frac\n\\frac{\\alpha}{\\beta}\nSema:\n\\frac{\\alpha}{\\beta} = \\mathbb{Q}\n"),
            // typed binder (Lambda-class big operator)
            ("D_B.qbit", "D.A.02. Big\n\\bigstar_{t : \\tau} A(t)\nSema:\n\\bigstar_{t : \\tau} A(t) = A\n"),
            // limit-style binder (arrow subscript)
            ("D_C.qbit", "D.A.03. Lim\n\\limes_{x \\to 0} f(x)\nSema:\n\\limes_{x \\to 0} f(x) = f\n"),
            // set literal
            ("D_D.qbit", "D.A.04. Set\n\\sigma\nSema:\n\\sigma = \\{ \\alpha, \\beta \\}\n"),
            // vector literal
            ("D_E.qbit", "D.A.05. Vec\n\\upsilon\nSema:\n\\upsilon = ( \\alpha, \\beta )\n"),
            // matrix + tensor index
            ("D_F.qbit", "D.A.06. Mat\n\\mu\nSema:\n\\mu = \\begin{matrix} \\alpha & \\beta \\\\ \\gamma & \\delta \\end{matrix}\n"),
        ];
        for (name, body) in units {
            fs::write(dir.join(name), body).unwrap();
        }
        let mut conn = qubitick_db::open_in_memory().unwrap();
        ingest_corpus(&mut conn, &dir).unwrap();

        // Every definition persisted a D-block elab rule.
        let elab: i64 = conn
            .query_row("SELECT COUNT(*) FROM symbol_elab_rules", [], |r| r.get(0))
            .unwrap();
        assert!(elab >= 6, "all six units persist elab, got {elab}");
        // A Binder elab for the two binder units.
        let binders: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM symbol_elab_rules WHERE rule = 'Binder'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(binders, 2, "both binder units -> elab=Binder, got {binders}");
        // Form bodies + mod roles persisted.
        let forms: i64 = conn
            .query_row("SELECT COUNT(*) FROM form_bodies", [], |r| r.get(0))
            .unwrap();
        assert!(forms >= 6, "form bodies persisted, got {forms}");
        // And every unit's Sema parses into a STRUCTURED Expr (no raw-Var fallback).
        let rows = qubitick_db::queries::symbols_in_scope(&conn, "Era0/Strata0").unwrap();
        let ctx = qubitick_ctx::Ctx::from_rows("Era0/Strata0", rows);
        for (stmt, want) in [
            ("\\sigma = \\{ \\alpha, \\beta \\}", "Set"),
            ("\\upsilon = ( \\alpha, \\beta )", "Vector"),
            (
                "\\mu = \\begin{matrix} \\alpha & \\beta \\\\ \\gamma & \\delta \\end{matrix}",
                "Matrix",
            ),
        ] {
            let e = qubitick_parse::parse_expr(stmt, &ctx).unwrap();
            let rhs = match &e {
                qubitick_model::Expr::Apply(h, args)
                    if h.primary == qubitick_model::EQ_SYM && args.len() == 2 =>
                {
                    &args[1]
                }
                _ => panic!("expected an equality, got {e:?}"),
            };
            let got = match rhs {
                qubitick_model::Expr::Set(_) => "Set",
                qubitick_model::Expr::Vector(_) => "Vector",
                qubitick_model::Expr::Matrix(_) => "Matrix",
                _ => "other",
            };
            assert_eq!(got, want, "rhs of `{stmt}` is {rhs:?}");
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn registers_braced_tensor_index_labels() {
        // `T_{jk}`: the multi-letter braced subscript `jk` is one index LABEL and
        // must be registered as a symbol from the .qbit text (data-driven), so the
        // lexer resolves it instead of rejecting a multi-letter bare identifier.
        let dir = scratch_dir("tensor");
        fs::write(
            dir.join("D_DEMO_01.qbit"),
            "D.DEMO.01. Tensor\nT_{jk}\nSema:\nT_{jk} = \\mathbb{1}\n",
        )
        .unwrap();
        let mut conn = qubitick_db::open_in_memory().unwrap();
        ingest_corpus(&mut conn, &dir).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM symbols WHERE tex = 'jk'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "index label `jk` must be registered as a symbol");
        // And the definition's Sema must PARSE with a real subscript mod, not
        // degrade to a raw Var: the whole point of registering the label.
        let ctx = qubitick_db::queries::symbols_in_scope(&conn, "Era0/Strata0").unwrap();
        let ctx = qubitick_ctx::Ctx::from_rows("Era0/Strata0", ctx);
        let e = qubitick_parse::parse_expr("T_{jk}", &ctx).unwrap();
        match e {
            qubitick_model::Expr::Sym(ac) => {
                let sub = ac.mods.r_sub.expect("subscript mod on T");
                assert!(sub.primary != 0, "subscript must resolve to the `jk` symbol");
            }
            other => panic!("expected Sym with a subscript mod, got {other:?}"),
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn derives_operator_role_from_sema_arity() {
        // A user's own unary operator definition must get role=prefix from the DB
        // (no engine edit); a binary one gets role=infix.
        let dir = scratch_dir("role");
        fs::write(
            dir.join("D_DEMO_01_Un.qbit"),
            "D.DEMO.01. Un\n\\myop a\nSema:\n\\myop a = a \\Delta a\n",
        )
        .unwrap();
        fs::write(
            dir.join("D_DEMO_02_Bin.qbit"),
            "D.DEMO.02. Bin\n a \\mybin b\nSema:\n a \\mybin b = a \\Delta b\n",
        )
        .unwrap();
        let mut conn = qubitick_db::open_in_memory().unwrap();
        ingest_corpus(&mut conn, &dir).unwrap();
        let roles: Vec<(String, Option<String>)> = {
            let mut stmt = conn
                .prepare("SELECT tex, role FROM symbols WHERE tex IN ('\\myop','\\mybin') ORDER BY tex")
                .unwrap();
            let it = stmt
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)))
                .unwrap();
            it.map(|r| r.unwrap()).collect()
        };
        let un = roles.iter().find(|(t, _)| t == "\\myop").and_then(|(_, r)| r.clone());
        let bin = roles.iter().find(|(t, _)| t == "\\mybin").and_then(|(_, r)| r.clone());
        assert_eq!(un.as_deref(), Some("prefix"), "unary op -> prefix, got {un:?}");
        assert_eq!(bin.as_deref(), Some("infix"), "binary op -> infix, got {bin:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_declaration_states_the_fixity_and_nothing_else_does() {
        // A corpus declares `times a` — one operand after the glyph, so the glyph is a
        // PREFIX operator. The engine carries no table that could call it binary, and
        // the declaration is the whole answer: the parser reads `prefix`, and no
        // BinOp facet is set for it.
        let dir = scratch_dir("prefix_wins");
        fs::write(
            dir.join("D_A_01_TimesPrefix.qbit"),
            "D.A.01. TimesPrefix\ntimes a\nType: R to R\n",
        )
        .unwrap();
        fs::write(
            dir.join("T_A_01_Use.qbit"),
            "T.A.01. Use\ntimes x = x\n",
        )
        .unwrap();
        let mut conn = qubitick_db::open_in_memory().unwrap();
        ingest_corpus(&mut conn, &dir).unwrap();

        // The declared symbol `times` must be in the symbols table with role=prefix.
        // Its facets must NOT have BinOp (prefix is not a binary operator).
        let syms = qubitick_db::queries::symbols_in_scope(&conn, "Era0/Strata0").unwrap();
        let times = syms.iter().find(|r| r.tex == "times").cloned();
        assert!(times.is_some(), "prefix operator `times` must be registered");
        let times = times.unwrap();
        assert_eq!(times.role.as_deref(), Some("prefix"), "`times` must have role=prefix, not infix");
        // BinOp facet must be cleared for prefix (the declaration wins over the
        // builtin table that says \times is BinOp).
        let facets = times.facets;
        let has_binop = facets & (1 << (qubitick_model::corpus::Facet::BinOp as u8)) != 0;
        assert!(!has_binop, "prefix operator must NOT have BinOp facet — declaration wins over builtin table");

        // The theorem's statement must parse as an application of prefix `times`.
        let ctx = qubitick_ctx::Ctx::from_rows("Era0/Strata0", syms);
        let e = qubitick_parse::parse_expr("times x", &ctx).unwrap();
        match e {
            qubitick_model::Expr::Apply(h, args) => {
                assert_eq!(h.primary, times.id as u32, "parsed head must be the `times` symbol");
                assert_eq!(args.len(), 1, "prefix application has one arg");
            }
            other => panic!("expected Apply(times, [x]), got {other:?}"),
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn plug_in_forms_roundtrip() {
        // A user authors NEW axioms using the data-driven forms (braced-arg macro,
        // binder, set, vector, matrix); the engine stores them with NO special case.
        let dir = scratch_dir("plug");
        fs::write(
            dir.join("D_DEMO_01.qbit"),
            "D.DEMO.01. Frac\n\\frac{\\alpha}{\\beta}\nSema:\n\\frac{\\alpha}{\\beta} = \\mathbb{Q}\n",
        )
        .unwrap();
        fs::write(
            dir.join("D_DEMO_02.qbit"),
            "D.DEMO.02. Uses\n\\iota\nSema:\n\\iota = \\frac{\\alpha}{\\beta} \\cup \\{ \\alpha, \\beta \\}\n",
        )
        .unwrap();
        fs::write(
            dir.join("D_DEMO_03.qbit"),
            "D.DEMO.03. Matrix\n\\mathbb{M}\nSema:\n\\mathbb{M} = \\begin{matrix} a & b \\\\ c & d \\end{matrix}\n",
        )
        .unwrap();
        let mut conn = qubitick_db::open_in_memory().unwrap();
        ingest_corpus(&mut conn, &dir).unwrap();
        let elab: i64 = conn.query_row("SELECT COUNT(*) FROM symbol_elab_rules", [], |r| r.get(0)).unwrap();
        assert!(elab >= 1, "braced-arg macro persists elab, got {elab}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn persists_dblock_elab_rule() {
        let dir = scratch_dir("elab");
        fs::write(
            dir.join("D_DEMO_01_Frac.qbit"),
            "D.DEMO.01. DemoFrac\n\\frac{\\alpha}{\\beta}\nSema:\n\\frac{\\alpha}{\\beta} = \\mathbb{Q}\n",
        )
        .unwrap();
        let mut conn = qubitick_db::open_in_memory().unwrap();
        ingest_corpus(&mut conn, &dir).unwrap();
        // The D-block ElabRule for the declared braced-arg macro must be stored.
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM symbol_elab_rules", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "one definition persists its elab rule");
        let row: (String, Option<String>) = conn
            .query_row(
                "SELECT rule, apply_form FROM symbol_elab_rules LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(row.0, "Apply", "braced macro elaborates via Apply");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ingests_calibrations() {
        let dir = scratch_dir("cal");
        fs::write(
            dir.join("S_1_P1.qbit"),
            "S.1. P1\nSymbol: \\mathfrak{h}\nQUnits: \\mathfrak{q}^{-1/2}\nDims: [h]\nRate: c=1\n",
        )
        .unwrap();
        let mut conn = qubitick_db::open_in_memory().unwrap();
        ingest_corpus(&mut conn, &dir).unwrap();
        let (sym, qu): (String, String) =
            conn.query_row("SELECT symbol, q_units FROM calibrations", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(sym, "\\mathfrak{h}");
        assert!(qu.contains("\\mathfrak{q}"), "q_units stored");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ingests_proof_chains() {
        // A theorem with a `Proof:` chain should populate proof_steps, chain_rows
        // and chain_rewrite_refs (DataSchema §7).
        let dir = scratch_dir("proof");
        fs::write(
            dir.join("T_BT_01_DeltaComm.qbit"),
            "T.BT.01. DeltaComm\n\\forall a b : \\mathfrak{E}, a \\Delta b = b \\Delta a\nProof:\na \\Delta b\n  = b \\Delta a    [T.BT.02]\n",
        )
        .unwrap();
        let mut conn = qubitick_db::open_in_memory().unwrap();
        ingest_corpus(&mut conn, &dir).unwrap();

        let steps: i64 = conn
            .query_row("SELECT COUNT(*) FROM proof_steps", [], |r| r.get(0))
            .unwrap();
        let chains: i64 = conn
            .query_row("SELECT COUNT(*) FROM chain_rows", [], |r| r.get(0))
            .unwrap();
        let refs: i64 = conn
            .query_row("SELECT COUNT(*) FROM chain_rewrite_refs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(steps, 1, "one Chain proof step");
        assert_eq!(chains, 1, "one chain row");
        assert_eq!(refs, 1, "one rewrite ref (T.BT.02)");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scopes_nodes_and_symbols_by_era_strata() {
        // A second era under project/Era1/Strata1 must get its own node era/strata
        // and scope its symbols there; `=` is global; symbols do not leak across eras.
        let dir = scratch_dir("scoped");
        fs::create_dir_all(dir.join("Era0/Strata0/Axioms")).unwrap();
        fs::create_dir_all(dir.join("Era1/Strata1/Axioms")).unwrap();
        fs::write(
            dir.join("Era0/Strata0/Axioms/A1_Realm.qbit"),
            "A1. Realm\n\\forall a b : \\mathfrak{E}, a \\Delta b = b \\Delta a\n",
        )
        .unwrap();
        fs::write(
            dir.join("Era1/Strata1/Axioms/A1_Qubit.qbit"),
            "A1. Qubit\n\\mathsf{T} = \\mathsf{T}\n",
        )
        .unwrap();

        let mut conn = qubitick_db::open_in_memory().unwrap();
        ingest_corpus(&mut conn, &dir).unwrap();

        let nodes = qubitick_db::read::get_node_list(&conn).unwrap();
        assert!(nodes.iter().any(|n| n.era == "Era0" && n.strata == 0), "Era0/Strata0 node");
        assert!(nodes.iter().any(|n| n.era == "Era1" && n.strata == 1), "Era1/Strata1 node");

        // Per-file symbol scoping: the Era1 axiom's glyph is scoped to Era1/Strata1
        // and is NOT visible from Era0; `=` is global so every era can use it.
        let s0 = qubitick_db::queries::symbols_in_scope(&conn, "Era0/Strata0").unwrap();
        assert!(s0.iter().any(|r| r.tex == "\\mathfrak{E}"));
        assert!(!s0.iter().any(|r| r.tex == "\\mathsf{T}"), "Era1 glyph not visible in Era0");

        let s1 = qubitick_db::queries::symbols_in_scope(&conn, "Era1/Strata1").unwrap();
        assert!(s1.iter().any(|r| r.tex == "\\mathsf{T}"), "Era1 glyph visible in its own scope");
        assert!(s1.iter().any(|r| r.tex == "=" && r.scope.is_empty()), "'=' is global scope");
        assert!(!s1.iter().any(|r| r.tex == "\\mathfrak{E}"), "Era0 glyph not visible in Era1");

        let _ = fs::remove_dir_all(&dir);
    }

    /// THE APP'S CONTEXT IS THE CORPUS, NOT A STRATUM (`crates/api/src/state.rs::open_project`).
    ///
    /// A project is the user's to organise. These two nodes sit in `Era0/Strata1`, and a context
    /// built for `Era0/Strata0` — the label the server used to build its one context from — knows
    /// exactly one symbol of them (`=`, which is global). Measured on the repo's own corpus that
    /// difference is 217 symbols against 1, and the interface lists its symbols and parses
    /// buffers from this context, so a corpus in any other stratum was unreadable to the UI while
    /// building perfectly. What a FILE sees is the scope question, asked per file by the build.
    #[test]
    fn a_corpus_outside_the_base_scope_is_still_the_apps_whole_context() {
        let dir = scratch_dir("app-context");
        fs::create_dir_all(dir.join("Era0/Strata1/Axioms")).unwrap();
        fs::create_dir_all(dir.join("Era0/Strata1/Definitions")).unwrap();
        fs::write(
            dir.join("Era0/Strata1/Axioms/A1_Base.qbit"),
            "A1. Base\n\\forall a b : \\mathfrak{E}, a \\Delta b = b \\Delta a\na \\Delta \\varnothing = a\n",
        )
        .unwrap();
        fs::write(
            dir.join("Era0/Strata1/Definitions/D_01_Wedge.qbit"),
            "D.X.01. Wedge\na \\wedge b\nSema:\na \\wedge b = \\mathbb{1}\n",
        )
        .unwrap();

        let mut conn = qubitick_db::open_in_memory().unwrap();
        ingest_corpus(&mut conn, &dir).unwrap();

        /* the scope-shaped question, which is what the build asks: nothing of Strata1 is visible
           from the base scope but the global `=` */
        let narrow = qubitick_db::queries::symbols_in_scope(&conn, "Era0/Strata0").unwrap();
        assert!(
            !narrow.iter().any(|r| r.tex == "\\wedge"),
            "a Strata1 glyph is not visible from Era0/Strata0"
        );
        assert!(narrow.iter().all(|r| r.scope.is_empty()), "{narrow:?}");

        /* the corpus-shaped question, which is what the INTERFACE asks */
        let ctx = qubitick_ctx::Ctx::from_rows(
            "Era0/Strata0",
            qubitick_db::queries::symbols_all(&conn).unwrap(),
        );
        for tex in ["\\wedge", "\\mathfrak{E}", "\\Delta", "\\varnothing"] {
            assert!(ctx.by_tex(tex).is_some(), "the app's context knows {tex}");
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_root_is_ok() {
        let dir = scratch_dir("empty");
        let mut conn = qubitick_db::open_in_memory().unwrap();
        let report = ingest_corpus(&mut conn, &dir).unwrap();
        assert_eq!(report.nodes, 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn punctuation_operator_declares_and_gets_infix_role() {
        // A corpus can declare a bare punctuation operator (e.g. `+`) in a glyph line
        // `a + b` and the engine must recognise it as the declared symbol, register it,
        // and derive role=infix from the two-operand shape.
        let dir = scratch_dir("punct_op");
        fs::write(
            dir.join("D_A_01_Add.qbit"),
            "D.A.01. Add\na + b\nType: R to R to R\n",
        )
        .unwrap();
        fs::write(
            dir.join("T_A_01_Use.qbit"),
            "T.A.01. Use\nx + y = y + x\n",
        )
        .unwrap();
        let mut conn = qubitick_db::open_in_memory().unwrap();
        let report = ingest_corpus(&mut conn, &dir).unwrap();
        assert_eq!(report.nodes, 2, "one definition + one theorem");

        // The declared symbol `+` must be in the symbols table with role=infix.
        let syms = qubitick_db::queries::symbols_in_scope(&conn, "Era0/Strata0").unwrap();
        let plus = syms.iter().find(|r| r.tex == "+").cloned();
        assert!(plus.is_some(), "punctuation operator `+` must be registered");
        let plus = plus.unwrap();
        assert_eq!(plus.role.as_deref(), Some("infix"), "`+` must have role=infix");
        // Lean name must be a valid identifier (not empty, not a keyword).
        assert!(!plus.lean_name.is_empty(), "lean_name must not be empty");
        assert!(!LEAN_KEYWORDS.contains(&plus.lean_name.as_str()), "lean_name must not be a Lean keyword");

        // The theorem's statement must parse as an application of `+` (not raw text).
        let ctx = qubitick_ctx::Ctx::from_rows("Era0/Strata0", syms);
        let e = qubitick_parse::parse_expr("x + y", &ctx).unwrap();
        match e {
            qubitick_model::Expr::Apply(h, args) => {
                assert_eq!(h.primary, plus.id as u32, "parsed head must be the `+` symbol");
                assert_eq!(args.len(), 2, "infix application has two args");
            }
            other => panic!("expected Apply(plus, [x, y]), got {other:?}"),
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn punctuation_alphabet_excludes_structural_chars() {
        // A corpus text with structural punctuation must register only the
        // OPERATOR_PUNCT characters (+, =) and NOT brackets, comma, period, or semicolon.
        // TRAP: a bracket registered as a symbol is a bracket the parser may stop
        // treating as grouping.
        let dir = scratch_dir("punct_alphabet");
        fs::write(
            dir.join("D_A_01_Test.qbit"),
            "D.A.01. Test\n(a + b), c = d.\nType: R to R to R\n",
        )
        .unwrap();
        let mut conn = qubitick_db::open_in_memory().unwrap();
        ingest_corpus(&mut conn, &dir).unwrap();

        let syms = qubitick_db::queries::symbols_in_scope(&conn, "Era0/Strata0").unwrap();
        let texes: Vec<&str> = syms.iter().map(|r| r.tex.as_str()).collect();

        // Operator punctuation that SHOULD be registered
        assert!(texes.contains(&"+"), "operator `+` must be registered");
        assert!(texes.contains(&"="), "operator `=` must be registered");

        // Structural punctuation that must NOT be registered
        assert!(!texes.contains(&"("), "bracket `(` must NOT be registered — trap: a bracket registered as a symbol is a bracket the parser may stop treating as grouping");
        assert!(!texes.contains(&")"), "bracket `)` must NOT be registered");
        assert!(!texes.contains(&","), "comma `,` must NOT be registered");
        assert!(!texes.contains(&"."), "period `.` must NOT be registered");
        assert!(!texes.contains(&";"), "semicolon `;` must NOT be registered");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn glyph_line_with_bracket_middle_token_does_not_declare_bracket() {
        // A glyph line whose middle token is a bracket run (e.g. "a ( b") must NOT
        // declare "(" as the head. The 3-token infix heuristic must only accept
        // OPERATOR_PUNCT characters as the middle token.
        let head = head_control_word("a ( b");
        assert!(head.is_none() || head.as_deref() != Some("("),
            "head_control_word must not return a bracket for 'a ( b' — brackets are structural, not operators");

        // Also test that a valid operator middle token still works
        let head = head_control_word("a + b");
        assert_eq!(head.as_deref(), Some("+"), "head_control_word must return `+` for 'a + b'");

        let head = head_control_word("a = b");
        assert_eq!(head.as_deref(), Some("="), "head_control_word must return `=` for 'a = b'");
    }
}