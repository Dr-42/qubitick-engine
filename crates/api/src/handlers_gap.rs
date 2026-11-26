//! Gap handlers — the generator-demanded endpoints (generator_demands.md).
//!
//! Thin `request → engine → JSON` wrappers over existing crates, plus honest
//! placeholders for engines not yet implemented (goal-at-cursor, LaTeX
//! typesetting, induction, expansion, handwriting OCR, slate evaluation).
//! Also mounts real websocket streams for `ws.check` / `ws.slate` / `ws.build`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::respond::{bad_request, internal, not_found, unprocessable};
use crate::state::AppState;
use qubitick_model::Phase;

// ---- request bodies ----

#[derive(Deserialize)]
pub struct PathQuery {
    pub path: Option<String>,
}

#[derive(Deserialize)]
pub struct FileSaveReq {
    pub path: String,
    pub content: String,
}

#[derive(Deserialize)]
pub struct TextReq {
    pub content: String,
    /// The caret's 1-based line, when the caller has one: the goal is the one AT a position.
    #[serde(default)]
    pub line_no: Option<u32>,
}

#[derive(Deserialize)]
pub struct SymbolAddReq {
    pub tex: String,
    #[serde(default)]
    pub lean: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub display: Option<String>,
    /// The corpus file the symbol is being added FOR. A symbol is read where it is written, so
    /// this decides its scope (`pipeline::scope_of_path`); without it the symbol goes in the
    /// engine's default scope for a path with no era in it, which is what `Era0/Strata0` is.
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Deserialize)]
pub struct CiteQuery {
    /// A fragment of a node's NAME: the old search over the node list.
    pub fragment: Option<String>,
    /// A citation as the corpus writes it (`A1.4`, `T.BT.01`, `D.2`): resolved to the node it
    /// names. This is what a hyperlink needs, and it is a different question from the search.
    pub name: Option<String>,
}

/// Reject path escapes; return the file path joined under the project root.
fn project_join(root: &Path, rel: &str) -> Option<PathBuf> {
    let rel = rel.trim_start_matches('/');
    if rel.split(['/', '\\']).any(|c| c == "..") {
        return None;
    }
    Some(root.join(rel))
}

// ---- /api/file ----

/// `GET /api/file?path=` — load a single `.qbit` file.
pub async fn get_file(
    State(state): State<Arc<AppState>>,
    Query(q): Query<PathQuery>,
) -> Response {
    let Some(rel) = q.path else {
        return bad_request(Phase::Store, "server.missing-path", "missing `path`");
    };
    let Some(path) = project_join(&state.corpus(), &rel) else {
        return bad_request(Phase::Store, "server.illegal-path", "illegal path");
    };
    match std::fs::read_to_string(&path) {
        Ok(content) => Json(json!({ "ok": true, "path": rel, "content": content })).into_response(),
        Err(e) => not_found(Phase::Store, "store.read-failed", e.to_string()),
    }
}

/// `POST /api/file` — persist an HL buffer to disk.
pub async fn post_file(
    State(state): State<Arc<AppState>>,
    Json(req): Json<FileSaveReq>,
) -> Response {
    let Some(path) = project_join(&state.corpus(), &req.path) else {
        return bad_request(Phase::Store, "server.illegal-path", "illegal path");
    };
    match std::fs::write(&path, &req.content) {
        Ok(()) => {
            /* A SAVE INJECTS THE FILE'S SYMBOLS, so the engine can parse the notation it declares
               without rebuilding the ledger (crates/build/src/ingest.rs::ingest_file_symbols). The
               corpus is NOT re-ingested here: that walk re-parses every file and renumbers every node
               (a node's Godel code IS its ledger ordinal) and is `POST /api/build`'s job — measured at
               ~10 s for 330 files in a debug build.
               A failure to inject does not fail the SAVE: the file is on disk, and the answer says what
               happened rather than pretending the save was the whole story. */
            let mut symbols = 0usize;
            let mut in_ledger = false;
            let mut warn = None;
            {
                let conn = state.db.lock().unwrap();
                match qubitick_build::ingest_file_symbols(&conn, &state.corpus(), &path) {
                    Ok(r) => {
                        symbols = r.written;
                        in_ledger = r.in_ledger;
                    }
                    Err(e) => warn = Some(e.to_string()),
                }
            }
            let mut body = json!({
                "ok": true,
                "path": req.path,
                // rows written for this file's glyphs; 0 with inLedger=false means the file has no
                // node row yet (a build is what gives it one)
                "symbols": symbols,
                "inLedger": in_ledger,
            });
            if let Some(w) = warn {
                body["warn"] = json!(w);
            }
            Json(body).into_response()
        }
        Err(e) => internal("store.write-failed", e.to_string()),
    }
}

// ---- parse/AST/IR/HL ----

/// `POST /api/ast` — parse content into a structured AST.
pub async fn post_ast(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TextReq>,
) -> Response {
    match qubitick_parse::parse_content(&state.ctx.read().unwrap(), &req.content) {
        Ok(nodes) => {
            let kinds: Vec<&str> = nodes.iter().map(|n| n.base().kind.as_str()).collect();
            Json(json!({
                "ok": true,
                "nodes": nodes.len(),
                "kinds": kinds,
                "ast": format!("{nodes:?}"),
            }))
            .into_response()
        }
        Err(e) => unprocessable(Phase::Parse, "parse.failed", e.to_string()),
    }
}

/// `GET /api/ir?path=` — parse HL into its internal IR.
pub async fn get_ir(
    State(state): State<Arc<AppState>>,
    Query(q): Query<PathQuery>,
) -> Response {
    let Some(rel) = q.path else {
        return bad_request(Phase::Store, "server.missing-path", "missing `path`");
    };
    let Some(path) = project_join(&state.corpus(), &rel) else {
        return bad_request(Phase::Store, "server.illegal-path", "illegal path");
    };
    match std::fs::read_to_string(&path) {
        Ok(src) => match qubitick_parse::parse_content(&state.ctx.read().unwrap(), &src) {
            Ok(nodes) => Json(json!({ "ok": true, "path": rel, "ir": format!("{nodes:?}") }))
                .into_response(),
            Err(e) => unprocessable(Phase::Parse, "parse.failed", e.to_string()),
        },
        Err(e) => not_found(Phase::Store, "store.read-failed", e.to_string()),
    }
}

/// `POST /api/ir` — emit IR → HL. (Round-trips the end-user surface best-effort.)
pub async fn post_ir(Json(req): Json<TextReq>) -> Json<Value> {
    Json(json!({
        "ok": true,
        "hl": req.content, // IR→HL surface emitter is a follow-on; echo the buffer.
        "note": "IR→HL emitter not yet implemented; buffer echoed"
    }))
}

/// `POST /api/hl` — higher-level emission (here: emitted Lean for the buffer).
pub async fn post_hl(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TextReq>,
) -> Json<Value> {
    match qubitick_parse::parse_content(&state.ctx.read().unwrap(), &req.content) {
        Ok(nodes) => match qubitick_emit::emit_lean_text(&state.ctx.read().unwrap(), &nodes, "Qubitick.HL", &mut qubitick_rewrite::rules::RuleTable::new()) {
            Ok(lean) => Json(json!({ "ok": true, "lean": lean })),
            Err(e) => Json(json!({ "ok": false, "error": e.to_string() })),
        },
        Err(e) => Json(json!({ "ok": false, "error": e.to_string() })),
    }
}

// ---- /api/lean ----

/// `GET /api/lean?path=` — emitted Lean for a node/file.
pub async fn get_lean(
    State(state): State<Arc<AppState>>,
    Query(q): Query<PathQuery>,
) -> Response {
    let Some(rel) = q.path else {
        return (axum::http::StatusCode::BAD_REQUEST, "missing `path`").into_response();
    };
    let Some(path) = project_join(&state.corpus(), &rel) else {
        return (axum::http::StatusCode::BAD_REQUEST, "illegal path").into_response();
    };
    match std::fs::read_to_string(&path) {
        Ok(src) => match qubitick_parse::parse_content(&state.ctx.read().unwrap(), &src) {
            Ok(nodes) => match qubitick_emit::emit_lean_text(&state.ctx.read().unwrap(), &nodes, "Qubitick.HL", &mut qubitick_rewrite::rules::RuleTable::new()) {
                Ok(lean) => Json(json!({ "ok": true, "path": rel, "lean": lean })).into_response(),
                Err(e) => (axum::http::StatusCode::BAD_REQUEST, Json(json!({ "ok": false, "error": e.to_string() }))).into_response(),
            },
            Err(e) => (axum::http::StatusCode::BAD_REQUEST, Json(json!({ "ok": false, "error": e.to_string() }))).into_response(),
        },
        Err(e) => (axum::http::StatusCode::NOT_FOUND, Json(json!({ "ok": false, "error": e.to_string() }))).into_response(),
    }
}

// ---- /api/graph ----

/// `GET /api/graph` — corpus DAG as nodes + edges (from the ledger DB).
pub async fn get_graph(State(state): State<Arc<AppState>>) -> Json<Value> {
    let conn = state.db.lock().unwrap();
    let nodes = qubitick_db::read::get_node_list(&conn).unwrap_or_default();
    let node_rows: Vec<Value> = nodes
        .iter()
        .map(|n| {
            json!({
                "id": n.id,
                "name": n.name,
                "kind": n.node_type.as_str(),
                "godel": n.godel,
            })
        })
        .collect();
    let edges: Vec<Value> = match conn.prepare("SELECT source_node_id, target_node_id FROM node_req") {
        Ok(mut stmt) => match stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))) {
            Ok(rows) => rows
                .filter_map(|r| r.ok())
                .map(|(source, target)| json!({ "source": source, "target": target }))
                .collect(),
            Err(_) => Vec::new(),
        },
        Err(_) => Vec::new(),
    };
    drop(conn);
    Json(json!({ "ok": true, "nodes": node_rows, "edges": edges }))
}

// ---- /api/cite, /api/citations ----

fn node_matches(conn: &qubitick_db::Db, fragment: &str) -> Vec<Value> {
    let nodes = qubitick_db::read::get_node_list(conn).unwrap_or_default();
    let frag = fragment.to_lowercase();
    nodes
        .iter()
        .filter(|n| n.name.to_lowercase().contains(&frag))
        .map(|n| json!({ "id": n.id, "name": n.name, "kind": n.node_type.as_str(), "godel": n.godel }))
        .collect()
}

/// `GET /api/cite?name=A1.4` — follow a citation to the node it names; `?fragment=` searches names.
///
/// TWO QUESTIONS, ONE ROUTE, because both are "what does this reference point at". A citation
/// (`A1.4`, `T.BT.01`) resolves through `crate::cite_target` — the same reading the build validates
/// with and the row check follows — and the answer carries the FILE, so a client can open it: the
/// corpus root is walked here rather than in the pane, because the engine is the side that knows
/// where the corpus is.
///
/// `\equiv` and `^i` are LOCAL laws (the definitional step, the induction hypothesis): they name
/// no node, and the answer says so instead of returning nothing.
pub async fn get_cite(
    State(state): State<Arc<AppState>>,
    Query(q): Query<CiteQuery>,
) -> Json<Value> {
    if let Some(name) = q.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        let headers = {
            let conn = state.db.lock().unwrap();
            qubitick_db::read::get_node_list(&conn).unwrap_or_default()
        };
        let ctx = state.ctx.read().unwrap();
        if qubitick_build::is_local(name) {
            return Json(json!({
                "ok": true,
                "name": name,
                "target": null,
                "local": true,
                "note": "a local law: the definitional step or the induction hypothesis — the corpus has no node for it",
            }));
        }
        let target = qubitick_build::resolve_cite(&ctx, &headers, name);
        drop(ctx);
        return Json(match target {
            Some(t) => {
                /* WHERE THE FILE IS. A node header carries the path the ingester read — an
                   absolute one in this build — so the corpus-relative path is the prefix stripped,
                   and a corpus whose headers carry a bare name falls back to the walk. The client
                   needs the corpus-relative form: that is what `openFile` opens. */
                let corpus = state.corpus();
                let as_path = std::path::PathBuf::from(&t.filename);
                let relative = as_path
                    .strip_prefix(&corpus)
                    .ok()
                    .map(|r| r.display().to_string())
                    .or_else(|| {
                        qubitick_build::walk_qbit(&corpus)
                            .into_iter()
                            .find(|p| {
                                p.file_name()
                                    .map(|f| f.to_string_lossy() == as_path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default())
                                    .unwrap_or(false)
                            })
                            .and_then(|p| p.strip_prefix(&corpus).ok().map(|r| r.display().to_string()))
                    });
                let basename = as_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| t.filename.clone());
                json!({
                    "ok": true,
                    "name": name,
                    "target": {
                        "label": t.label,
                        "kind": t.kind,
                        "node": t.node,
                        "number": t.number,
                        "statement": t.statement,
                        "sector": t.sector,
                        "file": basename,
                        /* the corpus-relative path, so a client can open it; null when neither
                           the prefix nor the walk can place the file */
                        "path": relative,
                    },
                })
            }
            None => json!({
                "ok": true,
                "name": name,
                "target": null,
                "note": "this citation names no node this scope can see",
            }),
        });
    }
    let conn = state.db.lock().unwrap();
    let fragment = q.fragment.unwrap_or_default();
    let matches = node_matches(&conn, &fragment);
    drop(conn);
    Json(json!({ "ok": true, "fragment": fragment, "matches": matches }))
}

/// `GET /api/citations` — list all citation candidates.
pub async fn get_citations(State(state): State<Arc<AppState>>) -> Json<Value> {
    let conn = state.db.lock().unwrap();
    let list = node_matches(&conn, "");
    drop(conn);
    Json(json!({ "ok": true, "citations": list }))
}

// ---- /api/symbols (add) ----

/// `POST /api/symbols` — register a new symbol in the corpus DB.
pub async fn post_symbols(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SymbolAddReq>,
) -> Response {
    /* THE SYMBOL'S SCOPE IS THE FILE'S, not the engine's idea of where a corpus starts. `Era0/
       Strata0` was hard-coded here for both the hash and the row, so a symbol added while working
       in `Era1/Strata3` landed in a scope its own file cannot see — a notation that vanishes.
       `scope_of_path` is the same rule ingest uses to place every other symbol. */
    let scope = symbol_scope(&state.corpus(), req.path.as_deref());
    let hash = qubitick_db::hash::hash_symbol(&scope, &req.tex, &req.kind.clone().unwrap_or_else(|| "notation".into()));
    let conn = state.db.lock().unwrap();
    // Symbols require a valid home_node FK; bind the scratch symbol to the first
    // existing ledger node (1 when the corpus is populated).
    let home_node: i64 = conn
        .query_row("SELECT COALESCE(MIN(id), 1) FROM nodes", [], |r| r.get(0))
        .unwrap_or(1);
    let res = conn.execute(
        "INSERT INTO symbols (scope, tex, display, lean_name, kind, home_node, symbol_hash)
         VALUES (?7, ?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            req.tex,
            req.display.clone().unwrap_or_else(|| req.tex.clone()),
            req.lean.clone().unwrap_or_else(|| req.tex.clone()),
            req.kind.clone().unwrap_or_else(|| "notation".into()),
            home_node,
            hash.to_le_bytes().to_vec(),
            scope
        ],
    );
    drop(conn);
    match res {
        Ok(_) => Json(json!({ "ok": true })).into_response(),
        Err(e) => (axum::http::StatusCode::CONFLICT, Json(json!({ "ok": false, "error": e.to_string() }))).into_response(),
    }
}

/// The scope a symbol added for `rel` belongs to. `Era0/Strata0` was hard-coded here for both the
/// hash and the row, so a symbol added while working in `Era1/Strata3` landed in a scope its own
/// file cannot see — a notation that vanishes. `scope_of_path` is the rule ingest already uses to
/// place every other symbol; the fallback is the engine's default for a path with no era in it,
/// which is what a project with no `EraN/StrataM` directories has.
fn symbol_scope(corpus: &Path, rel: Option<&str>) -> String {
    rel.and_then(|r| project_join(corpus, r))
        .map(|p| qubitick_build::pipeline::scope_of_path(&p))
        .unwrap_or_else(|| qubitick_build::pipeline::scope_of_path(corpus))
}

// ---- /api/diagnostics ----

/// `GET /api/diagnostics?path=` — structured build/Lean diagnostics for a file,
/// or all corpus files when no path is given.
pub async fn get_diagnostics(
    State(state): State<Arc<AppState>>,
    Query(q): Query<PathQuery>,
) -> Json<Value> {
    let ground = match ground_of(&state) {
        Ok(g) => g,
        Err(e) => return Json(json!({ "ok": false, "error": e, "diagnostics": [] })),
    };
    let entries: Vec<Value> = if let Some(rel) = q.path {
        project_join(&state.corpus(), &rel)
            .map(|p| build_one(&state, &p.to_string_lossy(), Some(&ground)))
            .map(|s| {
                serde_json::from_str(&s).unwrap_or_else(|_| json!({ "path": rel, "ok": false }))
            })
            .into_iter()
            .collect()
    } else {
        walk_qbit(&state.corpus())
            .into_iter()
            .map(|p| {
                let s = build_one(&state, &p, Some(&ground));
                serde_json::from_str(&s).unwrap_or_else(|_| json!({ "path": p, "ok": false }))
            })
            .collect()
    };
    Json(json!({ "ok": true, "diagnostics": entries }))
}


// ---- slate / goal / latex / induce / expand / handwrite ----

/// `POST /api/slate` — evaluate a scratch slate (parse it).
pub async fn post_slate(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TextReq>,
) -> Json<Value> {
    match qubitick_parse::parse_content(&state.ctx.read().unwrap(), &req.content) {
        Ok(nodes) => {
            let kinds: Vec<&str> = nodes.iter().map(|n| n.base().kind.as_str()).collect();
            Json(json!({ "ok": true, "nodes": nodes.len(), "kinds": kinds }))
        }
        Err(e) => Json(json!({ "ok": false, "error": e.to_string() })),
    }
}

/// `POST /api/goal` — goal-at-cursor (engine pending; honest placeholder).
pub async fn post_goal(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TextReq>,
) -> Json<Value> {
    /* WHAT THE AUTHOR ASKED FOR, IN ONE ANSWER: the obligation at the caret, what remains of it, and
       what the corpus can do from where the proof stands. Every piece already existed as a question
       of its own — `POST /api/rows` (does the row follow), `qubitick_autocite` (what bridges two
       expressions), `POST /api/steps` (what can come next) — and a goal pane that asked three times
       per keystroke would be a goal pane nobody keeps open. */
    /* THE EQUATIONS FIRST, BEFORE TAKING THE DB LOCK. `state.rules()` needs the DB lock itself, so a
       handler that locks the DB and then asks for the rules waits on a mutex it is holding: the
       route deadlocks, and the only symptom is a test that never finishes (`std::sync::Mutex` does
       not re-enter). The rules are built once per project, so this order costs nothing. */
    let rules = state.rules().ok();
    let conn = state.db.lock().unwrap();
    let ctx = state.ctx.read().unwrap();
    let nodes = match qubitick_parse::parse_content(&ctx, &req.content) {
        Ok(n) => n,
        Err(e) => {
            return Json(json!({
                "ok": false,
                "goal": null,
                "error": format!("{e}"),
            }));
        }
    };
    let Some(node) = nodes.first() else {
        return Json(json!({ "ok": false, "goal": null, "error": "the buffer holds no node" }));
    };
    let header_line = req.content
        .lines()
        .position(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with('%') && !t.starts_with('#') && !t.starts_with("//")
        })
        .map(|i| i as u32 + 1)
        .unwrap_or(1);
    let show = |e: &qubitick_model::Expr| qubitick_autocite::show(&ctx, e);

    let (kind, name) = match node {
        qubitick_model::Node::Theorem(t) => ("theorem", t.base.name.clone()),
        qubitick_model::Node::Axiom(a) => ("axiom", a.base.name.clone()),
        qubitick_model::Node::Definition(d) => ("definition", d.base.name.clone()),
        other => ("node", other.base().name.clone()),
    };
    let qubitick_model::Node::Theorem(theorem) = node else {
        /* an axiom or a definition states things and proves nothing: its "obligation" is the
           statements themselves, and there is no proof state to report */
        return Json(json!({
            "ok": true,
            "goal": {
                "kind": kind,
                "node": name,
                "line": header_line,
                "statement": null,
                "current": null,
                "target": null,
                "verdict": null,
                "admitted": false,
                "note": "this node states; it has no proof to stand in",
            },
        }));
    };

    /* the statement as the corpus reads it: a two-sided equation, so the target is its right side
       and the obligation is its left */
    let (statement_lhs, statement_rhs) = match &theorem.statement {
        qubitick_model::Expr::Binder(_, _, body) => split_equation(body).unwrap_or((None, None)),
        other => split_equation(other).unwrap_or((None, None)),
    };

    /* WHERE THE PROOF STANDS: the right-hand side of the row the caret is on, or of the row above it
       when the caret is on a line that is not a row. The mapping from a line to a row is the same
       one the parser numbers by (the block's lines, from its first). */
    let proof_row = req
        .line_no
        .and_then(|line| row_at_line(&req.content, line));
    let mut current = statement_lhs.clone();
    if let Some(qubitick_model::ProofStep::Chain { rows }) = &theorem.proof.last() {
        if let Some(idx) = proof_row.and_then(|r| r.checked_sub(2)) {
            if let Some(row) = rows.get(idx as usize) {
                current = Some(row.rhs.clone());
            }
        } else if let Some(last) = rows.last() {
            current = Some(last.rhs.clone());
        }
    }
    let verdicts = qubitick_build::check_rows(&conn, &ctx, std::slice::from_ref(node));
    let verdict = proof_row
        .and_then(|r| verdicts.iter().find(|v| v.row == r))
        .map(|v| json!({
            "verdict": v.verdict,
            "cite": v.cite,
            "reaches": v.reaches.iter().map(&show).collect::<Vec<_>>(),
        }));

    /* WHAT REMAINS: whether the corpus can bridge where the proof stands to where the statement
       ends, with the chains it found — and, when it cannot, the differingleaves and the closest state
       (`gap`, `closest`), which is the half an author acts on */
    let mut remaining = json!({ "reached": false, "citations": Vec::<Value>::new(), "closest": null, "gap": null });
    if let (Some(cur), Some(target)) = (current.clone(), statement_rhs.clone()) {
        if !matches!(
            qubitick_rewrite::justify::justified(&cur, &target, None, &[]),
            qubitick_rewrite::justify::Verdict::No { .. }
        ) {
            remaining = json!({ "reached": true, "citations": Vec::<Value>::new(), "closest": null, "gap": null });
        } else if let Some(rules) = &rules {
            let chain = qubitick_autocite::find_with_rules(&cur, &target, rules);
            remaining = json!({
                "reached": chain.reached,
                "citations": chain.steps.iter().map(|s| json!({ "cite": s.cite, "expr": show(&s.expr) })).collect::<Vec<_>>(),
                "closest": chain.closest.as_ref().map(show),
                "gap": chain.gap,
            });
        }
    }

    /* WHAT APPLIES: the rewrites the corpus can reach from where the proof stands */
    let steps = match (current.clone(), rules.as_ref()) {
        (Some(cur), Some(rules)) => qubitick_autocite::rows::steps(&cur, rules, 6)
            .iter()
            .map(|s| json!({ "cite": s.cite, "expr": show(&s.expr) }))
            .collect::<Vec<Value>>(),
        _ => Vec::new(),
    };

    Json(json!({
        "ok": true,
        "goal": {
            "kind": kind,
            "node": name,
            "line": req.line_no.unwrap_or(header_line),
            "statement": statement_rhs.as_ref().or(statement_lhs.as_ref()).map(show),
            "current": current.as_ref().map(show),
            "target": statement_rhs.as_ref().map(show),
            "row": proof_row,
            "verdict": verdict,
            "remaining": remaining,
            "steps": steps,
            "admitted": theorem.proof.is_empty(),
        },
    }))
}

/// The two sides of an equation, when the expression is one.
fn split_equation(e: &qubitick_model::Expr) -> Option<(Option<qubitick_model::Expr>, Option<qubitick_model::Expr>)> {
    let qubitick_model::Expr::Apply(head, args) = e else { return None };
    if head.primary != qubitick_model::EQ_SYM || args.len() != 2 {
        return None;
    }
    Some((Some(args[0].clone()), Some(args[1].clone())))
}

/// Which proof row a 1-based file line is on: the parser numbers the block's lines from its first
/// (`ChainRow::line`), so the answer is how many lines into the block the caret sits.
fn row_at_line(content: &str, line_no: u32) -> Option<u32> {
    let lines: Vec<&str> = content.lines().collect();
    let proof = lines.iter().position(|l| l.trim() == "Proof:")? as u32 + 1;
    if line_no <= proof {
        return None;
    }
    let mut row = 0u32;
    for (i, raw) in lines.iter().enumerate().skip(proof as usize) {
        let t = raw.trim();
        if t.is_empty() {
            break;
        }
        row += 1;
        if (i as u32 + 1) == line_no {
            return Some(row);
        }
    }
    None
}

/// `POST /api/latex` — LaTeX typeset/preview (engine pending; honest placeholder).
pub async fn post_latex(Json(req): Json<TextReq>) -> Json<Value> {
    Json(json!({
        "ok": false,
        "error": "LaTeX typesetting not implemented yet",
        "source": req.content,
    }))
}

/// `POST /api/induce` — induce a theorem (engine pending; honest placeholder).
pub async fn post_induce(Json(req): Json<TextReq>) -> Json<Value> {
    Json(json!({
        "ok": false,
        "error": "theorem induction not implemented yet",
        "source": req.content,
    }))
}

/// `POST /api/expand` — expand/augment a goal (engine pending; honest placeholder).
pub async fn post_expand(Json(req): Json<TextReq>) -> Json<Value> {
    Json(json!({
        "ok": false,
        "error": "goal expansion not implemented yet",
        "source": req.content,
    }))
}

/// `POST /api/handwrite` — ink OCR upload (engine pending; honest placeholder).
pub async fn post_handwrite(Json(req): Json<TextReq>) -> Json<Value> {
    Json(json!({
        "ok": false,
        "error": "handwriting OCR not implemented yet",
        "received": req.content.len(),
    }))
}

// ---- websockets ----

/// `GET /ws/check` — per-file Lean check stream.
pub async fn ws_check(State(state): State<Arc<AppState>>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| check_stream(socket, state))
}

async fn check_stream(mut socket: WebSocket, state: Arc<AppState>) {
    while let Some(Ok(Message::Text(text))) = socket.recv().await {
        let path = text.trim().to_string();
        let out = match ground_of(&state) {
            Ok(ground) => build_one(&state, &path, Some(&ground)),
            Err(e) => json!({ "path": path, "ok": false, "verified": false, "error": e })
                .to_string(),
        };
        if socket
            .send(Message::Text(out.into()))
            .await
            .is_err()
        {
            break;
        }
    }
}

/// `GET /ws/build` — stream full build progress per file.
pub async fn ws_build(State(state): State<Arc<AppState>>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| build_stream(socket, state))
}

async fn build_stream(mut socket: WebSocket, state: Arc<AppState>) {
    // Collect all results synchronously (holding the build lock only here, so no
    // non-Send guard crosses an `.await`), then stream them.
    let (results, files) = {
        let _lock = state.build_lock.lock().unwrap();
        let files: Vec<std::path::PathBuf> = walk_qbit(&state.corpus())
            .into_iter()
            .map(std::path::PathBuf::from)
            .collect();
        /* the ground, before the first file: a corpus whose own `HL.lean` does not check fails
           here as one sentence rather than as one error per module it never wrote */
        match ground_of(&state) {
            Ok(ground) => (
                files
                    .iter()
                    .map(|f| build_one(&state, &f.to_string_lossy(), Some(&ground)))
                    .collect::<Vec<String>>(),
                files.len(),
            ),
            /* the corpus's file count is still the corpus's: a report that said 0 files would be
               a second, wrong statement about a corpus that has them */
            Err(e) => (
                vec![json!({ "ok": false, "verified": false, "error": e }).to_string()],
                files.len(),
            ),
        }
    };
    let mut built = 0usize;
    for out in &results {
        if socket.send(Message::Text(out.clone().into())).await.is_err() {
            return;
        }
        if out.contains("\"ok\":true") || out.contains("\"ok\": true") {
            built += 1;
        }
    }
    let _ = socket
        .send(Message::Text(
            json!({ "done": true, "built": built, "files": files }).to_string().into(),
        ))
        .await;
}

/// `GET /ws/slate` — stream slate evaluation results.
pub async fn ws_slate(State(state): State<Arc<AppState>>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| slate_stream(socket, state))
}

async fn slate_stream(mut socket: WebSocket, state: Arc<AppState>) {
    while let Some(Ok(Message::Text(text))) = socket.recv().await {
        let out = match qubitick_parse::parse_content(&state.ctx.read().unwrap(), &text) {
            Ok(nodes) => {
                let kinds: Vec<&str> = nodes.iter().map(|n| n.base().kind.as_str()).collect();
                json!({ "ok": true, "nodes": nodes.len(), "kinds": kinds }).to_string()
            }
            Err(e) => json!({
                "ok": false,
                "error": e.to_string(),
                "diagnostics": [serde_json::to_value(
                    qubitick_model::Diagnostic::error(Phase::Parse, "parse.failed", e.to_string())
                )
                .unwrap_or_default()],
            })
            .to_string(),
        };
        if socket.send(Message::Text(out.into())).await.is_err() {
            break;
        }
    }
}

/// The ground this corpus is checked against: the engine's base with the project's `LeanDir`
/// over it (`qubitick_build::ground::for_check`). Assembled once per request, not once per file:
/// it is the same ground for every file of a corpus, and assembling it per file would compile it
/// per file. `Err` is a REPORT here and not a 500 — a project whose own `HL.lean` does not check
/// is a fact about the corpus, and every caller below says so in its own shape.
fn ground_of(state: &AppState) -> Result<std::path::PathBuf, String> {
    // `{e:#}`: the context names the ground the charter asked for, the cause says what is wrong
    qubitick_build::ground::for_check(&state.corpus()).map_err(|e| format!("{e:#}"))
}

/// One file's check, against the ground its corpus was assembled with.
fn build_one(state: &AppState, path: &str, ground: Option<&std::path::Path>) -> String {
    let conn = state.db.lock().unwrap();
    // no context by hand: the file's own scope decides which symbols it is written against
    let res = qubitick_build::build_file(&conn, std::path::Path::new(path), ground);
    drop(conn);
    match res {
        // the stream frame carries the same report a terminal would print
        Ok(r) => json!({
            "path": r.path,
            "ok": r.ok,
            "verified": r.verified,
            "diagnostics": r.diagnostics,
        })
        .to_string(),
        Err(e) => json!({
            "path": path,
            "ok": false,
            "verified": false,
            "error": e.to_string(),
            "diagnostics": [serde_json::to_value(
                qubitick_model::Diagnostic::error(Phase::Emit, "emit.failed", e.to_string())
                    .in_file(path)
            )
            .unwrap_or_default()],
        })
        .to_string(),
    }
}

fn walk_qbit(root: &Path) -> Vec<String> {
    // One walker, the corpus's own: it knows `Canon.qbit` is the charter rather
    // than a node, which a second hand-written walk here did not — so the HTTP
    // build tried to compile the project charter as a corpus file and reported it
    // as a failure nobody could reproduce from the CLI.
    qubitick_build::walk_qbit(root)
        .into_iter()
        .map(|p| p.display().to_string())
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_symbol_is_scoped_by_the_file_it_is_added_for() {
        let corpus = Path::new("/c/mine");
        assert_eq!(
            symbol_scope(corpus, Some("Era1/Strata3/Theorems/T_X_01_Y.qbit")),
            "Era1/Strata3",
            "the file's own scope, not the engine's first one"
        );
        assert_eq!(
            symbol_scope(corpus, Some("Basics.qbit")),
            "Era0/Strata0",
            "a flat corpus: the engine's default scope for a path with no era"
        );
        assert_eq!(
            symbol_scope(corpus, None),
            "Era0/Strata0",
            "no path at all is the same default, and never a guess about the project"
        );
        assert_eq!(
            symbol_scope(corpus, Some("../../etc/passwd")),
            "Era0/Strata0",
            "a path that escapes the corpus is refused by `project_join`, not scoped from"
        );
    }
}
