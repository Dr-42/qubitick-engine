//! HTTP handlers (CodeArch §5.13).
//!
//! Each handler is a thin wrapper: request → engine call → JSON response.

use std::path::{Path, PathBuf};
use std::sync::Arc;


use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::{Html, IntoResponse};
use axum::Json;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::respond::{bad_request, internal, not_found, unprocessable};
use crate::state::AppState;
use axum::response::Response;
use qubitick_model::Phase;

// ---- Request bodies ----

#[derive(Deserialize)]
pub struct ParseReq {
    pub content: String,
}

#[derive(Deserialize)]
pub struct AutociteReq {
    pub lhs: String,
    pub rhs: String,
}

/// `POST /api/rows` — the buffer whose rows are checked.
#[derive(Deserialize)]
pub struct RowsReq {
    pub content: String,
}

/// `POST /api/steps` — an expression to step from, and how many suggestions to answer with.
#[derive(Deserialize)]
pub struct StepsReq {
    pub expr: String,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
pub struct BuildReq {
    #[serde(default)]
    pub scope: Option<String>,
}

// ---- UI / static ----

/// Serve the web IDE shell at `/`.
pub async fn serve_ui(State(_state): State<Arc<AppState>>) -> Html<String> {
    let body = "<!doctype html><html><head><title>Qubitick</title></head><body><h1>Qubitick</h1><p>Web IDE</p></body></html>";
    Html(body.to_string())
}

// ---- Corpus queries ----

/// `GET /api/files` — list `.qbit` project files.
pub async fn get_files(State(state): State<Arc<AppState>>) -> Json<Value> {
    let corpus = state.corpus();
    // Paths are CORPUS-relative, because that is the vocabulary every path-taking
    // endpoint joins against (`project_join(corpus, rel)`). Walking the corpus
    // directly yields the root's own prefix — `./project/Era0/…` on a server started
    // with `--root .` — so the client asked /api/file for exactly those paths, got a
    // 404 for every one of them, and the app showed a live tree and graph around
    // tiles that could not load a single line of corpus text. The mock server always
    // answered corpus-relative paths, which is why every UI test passed while the
    // real app was blank.
    let files: Vec<String> = qubitick_build::walk_qbit(&corpus)
        .into_iter()
        .map(|p| {
            p.strip_prefix(&corpus)
                .map(|rel| rel.display().to_string())
                .unwrap_or_else(|_| p.display().to_string())
        })
        .collect();
    Json(json!({ "files": files }))
}

/// `GET /api/symbols` — list symbols in scope.
pub async fn get_symbols(State(state): State<Arc<AppState>>) -> Json<Value> {
    let ctx = state.ctx.read().unwrap();
    let symbols: Vec<Value> = ctx.symbols()
        .map(|r| {
            json!({
                "symbol": r.display,
                "lean": r.lean_name,
                "tex": r.tex,
                "kind": r.kind,
                "trigger": r.trigger,
                "home_node": r.home_node,
            })
        })
        .collect();
    Json(json!({ "symbols": symbols }))
}

/// `GET /api/palette` — per-symbol colors.
pub async fn get_palette(State(state): State<Arc<AppState>>) -> Json<Value> {
    let conn = state.db.lock().unwrap();
    let entries = qubitick_palette::generate_palette(&conn).unwrap_or_default();
    let symbols: Vec<Value> = entries
        .iter()
        .map(|e| {
            json!({
                "symbol": e.symbol,
                "lean": e.lean,
                "kind": e.kind,
                "color": e.color,
                "hue": e.hue,
                "usage": e.usage,
            })
        })
        .collect();
    Json(json!({ "symbols": symbols }))
}

/// `GET /api/stats` — corpus statistics.
pub async fn get_stats(State(state): State<Arc<AppState>>) -> Json<Value> {
    let conn = state.db.lock().unwrap();
    let nodes = qubitick_db::read::get_node_list(&conn).unwrap_or_default();
    let mut by_kind = std::collections::HashMap::new();
    for n in &nodes {
        *by_kind.entry(n.node_type.as_str().to_string()).or_insert(0usize) += 1;
    }
    let godels: Vec<i64> = nodes.iter().filter_map(|n| n.godel).collect();
    Json(json!({
        "nodes": nodes.len(),
        "kinds": by_kind,
        "symbols": state.ctx.read().unwrap().symbols().count(),
        "godel": {
            "min": godels.iter().min().copied(),
            "max": godels.iter().max().copied(),
            "count": godels.len(),
        },
    }))
}


/// `GET /api/projects` — every project this server can serve, and which is active.
///
/// A project is a directory with a charter (`Canon.qbit`), so this is a directory
/// listing, and "adding a project" needs no configuration.
pub async fn get_projects(State(state): State<Arc<AppState>>) -> Json<Value> {
    let active = state.project();
    Json(json!({
        "projects": project_rows(&state),
        "active": { "id": active.id, "name": active.name },
    }))
}

/// Every project as a client sees it, with the two facts a client cannot derive.
///
/// `active` is the one being served. `wired` says the path was WIRED IN by the user — kept in the
/// workspace settings — as opposed to DISCOVERED under the projects root, and it is what decides
/// whether a row can be forgotten at all (`AppState::unregister`): a discovered directory is found
/// again on the next read, so offering to forget it would be offering something the server cannot
/// do. The pane draws a `forget` on the wired rows and on no others, which is why the server, not
/// the pane, says which they are.
fn project_rows(state: &AppState) -> Vec<Value> {
    let active = state.project();
    let wired = state.registered();
    state
        .projects()
        .iter()
        .map(|p| {
            json!({
                "id": p.id,
                "name": p.name,
                "active": p.id == active.id,
                "wired": wired.iter().any(|w| w == &p.id),
            })
        })
        .collect()
}

/// A project path as a person types it: absolute, relative, `~`, a `file://` URI, quoted.
///
/// THE PATH BELONGS TO THE SERVER, so it is resolved here. The client and the engine need not be
/// anywhere near each other, and a path the client sends is only meaningful relative to something
/// on the machine that reads it — this is the process's working directory, which is what the shell
/// would mean by the same string. What comes back is absolute, and canonical when it exists.
///
/// Canonicalizing is what makes two spellings of one directory ONE project: `project`, `./project/`,
/// `/abs/project` and `file:///abs/project` are the same directory, and the id remembered in the
/// settings is the directory rather than the string somebody happened to type. Without it the
/// picker grew a row per spelling, and forgetting one left the others behind.
///
/// Resolving is best-effort: a path that does not exist (or cannot be read) keeps its resolved
/// spelling, so the refusal can name the directory the server actually looked in.
fn resolve_project_path(input: &str, cwd: &Path) -> PathBuf {
    let trimmed = input.trim();
    // people paste quotes around a path with a space in it
    let unquoted = match (trimmed.chars().next(), trimmed.chars().last()) {
        (Some('"'), Some('"')) | (Some('\''), Some('\'')) if trimmed.len() >= 2 => &trimmed[1..trimmed.len() - 1],
        _ => trimmed,
    };
    let s = strip_file_url(unquoted.trim());
    let expanded = match s.as_str() {
        "~" => std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(&s)),
        _ => match s.strip_prefix("~/") {
            Some(rest) => std::env::var("HOME")
                .map(|home| PathBuf::from(home).join(rest))
                .unwrap_or_else(|_| PathBuf::from(&s)),
            None => PathBuf::from(&s),
        },
    };
    let joined = tidy(if expanded.is_absolute() { expanded } else { cwd.join(expanded) });
    std::fs::canonicalize(&joined).unwrap_or(joined)
}

/// The path with its `.` segments and trailing separators dropped, and nothing else touched.
///
/// `std::path::absolute` keeps a trailing slash (`/opt/mine/` stays `/opt/mine/`), which is exactly
/// the spelling that turns one directory into two ids: the same corpus typed with and without a
/// slash has to be the same project. `Path::components` is the normalizer here — it drops `.` and a
/// trailing separator, and it leaves `..` alone, which `canonicalize` resolves where it can.
fn tidy(path: PathBuf) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() { PathBuf::from(".") } else { out }
}

/// `file:///home/x`, `file://localhost/home/x`, `%20` and friends → the directory they name.
///
/// A browser spells a local directory as a URI, and a person pasting one means the directory; a
/// string that is not a `file:` URI comes back untouched.
fn strip_file_url(s: &str) -> String {
    match s.strip_prefix("file://") {
        None => s.to_string(),
        Some(rest) => {
            let rest = rest.strip_prefix("localhost").unwrap_or(rest);
            percent_decode(if rest.is_empty() { "/" } else { rest })
        }
    }
}

/// The percent-escapes a URI carries, decoded. Only for the path of a `file:` URI, so a malformed
/// escape is left as the character it is rather than refused.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 3 <= bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}


/// `POST /api/projects` — wire in another project path.
///
/// The engine is one executable that can be run from anywhere, so a project does not
/// have to live under any particular directory: the client sends a path, this checks
/// it carries a charter, and remembers it in the workspace settings. The answer carries
/// the project the path turned out to be (`project`), because the path the caller sent
/// is not necessarily the path that got stored — `~` is expanded here — and the name
/// comes from the charter, not from the path.
pub async fn post_projects(
    State(state): State<Arc<AppState>>,
    Json(req): Json<Value>,
) -> Response {
    let path = req.get("path").and_then(|v| v.as_str()).unwrap_or_default();
    if path.is_empty() {
        return bad_request(Phase::Store, "server.missing-path", "missing `path`");
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let resolved = resolve_project_path(path, &cwd);
    match state.register(&resolved) {
        Ok(project) => Json(json!({
            "ok": true,
            "project": { "id": project.id, "name": project.name },
            /* THE PATH THE SERVER USED. Not always the string the client sent — `~`, a relative
               path, a `file://` URI and a trailing slash all resolve here — and the client shows
               this one, so a person can see which directory their corpus really came from. */
            "path": resolved.display().to_string(),
            "projects": project_rows(&state),
        }))
        .into_response(),
        // a path that carries no charter is the caller's mistake, not ours
        Err(err) => bad_request(Phase::Store, "server.register-project", err.to_string()),
    }
}

/// `POST /api/projects/forget` — take a wired-in path back out of the list.
///
/// The other half of `POST /api/projects`, and the reason it exists: a path the user wired in is
/// remembered in the workspace settings, so without this the interface can add a project it can
/// never remove, and the list it shows is not one the user owns. The answer is the list as it now
/// stands, so the caller never has to guess what was kept.
///
/// Two refusals, and neither is a client error it can fix by re-sending: a path that is not a
/// directory or carries no charter can never have been wired in, and the project being SERVED
/// cannot be forgotten (the list is what the next boot starts from). A discovered project is not
/// refused: forgetting one is a no-op that answers with the list, which is one entry longer than
/// the caller expected — and that is the honest answer.
pub async fn post_projects_forget(
    State(state): State<Arc<AppState>>,
    Json(req): Json<Value>,
) -> Response {
    let path = req.get("path").and_then(|v| v.as_str()).unwrap_or_default();
    if path.is_empty() {
        return bad_request(Phase::Store, "server.missing-path", "missing `path`");
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let resolved = resolve_project_path(path, &cwd);
    match state.unregister(&resolved) {
        Ok(_) => Json(json!({
            "ok": true,
            "forgotten": resolved.display().to_string(),
            "projects": project_rows(&state),
        }))
        .into_response(),
        Err(err) => bad_request(Phase::Store, "server.forget-project", err.to_string()),
    }
}

/// `GET /api/project` — the active project.
pub async fn get_project(State(state): State<Arc<AppState>>) -> Json<Value> {
    let active = state.project();
    Json(json!({ "id": active.id, "name": active.name }))
}

/// `POST /api/project` — switch the active project: open its DB, ingest its corpus,
/// build its context. The previous project stays active if any of that fails.
pub async fn post_project(
    State(state): State<Arc<AppState>>,
    Json(req): Json<Value>,
) -> Response {
    let id = req.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    if id.is_empty() {
        return bad_request(Phase::Store, "server.missing-project-id", "missing `id`");
    }
    match state.switch(id) {
        Ok(project) => Json(json!({
            "ok": true,
            "active": { "id": project.id, "name": project.name },
        }))
        .into_response(),
        // the previous project stays active; the named one simply is not there
        Err(err) => not_found(Phase::Store, "server.switch-project", err.to_string()),
    }
}

/// `GET /api/settings` — the stored web UI document (empty when there is none).
pub async fn get_settings(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({ "settings": crate::settings::load(&state.app_root) }))
}

/// `POST /api/settings` — merge the posted keys into the stored document.
///
/// A top-level `null` deletes a key, so `{"layout": null}` is "forget the
/// layout". The merged document is written to `<root>/web-settings.json` and
/// echoed back, so the client never has to guess what was kept.
pub async fn post_settings(
    State(state): State<Arc<AppState>>,
    Json(patch): Json<Value>,
) -> Response {
    let current = crate::settings::load(&state.app_root);
    let next = crate::settings::merge(&current, &patch);
    match crate::settings::save(&state.app_root, &next) {
        Ok(()) => Json(json!({ "ok": true, "settings": next })).into_response(),
        // "could not write your settings" is not a success, whatever the body said
        Err(err) => {
            eprintln!("[settings] save failed: {err}");
            internal("store.settings-save-failed", err.to_string())
        }
    }
}

// ---- Engines ----

/// `POST /api/parse` — parse `.qbit` content to an AST.
pub async fn post_parse(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ParseReq>,
) -> Response {
    match qubitick_parse::parse_content(&state.ctx.read().unwrap(), &req.content) {
        Ok(nodes) => {
            let kinds: Vec<&str> = nodes.iter().map(|n| n.base().kind.as_str()).collect();
            Json(json!({ "ok": true, "nodes": nodes.len(), "kinds": kinds, "ast": format!("{nodes:?}") }))
                .into_response()
        }
        // the request was fine; the text is not a corpus file
        Err(e) => unprocessable(Phase::Parse, "parse.failed", e.to_string()),
    }
}

/// `POST /api/rows` — does each row of this buffer follow from the row above it?
///
/// The corpus's rows are explicit citations, and an explicit reference can name the wrong row. The
/// build catches a citation that names nothing (`cite.dangling`); this catches a citation that
/// does nothing, per row, WHILE the author is typing. `crates/build/src/row_check.rs` is the
/// check; the answers are `justified`, `not-justified` (with the states that citation does
/// reach), `local` (`\equiv`, `^i` — a step the proof takes without citing a node) and
/// `unknown-cite`.
///
/// The buffer is what the editor has, not what the file holds: an unsaved row is exactly the row
/// an author wants checked.
pub async fn post_rows(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RowsReq>,
) -> Response {
    /* DB before context, the order the rest of this file takes them in. */
    let conn = state.db.lock().unwrap();
    let ctx = state.ctx.read().unwrap();
    let nodes = match qubitick_parse::parse_content(&ctx, &req.content) {
        Ok(n) => n,
        Err(e) => {
            return unprocessable(Phase::Parse, "parse.content", format!("{e}"));
        }
    };
    let rows = qubitick_build::check_rows(&conn, &ctx, &nodes);
    let show = |e: &qubitick_model::Expr| qubitick_autocite::show(&ctx, e);
    let unconfirmed = rows
        .iter()
        .filter(|r| r.verdict != "confirmed" && r.verdict != "local")
        .count();
    Json(json!({
        /* `ok` is "the check ran", not "every row was confirmed": what the corpus could not
           confirm is `unconfirmed`, and the two mirrors (this route and `qubitick rows`) say it
           the same way. */
        "ok": true,
        "count": rows.len(),
        "unconfirmed": unconfirmed,
        /* the rows in the order the parser read them; `row` is the row's number within its proof
           block (1-based), which is the only position the parser carries — a client maps it to a
           file line by counting the block's `= ` lines. */
        "rows": rows.iter().map(|r| json!({
            "row": r.row,
            "cite": r.cite,
            "refs": r.refs,
            "verdict": r.verdict,
            "reaches": r.reaches.iter().map(show).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        /* a theorem the corpus has not proved is not a row: the emitter makes it an axiom and the
           build reports it as admitted (`crates/build/src/pipeline.rs`) */
        "admitted": nodes.iter().any(|n| match n {
            qubitick_model::Node::Theorem(t) => t.proof.is_empty(),
            _ => false,
        }),
    }))
    .into_response()
}

/// `POST /api/steps` — the states one cited equation reaches from an expression.
///
/// The question an author asks while writing a proof row is "what can follow this?", and the
/// corpus can answer it: every citable equation applied to the expression, each suggestion
/// carrying the citation that licenses it (`js/live/proof-row.js` writes the row from it). This
/// is the walk `autocite` does INSIDE a bridge, exposed as a list — the difference between a
/// search that needs a target and a suggestion that does not.
///
/// The equations are the corpus's own (`state.rules()`, built once per project), so a suggestion
/// is always something the corpus can cite. The expression is read against the PROJECT's symbols
/// rather than one file's scope: a list of what is reachable is more useful wide than narrow, and
/// the equations handed in are the project's.
pub async fn post_steps(
    State(state): State<Arc<AppState>>,
    Json(req): Json<StepsReq>,
) -> Response {
    let Some(expr) = parse_guess(&state, &req.expr) else {
        return unprocessable(Phase::Parse, "parse.expr", "could not parse the expression");
    };
    let rules = match state.rules() {
        Ok(r) => r,
        Err(e) => return internal("steps.rules", format!("could not read the corpus's equations: {e}")),
    };
    let limit = req.limit.unwrap_or(24).clamp(1, 200);
    let steps = qubitick_autocite::rows::steps(&expr, &rules, limit);
    let ctx = state.ctx.read().unwrap();
    Json(json!({
        "ok": true,
        "state": qubitick_autocite::show(&ctx, &expr),
        "count": steps.len(),
        /* each step is a state plus the citation that reaches it. A step may be an equation
           applied in either direction at any position: what a proof row needs is the citation,
           and the direction is the author's to choose. */
        "steps": steps.iter().map(|s| json!({
            "cite": s.cite,
            "expr": qubitick_autocite::show(&ctx, &s.expr),
        })).collect::<Vec<_>>(),
    }))
    .into_response()
}

/// `POST /api/autocite` — the chain of citable equations that bridges `lhs` to `rhs`.
///
/// The search is the RULED one (`qubitick_autocite::find_with_rules`): it expands by applying
/// the corpus's own equations, so every step of the answer names the citation that justified it
/// (`A1.5`, `D.BT.02`, `T.1`) — which is what makes the answer usable in a proof. The skeleton
/// walk (`find`) substitutes the last argument of an application and hopes; it answered
/// `reached:false` for the corpus's own questions, so a client asking "what justifies
/// `a \\Delta b = b \\Delta a`" was told "nothing" while `qubitick-cli autocite` answered `T.1`.
///
/// A miss is still the useful half: `closest` is the reachable state nearest `rhs` and `gap`
/// the leaves that differ, both in the corpus's own notation rather than as Rust debug output.
pub async fn post_autocite(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AutociteReq>,
) -> Response {
    let lhs = parse_guess(&state, &req.lhs);
    let rhs = parse_guess(&state, &req.rhs);
    let (lhs, rhs) = match (lhs, rhs) {
        (Some(l), Some(r)) => (l, r),
        _ => return unprocessable(Phase::Parse, "parse.lhs-rhs", "could not parse lhs/rhs"),
    };
    let rules = match state.rules() {
        Ok(r) => r,
        Err(e) => return internal("autocite.rules", format!("could not read the corpus's equations: {e}")),
    };
    let result = qubitick_autocite::find_with_rules(&lhs, &rhs, &rules);
    let ctx = state.ctx.read().unwrap();
    let show = |e: &qubitick_model::Expr| qubitick_autocite::show(&ctx, e);
    Json(json!({
        "reached": result.reached,
        "rules": rules.len(),
        /* THE CHAIN, each step with the citation that made it. The first step is where the
           search started, so its `cite` is null — a step nobody had to justify. */
        "citations": result.steps.iter().map(|s| json!({
            "cite": s.cite,
            "expr": show(&s.expr),
        })).collect::<Vec<_>>(),
        "closest": result.closest.as_ref().map(show),
        "gap": result.gap,
    }))
    .into_response()
}

/// `POST /api/rewrite` — compile a proof against a goal.
pub async fn post_rewrite(
    State(state): State<Arc<AppState>>,
    Json(req): Json<Value>,
) -> Json<Value> {
    // Rewrite of a parsed goal; step parsing is a follow-on, so we report the
    // goal parse status and a placeholder term.
    let goal = req.get("goal").and_then(Value::as_str).unwrap_or("");
    let goal_ok = parse_guess(&state, goal).is_some();
    Json(json!({ "lean_term": "by sorry", "ok": goal_ok }))
}

// ---- Build / PDF ----

/// `POST /api/build` — run the build pipeline.
pub async fn post_build(
    State(state): State<Arc<AppState>>,
    Json(_req): Json<BuildReq>,
) -> Response {
    let _lock = state.build_lock.lock().unwrap();
    let mut conn = state.db.lock().unwrap();
    // Refresh the corpus DB so the build reflects the on-disk ledger, then rebuild
    // the CONTEXT from that DB. Building against `state.ctx` — which was made when
    // the project was opened — emitted modules whose class chain came from a
    // corpus snapshot older than the files being built; every theorem then failed
    // with `Unknown identifier Realm`, and 82 of 329 files "did not build". The
    // old endpoint could not show that: it reported `ok`, never `verified`.
    if let Err(e) = qubitick_build::ingest_corpus(&mut conn, &state.corpus()) {
        return internal("build.ingest-failed", e.to_string());
    }
    let files = qubitick_build::walk_qbit(&state.corpus());
    // THE GROUND IS THE PROJECT'S, not ours: the engine's base with the charter's `LeanDir` laid
    // over it (crates/build/src/ground.rs). Without this a project that brings its own `HL.lean`
    // was checked against the shipped one — the decoupling's one promise, silently unimplemented
    // on the path the interface's build button takes. A ground that does not check fails HERE,
    // by name, instead of as three hundred errors in scratch modules nobody wrote.
    let ground = match qubitick_build::ground::for_check(&state.corpus()) {
        Ok(g) => g,
        // `{e:#}` keeps the chain: the context names the ground, the cause says what is wrong
        Err(e) => return internal("build.ground-failed", format!("{e:#}")),
    };
    // A pipeline that could not RUN is a 500; a pipeline that ran and found the
    // corpus does not build is a 200 whose answer is `verified < files`.
    // no context is handed in: `run_pipeline` builds each file's own from the scope its path
    // implies, so a project organised in any stratum is emitted against the symbols visible THERE
    let results = match qubitick_build::run_pipeline(&conn, &files, Some(&ground)) {
        Ok(r) => r,
        Err(e) => return internal("build.pipeline-failed", e.to_string()),
    };
    let warnings: Vec<Value> = results
        .iter()
        .flat_map(|r| r.diagnostics.iter())
        .filter(|d| !d.is_error())
        .map(|d| serde_json::to_value(d).unwrap_or_default())
        .collect();
    let errors = results
        .iter()
        .flat_map(|r| &r.diagnostics)
        .filter(|d| d.is_error())
        .count();
    Json(json!({
        "built": results.iter().filter(|r| r.ok).count(),
        "verified": results.iter().filter(|r| r.verified).count(),
        "files": results.len(),
        // the same report the CLI prints, so a client and a terminal agree
        "failures": results
            .iter()
            .filter(|r| !r.verified)
            .map(|r| json!({
                "path": r.path,
                "diagnostics": r
                    .diagnostics
                    .iter()
                    .filter(|d| d.is_error())
                    .map(|d| serde_json::to_value(d).unwrap_or_default())
                    .collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>(),
        "warnings": warnings,
        "warning_count": qubitick_build::warning_count(&results),
        /* stated and not proved: the emitter makes each one an axiom of its node's module, so
           the file checks — this is the number that says how much of the corpus is assumed */
        "admitted": qubitick_build::admitted_count(&results),
        "error_count": errors,
        "results": results.iter().map(|r| json!({ "path": r.path, "ok": r.ok, "verified": r.verified })).collect::<Vec<_>>(),
    }))
    .into_response()
}


/// `GET /api/pdf` — generate and return the corpus PDF bytes.
/// `GET /api/pdf` — the corpus as a document, with its citations as hyperlinks.
///
/// The material is `qubitick_build::pdf_sections` (the nodes' own lines, each citation resolved to
/// the target its statement declares) and the typesetter is `qubitick_pdf` (the preamble — with
/// `hyperref` — and the assembly). The document used to be a list of node HEADERS: no statements
/// and no proofs, so there was nothing for a citation to point at.
pub async fn get_pdf(State(state): State<Arc<AppState>>) -> axum::response::Response {
    /* build the material before taking the db lock again for the compile */
    let sections = {
        let ctx = state.ctx.read().unwrap();
        let headers = {
            let conn = state.db.lock().unwrap();
            qubitick_db::read::get_node_list(&conn).unwrap_or_default()
        };
        match qubitick_build::pdf_sections(&ctx, &headers) {
            Ok(s) => s,
            Err(e) => {
                return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
            }
        }
    };
    match qubitick_pdf::render_sections_pdf(&sections, &std::env::temp_dir().join("qubitick_pdf")) {
        Ok(bytes) => build_pdf_response(bytes),
        Err(e) => {
            let msg = e.to_string();
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                msg,
            )
                .into_response()
        }
    }
}

fn build_pdf_response(bytes: Vec<u8>) -> axum::response::Response {
    let body = bytes.into();
    let mut resp = axum::response::Response::new(body);
    resp.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        "application/pdf".parse().unwrap(),
    );
    resp
}

// ---- WebSocket ----

/// `GET /ws/check` — a minimal echo socket (live check marker).
pub async fn ws_check_handler(
    State(_state): State<Arc<AppState>>,
    ws: WebSocketUpgrade,
) -> axum::response::Response {
    ws.on_upgrade(ws_echo)
}

async fn ws_echo(mut socket: WebSocket) {
    while let Some(Ok(Message::Text(text))) = socket.recv().await {
        if socket
            .send(Message::Text(format!("echo:{text}").into()))
            .await
            .is_err()
        {
            break;
        }
    }
}

// ---- helpers ----

/* ONE WALKER. This file had a second, hand-written one that listed `Canon.qbit` — the project's
   CHARTER, which is prose and not a node — so every project showed a file nobody could open as a
   node, and `/api/build` (which uses the corpus's own walker) disagreed with the file list about
   what the corpus contains. `crates/api/src/handlers_gap.rs::walk_qbit` had already been rewritten
   for exactly this; this copy was left behind. */

fn parse_guess(
    state: &AppState,
    src: &str,
) -> Option<qubitick_model::Expr> {
    qubitick_parse::semantic::parse_expr(src, &state.ctx.read().unwrap()).ok()
}


#[cfg(test)]
mod tests {
    use super::*;

    /* WHERE A PATH IS RESOLVED, and why it is worth asserting: the client and the engine are not
       necessarily on the same machine, and a path is meaningful only relative to something on the
       machine that reads it. The engine reads it, from the directory it runs in.

       The cases below use paths that do NOT exist, which is what makes them pure: a path that
       cannot be canonicalized keeps its resolved spelling, and that spelling is the whole subject.
       The canonicalizing half (one directory, one id) is the last test, on a directory that does. */

    fn cwd() -> PathBuf {
        PathBuf::from("/srv/engine")
    }

    #[test]
    fn a_relative_path_is_resolved_where_the_engine_runs() {
        assert_eq!(resolve_project_path("corpus", &cwd()).display().to_string(), "/srv/engine/corpus");
        assert_eq!(resolve_project_path("./corpus/", &cwd()).display().to_string(), "/srv/engine/corpus");
        assert_eq!(resolve_project_path("  corpora/mine  ", &cwd()).display().to_string(), "/srv/engine/corpora/mine");
    }

    #[test]
    fn an_absolute_path_is_taken_as_it_stands() {
        assert_eq!(resolve_project_path("/opt/corpora/mine", &cwd()).display().to_string(), "/opt/corpora/mine");
        assert_eq!(resolve_project_path("/opt/corpora/mine/", &cwd()).display().to_string(), "/opt/corpora/mine");
    }

    #[test]
    fn a_file_url_is_the_directory_it_names() {
        assert_eq!(resolve_project_path("file:///opt/corpora/mine", &cwd()).display().to_string(), "/opt/corpora/mine");
        assert_eq!(resolve_project_path("file://localhost/opt/a%20b", &cwd()).display().to_string(), "/opt/a b");
        // and a string that is not a file: URI is a path, with its colons and all
        assert_eq!(resolve_project_path("weird:name", &cwd()).display().to_string(), "/srv/engine/weird:name");
    }

    #[test]
    fn quotes_and_a_leading_tilde_are_the_caller_s_spelling_not_the_path() {
        assert_eq!(resolve_project_path("\"/opt/corpora/mine\"", &cwd()).display().to_string(), "/opt/corpora/mine");
        assert_eq!(resolve_project_path("'/opt/corpora/mine'", &cwd()).display().to_string(), "/opt/corpora/mine");
        if let Ok(home) = std::env::var("HOME") {
            assert_eq!(resolve_project_path("~/corpus", &cwd()), PathBuf::from(&home).join("corpus"));
            assert_eq!(resolve_project_path("~", &cwd()), PathBuf::from(&home));
        }
    }

    #[test]
    fn one_directory_typed_two_ways_is_one_project() {
        let temp = std::env::temp_dir();
        let canonical = std::fs::canonicalize(&temp).unwrap();
        let spelled = format!("{}/", temp.display());
        assert_eq!(resolve_project_path(&spelled, &cwd()), canonical, "a trailing slash is not a second project");
        assert_eq!(resolve_project_path(&format!("{}/.", temp.display()), &cwd()), canonical, "nor is a `.` segment");
    }
}
