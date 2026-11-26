//! THE DECOUPLING, END TO END, ON THE ROUTER THE INTERFACE TALKS TO.
//!
//! Everything else that proves it proves a piece: `crates/api/src/state.rs` owns the project list
//! and the switch, `crates/build/src/pipeline.rs` proves a charter's `LeanDir` is the ground its
//! files are checked against, and the UI suite drives a MOCK server. This file is the whole sentence
//! in one run, over real HTTP handling and a real project on disk:
//!
//!   a directory the engine knows nothing about, OUTSIDE every root it was started with, is wired in
//!   by path, switched to, listed, built against ITS OWN `Qubitick/HL.lean`, and then forgotten
//!   again — with the served project refusing to be forgotten while it is being served.
//!
//! It calls the handlers the router mounts rather than making HTTP requests, because a test that
//! drives the router needs `tower::ServiceExt::oneshot` and `Cargo.lock` is a protected file here
//! (tools/lint-rules.mjs): a dependency added for a test would churn the lock for every build. What
//! that leaves uncovered is the route TABLE — path and method — and `tests/unit/route-table.test.mjs`
//! covers it by checking the client's declared routes against this crate's router.
//!
//! The one assertion that carries the promise is the ground: the same corpus builds green, then its
//! own `HL.lean` is broken and the build FAILS naming `LeanDir: lean`. If the engine had quietly
//! checked the corpus against the ground it ships, both builds would have been green — which is the
//! bug this test exists to make impossible to reintroduce.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde_json::{Value, json};

use qubitick_api::handlers;
use qubitick_api::state::AppState;

/// A throwaway directory under `temp_dir`, named for the process so parallel runs do not collide.
fn scratch(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("qb-e2e-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

/// A corpus that verifies: one axiom, one definition, one theorem that cites both.
///
/// It is the trio `crates/build/src/pipeline.rs` uses for its own ground tests — known to verify,
/// so anything this file says is about the GROUND and the ROUTES rather than about the corpus.
/// The declarations the fixture corpus OWES: the engine classifies no mathematical
/// glyph of its own, so a corpus that writes `\Delta`, `\cap` or `\varnothing`
/// declares them. One node per glyph, distinct division numbers (two nodes claiming
/// one number is an ingest error).
fn declare_ops(dir: &Path) {
    let defs = dir.join("Era0/Strata0/Definitions");
    std::fs::create_dir_all(&defs).unwrap();
    /* EVERY GLYPH NAMES ITS HOME. `Type: R.Delta` says the glyph IS the ground's
       `Delta`, reached through the instance the proofs are parametric over — so the
       engine declares nothing for it, and the corpus's own ground (below) is where it
       lives. A bare node would be a glyph the ENGINE has to invent a declaration for,
       which is the fallback this architecture removes. */
    std::fs::write(defs.join("D_00_Delta.qbit"), "D.X.00. Delta\na \\Delta b\nType: R.Delta : \\mathfrak{E} \\to \\mathfrak{E} \\to \\mathfrak{E}\nRole: nullity\n").unwrap();
    std::fs::write(defs.join("D_02_Cap.qbit"), "D.X.02. Cap\na \\cap b\nType: R.cap : \\mathfrak{E} \\to \\mathfrak{E} \\to \\mathfrak{E}\n").unwrap();
    std::fs::write(defs.join("D_03_Empty.qbit"), "D.X.03. Empty\n\\varnothing\nType: R.varnothing : \\mathfrak{E}\nRole: null\n").unwrap();
    std::fs::write(defs.join("D_04_Carrier.qbit"), "D.X.04. Carrier\n\\mathfrak{E}\nType: E\n").unwrap();
    std::fs::write(defs.join("D_05_Unit.qbit"), "D.X.05. Unit\n\\mathbb{1}\nType: R.unit : \\mathfrak{E}\n").unwrap();
}

fn corpus(dir: &Path) {
    declare_ops(dir);
    for d in ["Axioms", "Definitions", "Theorems"] {
        std::fs::create_dir_all(dir.join("Era0/Strata0").join(d)).unwrap();
    }
    std::fs::write(
        dir.join("Era0/Strata0/Axioms/A1_Base.qbit"),
        "A1. Base\n\\forall a b c : \\mathfrak{E}, (a \\Delta b) \\Delta c = a \\Delta (b \\Delta c)\n\
         a \\Delta \\varnothing = a\n\
         a \\Delta a = \\varnothing\n\
         a \\cap \\mathbb{1} = a\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Era0/Strata0/Definitions/D_01_Wedge.qbit"),
        "D.X.01. Wedge\na \\wedge b\nSema:\na \\wedge b = \\mathbb{1} \\Delta (a \\Delta b)\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Era0/Strata0/Theorems/T_01_Self.qbit"),
        "T.X.01. WedgeSelf\n\\forall a : \\mathfrak{E}, a \\wedge a = \\mathbb{1}\n\
         Proof:\na \\wedge a\n  = \\mathbb{1} \\Delta (a \\Delta a)    [D.X.01]\n\
         \x20 = \\mathbb{1} \\Delta \\varnothing     [A1.3]\n\
         \x20 = \\mathbb{1}                     [A1.2]\n",
    )
    .unwrap();
}

/// A project that carries its OWN ground: a charter naming `LeanDir: lean`, and that ground's
/// `Qubitick/HL.lean` — the shipped one to start with, so a green build is about WHERE it was read
/// from rather than about what it says.
/// The ground a corpus's own A block implies: the contract, plus the class chain the
/// block's statements become. `Canon.qbit` names `LeanDir:`; without it the engine's
/// base is used, which carries the contract and NOT the corpus's vocabulary.
fn ground_with_chain(dir: &Path) -> String {
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, dir).expect("ingest for the scaffold");
    let chain = qubitick_build::scaffold::scaffold(&db, dir, "Qubitick.HL").expect("scaffold");
    /* The root class carries the symbols the corpus only NAMES, and the scaffold's chain
       declares the laws over them (`ax_0 : ... Eq (Delta (Delta a b) c) ...` — a field is
       written bare inside the class that declares it). */
    let text = format!("{}\n\n{chain}", qubitick_build::ground::HL_LEAN);
    let _ = std::fs::write("/tmp/qb-fixture-ground.lean", &text);
    text
}

fn project_with_own_ground(dir: &Path, title: &str) {
    corpus(dir);
    std::fs::create_dir_all(dir.join("lean/Qubitick")).unwrap();
    std::fs::write(dir.join("lean/Qubitick/HL.lean"), ground_with_chain(dir)).unwrap();
    std::fs::write(
        dir.join("Canon.qbit"),
        format!("P. {title}\nId: own\nKind: project\nModule: Qubitick.HL\nLeanDir: lean\n"),
    )
    .unwrap();
}

/// A handler's answer as `(status, body)`: the response the router would send, without the router.
async fn answer(res: axum::response::Response) -> (u16, Value) {
    let status = res.status().as_u16();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8_lossy(&bytes).to_string();
    (status, serde_json::from_str(&text).unwrap_or(Value::String(text)))
}

/// The JSON body of a handler that answers `Json<Value>` (a `200` by construction).
async fn body_of(v: Json<Value>) -> (u16, Value) {
    (200, v.0)
}

fn lean_is_here() -> bool {
    std::process::Command::new("lean").arg("--version").output().is_ok()
}

/// The file list, relative to the corpus, charter excluded — what the interface's tree draws.
fn files_of(body: &Value) -> Vec<String> {
    body.get("files")
        .and_then(|f| f.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

#[tokio::test]
async fn a_project_outside_every_root_is_wired_in_served_built_and_forgotten() {
    let app_root = scratch("app");
    /* the settings file is put beside the app root FIRST: `settings::path_for` otherwise falls back to
       the developer's own `~/.config/qubitick`, and this test wires paths in (state.rs::register) */
    std::fs::write(app_root.join("web-settings.json"), "{}").unwrap();
    /* one project where the server looks, so it boots: the repo's own corpus is `project`, and an
       install that keeps its corpora elsewhere has one of these too */
    let boot = app_root.join("boot");
    project_with_own_ground(&boot, "Boot");

    /* THE USER'S PROJECT: outside the app root, outside the projects root, named by nothing but the
       path the client will send. This is the whole decoupling in one directory. */
    let outside = app_root.join("elsewhere/my-project");
    project_with_own_ground(&outside, "My Project");

    let state = Arc::new(AppState::new(app_root.clone(), Some(app_root.clone())).expect("the server boots"));
    let st = State(state.clone());

    /* ---- it is not a project to the engine until the client says where it is ---- */
    let (status, listed) = body_of(handlers::get_projects(st.clone()).await).await;
    assert_eq!(status, 200);
    let ids = |v: &Value| -> Vec<String> {
        v["projects"].as_array().map(|a| a.iter().filter_map(|p| p["id"].as_str().map(str::to_string)).collect()).unwrap_or_default()
    };
    assert_eq!(ids(&listed), vec![boot.display().to_string()], "only what it was started with: {listed}");

    /* ---- wire the PATH in ---- */
    let (status, added) = answer(handlers::post_projects(st.clone(), Json(json!({ "path": outside.display().to_string() }))).await).await;
    assert_eq!(status, 200, "a path that carries a charter is a project: {added}");
    assert_eq!(added["project"]["id"], json!(outside.display().to_string()), "the answer is the project the path IS");
    assert_eq!(added["project"]["name"], json!("My Project"), "and its name comes from the charter, not the directory");
    let rows = added["projects"].as_array().cloned().unwrap_or_default();
    let row = rows.iter().find(|p| p["id"] == json!(outside.display().to_string())).expect("it is in the list");
    assert_eq!(row["wired"], json!(true), "and it is remembered rather than discovered: {row}");

    /* ---- serve it ---- */
    let (status, served) = answer(handlers::post_project(st.clone(), Json(json!({ "id": outside.display().to_string() }))).await).await;
    assert_eq!(status, 200, "{served}");
    let (_, active) = body_of(handlers::get_project(st.clone()).await).await;
    assert_eq!(active["id"], json!(outside.display().to_string()));

    /* ---- the corpus the interface draws is THAT directory's: relative, sorted, no charter ---- */
    let (status, files) = body_of(handlers::get_files(st.clone()).await).await;
    assert_eq!(status, 200);
    assert_eq!(
        files_of(&files),
        vec![
            "Era0/Strata0/Axioms/A1_Base.qbit",
            "Era0/Strata0/Definitions/D_00_Delta.qbit",
            "Era0/Strata0/Definitions/D_01_Wedge.qbit",
            "Era0/Strata0/Definitions/D_02_Cap.qbit",
            "Era0/Strata0/Definitions/D_03_Empty.qbit",
            "Era0/Strata0/Definitions/D_04_Carrier.qbit",
            "Era0/Strata0/Definitions/D_05_Unit.qbit",
            "Era0/Strata0/Theorems/T_01_Self.qbit",
        ],
        "corpus-relative and sorted, and `Canon.qbit` is not a node: {files}"
    );

    if !lean_is_here() {
        eprintln!("no lean in this environment: the build half of this test is skipped");
        let _ = std::fs::remove_dir_all(&app_root);
        return;
    }

    /* ---- THE PROMISE: it builds against ITS OWN ground ---- */
    let (status, built) = answer(handlers::post_build(st.clone(), Json(handlers::BuildReq { scope: None })).await).await;
    assert_eq!(status, 200, "a corpus that verifies is a 200: {built}");
    assert_eq!(built["files"], json!(8), "three nodes and the five declarations they use");
    assert_eq!(
        built["verified"],
        json!(8),
        "all eight verify against the project's own ground: {built}"
    );
    assert_eq!(built["error_count"], json!(0));

    /* ---- and the same corpus FAILS when only the PROJECT's ground is broken ---- */
    let hl = outside.join("lean/Qubitick/HL.lean");
    let mut broken = std::fs::read_to_string(&hl).unwrap();
    broken.push_str("\n-- names nothing, so the file cannot typecheck; nothing else changed\ndef ground_marker : Type := NoSuchGroundType\n");
    std::fs::write(&hl, broken).unwrap();
    let (status, refused) = answer(handlers::post_build(st.clone(), Json(handlers::BuildReq { scope: None })).await).await;
    assert_eq!(status, 500, "a ground that does not check fails the build: {refused}");
    let said = refused["error"].as_str().unwrap_or(&refused.to_string()).to_string();
    assert!(
        said.contains("LeanDir: lean"),
        "the refusal names the PROJECT's ground, not the shipped one: {said}"
    );
    assert!(
        said.contains("failed to compile") || said.contains("does not meet"),
        "and says what is wrong with it: {said}"
    );

    /* ---- the project being served cannot be forgotten out of its own list ---- */
    let (status, refused) = answer(handlers::post_projects_forget(st.clone(), Json(json!({ "path": outside.display().to_string() }))).await).await;
    assert_eq!(status, 400, "{refused}");
    assert!(
        refused["error"].as_str().unwrap_or_default().contains("being served"),
        "and says why: {refused}"
    );

    /* ---- switch away, and then it can be ---- */
    let (status, _) = answer(handlers::post_project(st.clone(), Json(json!({ "id": boot.display().to_string() }))).await).await;
    assert_eq!(status, 200);
    let (status, forgot) = answer(handlers::post_projects_forget(st.clone(), Json(json!({ "path": outside.display().to_string() }))).await).await;
    assert_eq!(status, 200, "{forgot}");
    let (_, left) = body_of(handlers::get_projects(st.clone()).await).await;
    assert_eq!(
        ids(&left),
        vec![boot.display().to_string()],
        "the path is gone from the list the next boot reads: {left}"
    );
    /* forgetting is a SETTINGS edit, not a delete: the user's corpus is still on disk, charter and all */
    assert!(outside.join("Canon.qbit").is_file(), "the directory the user owns is untouched");
    assert!(outside.join("lean/Qubitick/HL.lean").is_file(), "including the ground it brings");

    let _ = std::fs::remove_dir_all(&app_root);
}
/// `POST /api/autocite` — the answer is a chain of the corpus's OWN rows, each naming its citation.
///
/// This route used to call the skeleton walk (`qubitick_autocite::find`), whose single expansion
/// substitutes the last argument of an application and hopes: asked the corpus's own question it
/// answered `reached:false` with a `closest` that was Rust debug output, while
/// `qubitick-cli autocite` answered the same query with a cited chain. What a proof needs from
/// this route is the citation (`A1.2`, `D.X.01`, `T.X.01`) and the state it reached, so both are
/// asserted here, over the trio of files `crates/build`'s own ground tests use.
#[tokio::test]
async fn autocite_answers_with_the_citations_that_bridge_two_expressions() {
    let app_root = scratch("autocite");
    std::fs::write(app_root.join("web-settings.json"), "{}").unwrap();
    let corpus_dir = app_root.join("project");
    corpus(&corpus_dir);
    std::fs::write(corpus_dir.join("Canon.qbit"), "P. Autocite\nId: autocite\nKind: project\n").unwrap();

    let state = Arc::new(AppState::new(app_root.clone(), Some(app_root.clone())).expect("the server boots"));
    let st = State(state.clone());

    /* the theorem's own statement, bridged by the rows the corpus states: `a ∧ a = 1` is written
       down in T_01_Self, and `a ∧ b = 1 Δ (a Δ b)` in D_01_Wedge */
    let (status, body) = answer(
        handlers::post_autocite(
            st.clone(),
            Json(handlers::AutociteReq {
                lhs: "a \\wedge a".to_string(),
                rhs: "\\mathbb{1}".to_string(),
            }),
        )
        .await,
    )
    .await;

    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body["reached"],
        json!(true),
        "a \\wedge a reaches \\mathbb{{1}} through the corpus's own equations: {body}"
    );
    assert!(
        body["rules"].as_u64().unwrap_or(0) >= 5,
        "the rules ARE the corpus's equations — axioms, the Sema of a definition, a theorem's \
         statement: {body}"
    );
    let steps = body["citations"].as_array().cloned().unwrap_or_default();
    assert!(
        steps.len() >= 2,
        "a chain starts where the search started and ends where it reached: {body}"
    );
    assert_eq!(steps[0]["cite"], Value::Null, "the first step needed no citation: {body}");
    let named: Vec<String> = steps
        .iter()
        .filter_map(|s| s["cite"].as_str().map(str::to_string))
        .collect();
    assert!(!named.is_empty(), "a step names the row that justified it: {body}");
    assert!(
        named.iter().all(|c| c.starts_with('A') || c.starts_with('D') || c.starts_with('T')),
        "and it is an id the corpus writes: {named:?}"
    );
    let last = steps.last().unwrap()["expr"].as_str().unwrap_or_default().to_string();
    assert!(
        last.contains("mathbb{1}"),
        "the chain ends at the expression that was asked for, in the corpus's own notation \
         rather than as Rust debug output: {last}"
    );
    assert_eq!(body["closest"], Value::Null, "nothing is left unbridged: {body}");

    /* AND IT COSTS NOTHING THE SECOND TIME. The equations are built once per project (`rules`
       walks the whole corpus), not per request: a query that took the corpus's size to answer
       would take it again here. */
    let (_, again) = answer(
        handlers::post_autocite(
            st,
            Json(handlers::AutociteReq {
                lhs: "a \\wedge a".to_string(),
                rhs: "\\mathbb{1}".to_string(),
            }),
        )
        .await,
    )
    .await;
    assert_eq!(again["citations"], body["citations"], "the same question answers the same way: {again}");

    let _ = std::fs::remove_dir_all(&app_root);
}

/// AN ADMITTED THEOREM IS A REAL STATE, AND IT IS REPORTED AS ONE.
///
/// A theorem whose `Proof:` block says `\sorry` (or is empty, or is absent) carries no steps.
/// The emitter writes it as an `axiom` of the node's OWN module — which is the answer to "how do
/// we express sorry in a ground with no axioms": the ground stays axiom-free (it
/// declare no `sorryAx`, no prove-anything axiom), the assumption is local and named, and
/// `#print axioms` points straight at it — so the file still checks against Lean.
///
/// What must not happen is that file reading as PROVED. The placeholder this replaced
/// (`fun a => Eq.refl _`) proved every statement that happened to be definitional and said
/// `verified` with nothing written, so the report counts admissions and a warning names them.
#[tokio::test]
async fn an_admitted_theorem_verifies_and_is_counted() {
    if !lean_is_here() {
        eprintln!("no lean in this environment: the admitted-theorem test is skipped");
        return;
    }
    let app_root = scratch("admitted");
    std::fs::write(app_root.join("web-settings.json"), "{}").unwrap();
    let dir = app_root.join("project");
    corpus(&dir);
    std::fs::write(
        dir.join("Era0/Strata0/Theorems/T_02_Open.qbit"),
        "T.X.02. Open\n\\forall a : \\mathfrak{E}, a \\wedge a = \\mathbb{1}\nProof:\n\\sorry\n",
    )
    .unwrap();
    // The corpus has an A block, so it has a class chain, so it carries a ground.
    std::fs::create_dir_all(dir.join("lean/Qubitick")).unwrap();
    std::fs::write(dir.join("lean/Qubitick/HL.lean"), ground_with_chain(&dir)).unwrap();
    std::fs::write(
        dir.join("Canon.qbit"),
        "P. Admitted\nId: admitted\nKind: project\nLeanDir: lean\n",
    )
    .unwrap();

    let state = Arc::new(AppState::new(app_root.clone(), Some(app_root.clone())).expect("the server boots"));
    let st = State(state.clone());

    let (status, built) = answer(handlers::post_build(st, Json(handlers::BuildReq { scope: None })).await).await;
    assert_eq!(status, 200, "{built}");
    assert_eq!(
        built["files"],
        json!(9),
        "three proved nodes, one admitted, and the five declarations: {built}"
    );
    assert_eq!(
        built["verified"],
        json!(9),
        "an admitted theorem still checks — it is an axiom of its own module: {built}"
    );
    assert_eq!(built["error_count"], json!(0), "{built}");
    assert_eq!(
        built["admitted"],
        json!(1),
        "and the corpus says how much of it is assumed: {built}"
    );
    let codes: Vec<String> = built["warnings"]
        .as_array()
        .map(|ws| ws.iter().filter_map(|w| w["code"].as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    assert!(
        codes.iter().any(|c| c == "proof.admitted"),
        "the warning names the node that is not proved: {built}"
    );

    let _ = std::fs::remove_dir_all(&app_root);
}

/// `POST /api/steps` — "what can follow this expression", each state with its citation.
///
/// The route exists for the editor's suggestion list (`js/live/proof-row.js` writes the chosen
/// row), and what it must never do is suggest a step the corpus cannot cite: the equations handed
/// in are the corpus's own (`cite_rules`), so a suggestion always names something.
#[tokio::test]
async fn steps_offer_the_rewrites_the_corpus_can_cite() {
    let app_root = scratch("steps");
    std::fs::write(app_root.join("web-settings.json"), "{}").unwrap();
    let dir = app_root.join("project");
    corpus(&dir);
    std::fs::write(dir.join("Canon.qbit"), "P. Steps\nId: steps\nKind: project\n").unwrap();

    let state = Arc::new(AppState::new(app_root.clone(), Some(app_root.clone())).expect("the server boots"));
    let st = State(state.clone());

    let (status, body) = answer(
        handlers::post_steps(
            st.clone(),
            Json(handlers::StepsReq { expr: "a \\wedge a".to_string(), limit: Some(24) }),
        )
        .await,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["ok"], json!(true), "{body}");
    assert_eq!(body["state"], json!("\\wedge a a"), "the state is echoed in the corpus's notation: {body}");
    let steps = body["steps"].as_array().cloned().unwrap_or_default();
    assert!(!steps.is_empty(), "`a ∧ a` is unfoldable through its own definition: {body}");
    assert!(
        steps.iter().all(|s| s["cite"].as_str().is_some_and(|c| !c.is_empty())),
        "every suggestion names the citation that licenses it: {body}"
    );
    assert!(
        steps.iter().any(|s| s["expr"].as_str().is_some_and(|e| e.contains("Delta"))),
        "and the definition's own Sema is among them (`a ∧ b = 1 Δ (a Δ b)`): {body}"
    );

    /* a suggestion that changes nothing is not a suggestion */
    assert!(
        !steps.iter().any(|s| s["expr"] == body["state"]),
        "the state itself is never offered back: {body}"
    );

    /* and an expression the corpus cannot read is refused with the parser's own words */
    let (status, refused) = answer(
        handlers::post_steps(
            st,
            Json(handlers::StepsReq { expr: "\\notacommand x".to_string(), limit: None }),
        )
        .await,
    )
    .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(
        refused["diagnostics"][0]["code"],
        json!("parse.expr"),
        "the refusal carries the phase and the code a client switches on: {refused}"
    );

    let _ = std::fs::remove_dir_all(&app_root);
}

/// `POST /api/rows` — the row check, on the buffer the editor has.
///
/// Three things have to hold at once: a row the corpus CAN derive is confirmed (which exercises
/// the label → node → equation path, including a definition cited by its division `D.X.01`); a row
/// whose citation fires but lands somewhere else is reported UNCONFIRMED with the states it did
/// reach; and the check never claims a row is wrong, because it searches rather than decides
/// (crates/build/src/row_check.rs explains why).
#[tokio::test]
async fn rows_confirm_what_the_corpus_can_derive_and_show_what_it_cannot() {
    let app_root = scratch("rows");
    std::fs::write(app_root.join("web-settings.json"), "{}").unwrap();
    let dir = app_root.join("project");
    corpus(&dir);
    std::fs::write(dir.join("Canon.qbit"), "P. Rows\nId: rows\nKind: project\n").unwrap();

    let state = Arc::new(AppState::new(app_root.clone(), Some(app_root.clone())).expect("the server boots"));
    let st = State(state.clone());

    /* the fixture's own theorem: `a ∧ a` → `1 Δ (a Δ a)` by its definition, → `1 Δ ∅` by A1.3,
       → `1` by A1.2 — every row derivable in one application */
    let good = "T.X.01. WedgeSelf\n\\forall a : \\mathfrak{E}, a \\wedge a = \\mathbb{1}\nProof:\na \\wedge a\n  = \\mathbb{1} \\Delta (a \\Delta a)    [D.X.01]\n  = \\mathbb{1} \\Delta \\varnothing     [A1.3]\n  = \\mathbb{1}                     [A1.2]\n";
    let (status, body) = answer(
        handlers::post_rows(st.clone(), Json(handlers::RowsReq { content: good.to_string() })).await,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["count"], json!(3), "three rows: {body}");
    let verdicts: Vec<&str> = body["rows"].as_array().unwrap().iter().filter_map(|r| r["verdict"].as_str()).collect();
    assert_eq!(
        verdicts,
        vec!["confirmed", "confirmed", "confirmed"],
        "each row is one application of the equation it cites: {body}"
    );
    /* ONE CITATION, TWO SPELLINGS. The parser keeps a citation as a NODE plus a number, not as
       the text the author wrote: `D.X.01` and `D.1` are the same reference (the digits are the
       number), and the engine reports the canonical one. A client shows the author's own text —
       it has it — and what it needs from here is the verdict. */
    assert_eq!(
        body["rows"][0]["cite"], json!("D.1"),
        "the citation is reported in the engine's canonical form: {body}"
    );

    /* the same theorem with the FIRST row citing a law that fires elsewhere: A1.4 is
       `a ∩ 1 = a`, so it rewrites `a` inside `a ∧ a` to `a ∩ 1` and reaches the wrong place */
    let mutant = good.replace("= \\mathbb{1} \\Delta (a \\Delta a)    [D.X.01]", "= \\mathbb{1} \\Delta (a \\Delta a)    [A1.4]");
    let (_, body) = answer(
        handlers::post_rows(st, Json(handlers::RowsReq { content: mutant })).await,
    )
    .await;
    assert_eq!(body["rows"][0]["verdict"], json!("unconfirmed"), "{body}");
    let reaches = body["rows"][0]["reaches"].as_array().cloned().unwrap_or_default();
    assert!(
        !reaches.is_empty(),
        "an unconfirmed row carries the states its citation DOES reach — the half of the answer \
         an author can act on: {body}"
    );
    assert_eq!(
        body["rows"][0]["from"], body["rows"][0]["to"],
        "the check says what it compared: {body}"
    );

    let _ = std::fs::remove_dir_all(&app_root);
}

/// `GET /api/cite?name=` — following a citation to the node, statement and FILE it names.
///
/// The editor's citation links rest on this: the engine resolves the label (one reading, the
/// parser's own — `qubitick_parse::cite_name`) and hands back the corpus-relative path, because a
/// client that had to work out which file a node lives in would be a second implementation of
/// where the corpus keeps things. Three answers, because two of them are not failures: a node, a
/// LOCAL law (`\equiv`, `^i` — no node exists), and a label that names nothing this scope sees.
#[tokio::test]
async fn a_citation_resolves_to_its_node_statement_and_file() {
    use axum::extract::Query;
    use qubitick_api::handlers_gap::{CiteQuery, get_cite};

    let app_root = scratch("cite");
    std::fs::write(app_root.join("web-settings.json"), "{}").unwrap();
    let dir = app_root.join("project");
    corpus(&dir);
    std::fs::write(dir.join("Canon.qbit"), "P. Cite\nId: cite\nKind: project\n").unwrap();

    let state = Arc::new(AppState::new(app_root.clone(), Some(app_root.clone())).expect("the server boots"));
    let st = State(state.clone());

    let (status, body) = body_of(
        get_cite(st.clone(), Query(CiteQuery { fragment: None, name: Some("A1.2".into()) })).await,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let target = &body["target"];
    assert_eq!(target["label"], json!("A1.2"), "{body}");
    assert_eq!(target["kind"], json!("axiom"), "{body}");
    assert_eq!(target["node"], json!("Base"), "the axiom's own name: {body}");
    assert_eq!(target["statement"], json!(2), "which statement of it the citation names: {body}");
    assert_eq!(
        target["path"],
        json!("Era0/Strata0/Axioms/A1_Base.qbit"),
        "and the file a client can open: {body}"
    );

    /* a local law is not a node, and saying so is the answer */
    let (_, local) = body_of(
        get_cite(st.clone(), Query(CiteQuery { fragment: None, name: Some("\\equiv".into()) })).await,
    )
    .await;
    assert_eq!(local["local"], json!(true), "{local}");
    assert_eq!(local["target"], json!(null), "{local}");
    assert!(local["note"].as_str().unwrap_or("").contains("local law"), "{local}");

    /* and a citation that names nothing says that rather than pointing somewhere */
    let (_, none) = body_of(
        get_cite(st, Query(CiteQuery { fragment: None, name: Some("T.9".into()) })).await,
    )
    .await;
    assert_eq!(none["target"], json!(null), "{none}");
    assert!(none["note"].as_str().unwrap_or("").contains("no node"), "{none}");

    let _ = std::fs::remove_dir_all(&app_root);
}

/// `GET /api/pdf`'s MATERIAL, end to end: the corpus as a document whose citations are links.
///
/// The PDF itself needs `tectonic`, which is not installed everywhere (and not here); what can be
/// checked anywhere is the half that matters and is ours — that the document says the corpus's own
/// statements and rows, and that EVERY link it writes has a target it declares. A document full of
/// links to nothing is worse than one with no links, and this corpus cites across files
/// (`T_01_Self` cites `D.X.01`, `A1.3`, `A1.2`).
#[tokio::test]
async fn the_pdf_material_is_a_document_whose_citations_are_links() {
    let app_root = scratch("pdf");
    std::fs::write(app_root.join("web-settings.json"), "{}").unwrap();
    let dir = app_root.join("project");
    corpus(&dir);
    std::fs::write(dir.join("Canon.qbit"), "P. Pdf\nId: pdf\nKind: project\n").unwrap();

    let state = Arc::new(AppState::new(app_root.clone(), Some(app_root.clone())).expect("the server boots"));
    let sections = {
        let ctx = state.ctx.read().unwrap();
        let headers = {
            let conn = state.db.lock().unwrap();
            qubitick_db::read::get_node_list(&conn).unwrap_or_default()
        };
        qubitick_build::pdf_sections(&ctx, &headers).expect("the corpus reads")
    };
    let doc = qubitick_pdf::latex::document_sections(&sections);

    assert!(doc.contains("\\usepackage[colorlinks=true"), "the preamble links");
    assert!(doc.contains("A1: Base"), "every node is a section: {}", &doc[..400.min(doc.len())]);
    assert!(
        doc.contains("\\Delta") && doc.contains("\\wedge"),
        "and it carries the corpus's own statements, not just headings"
    );
    assert!(!doc.contains("\\textbackslash{}Delta"), "the mathematics is not text-escaped");

    /* THE NUMBERING IS THE WHOLE POINT OF A TARGET. `A1.3` is the third statement of A1 as the
       corpus's own citations mean it (`a \Delta a = \varnothing`), and the first version of this
       document was off by one — a statement's own colon (`\forall a : \mathfrak{E}, …`) read as a
       LABEL — which moved every target silently. This pins it. */
    let target_line = |anchor: &str| {
        doc.lines()
            .find(|l| l.contains(&format!("\\hypertarget{{cite:{anchor}}}")))
            .unwrap_or_default()
            .to_string()
    };
    assert!(target_line("A1.1").contains("\\forall"), "A1.1 is the axiom's first statement: {}", target_line("A1.1"));
    assert!(
        target_line("A1.3").contains("a \\Delta a = \\varnothing"),
        "A1.3 is `a \\Delta a = \\varnothing` as the corpus's citations mean it: {}",
        target_line("A1.3")
    );
    assert!(
        doc.contains("\\hypertarget{cite:A1}{}") && doc.contains("\\hypertarget{cite:T.1}{}"),
        "and a node itself is a target, not only its statements"
    );

    let targets: std::collections::HashSet<&str> = doc
        .match_indices("\\hypertarget{")
        .map(|(i, _)| {
            let rest = &doc[i + "\\hypertarget{".len()..];
            &rest[..rest.find('}').unwrap_or(0)]
        })
        .collect();
    let links: Vec<&str> = doc
        .match_indices("\\hyperlink{")
        .map(|(i, _)| {
            let rest = &doc[i + "\\hyperlink{".len()..];
            &rest[..rest.find('}').unwrap_or(0)]
        })
        .collect();
    assert!(links.len() >= 3, "the fixture's theorem cites three equations: {links:?}");
    for link in &links {
        assert!(
            targets.contains(link),
            "{link} points at a target the document does not declare (targets: {targets:?})"
        );
    }
    assert!(
        !doc.contains("hyperlink{cite:\\equiv") && !doc.contains("hyperlink{cite:^i"),
        "a local law has no node to point at, so it stays plain text"
    );

    let _ = std::fs::remove_dir_all(&app_root);
}

/// `POST /api/goal` — the obligation at the caret, what remains of it, and what applies.
///
/// The answer is what an author wants at a position, in one call: the statement being proved, where
/// the proof stands, the row's verdict (`POST /api/rows`), what the corpus still has to bridge
/// (`autocite`, with its `gap` when it cannot), and the rewrites that apply from here
/// (`POST /api/steps`). Each of those was a question of its own; a goal pane asking four times per
/// keystroke is a goal pane nobody keeps open.
#[tokio::test]
async fn the_goal_at_a_caret_says_what_remains_and_what_applies() {
    let app_root = scratch("goal");
    std::fs::write(app_root.join("web-settings.json"), "{}").unwrap();
    let dir = app_root.join("project");
    corpus(&dir);
    std::fs::write(dir.join("Canon.qbit"), "P. Goal\nId: goal\nKind: project\n").unwrap();

    let state = Arc::new(AppState::new(app_root.clone(), Some(app_root.clone())).expect("the server boots"));
    let st = State(state.clone());

    /* the fixture's theorem, with the caret on its SECOND row (`= 1 Δ ∅ [A1.3]`): the row above
       ends at `1 Δ (a Δ a)`, so where the proof stands is that, and what remains is `1` */
    let content = "T.X.01. WedgeSelf\n\\forall a : \\mathfrak{E}, a \\wedge a = \\mathbb{1}\nProof:\na \\wedge a\n  = \\mathbb{1} \\Delta (a \\Delta a)    [D.X.01]\n  = \\mathbb{1} \\Delta \\varnothing     [A1.3]\n  = \\mathbb{1}                     [A1.2]\n";
    let (status, body) = body_of(
        qubitick_api::handlers_gap::post_goal(
            st.clone(),
            Json(qubitick_api::handlers_gap::TextReq { content: content.to_string(), line_no: Some(6) }),
        )
        .await,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let goal = &body["goal"];
    assert_eq!(body["ok"], json!(true), "{body}");
    assert_eq!(goal["kind"], json!("theorem"), "{goal}");
    assert_eq!(goal["node"], json!("WedgeSelf"), "{goal}");
    assert_eq!(goal["target"], json!("\\mathbb{1}"), "what remains is the statement's right side: {goal}");
    assert!(
        goal["current"].as_str().unwrap_or("").contains("Delta"),
        "and where the proof stands is the row above's right side: {goal}"
    );
    assert_eq!(goal["admitted"], json!(false), "{goal}");

    /* the row's verdict, from the same check the editor's strip uses */
    assert_eq!(goal["verdict"]["verdict"], json!("confirmed"), "{goal}");
    assert_eq!(goal["verdict"]["cite"], json!("A1.3"), "{goal}");

    /* WHAT REMAINS, as the corpus sees it: one step (`A1.2` turns `1 Δ ∅` into `1`) */
    let remaining = &goal["remaining"];
    assert_eq!(remaining["reached"], json!(true), "the corpus can finish this proof: {goal}");
    let cites: Vec<&str> = remaining["citations"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["cite"].as_str())
        .collect();
    assert!(cites.contains(&"A1.2"), "and says with what: {remaining}");

    /* WHAT APPLIES from here */
    assert!(
        goal["steps"].as_array().map(|s| !s.is_empty()).unwrap_or(false),
        "a list of applicable rewrites rides along: {goal}"
    );

    let _ = std::fs::remove_dir_all(&app_root);
}

/// THE NODE'S OWN PROSE IS PRINTED AND COMMENTED, NOT DISCARDED.
///
/// `Math:` and `Phys:` are the two readings of a node — its mathematics, its physics — and they are
/// the author's prose. They are KEPT (crates/model/src/node.rs::NodeBase::note) and reach both
/// readers: the PDF prints them (`\textit{Math}: ...`, built from the file's own text) and the Lean
/// emission carries them as a module doc comment (one `.qbit` emits one `.lean`, so the reading
/// belongs at the top of it). Which reading it is lives in the kind's `tag`, which used to be
/// hardcoded `Math` at every construction site — so a `Phys:` axiom was labelled Mathematics.
#[tokio::test]
async fn the_prose_a_node_carries_is_printed_and_commented() {
    let app_root = scratch("prose");
    std::fs::write(app_root.join("web-settings.json"), "{}").unwrap();
    let dir = app_root.join("project");
    corpus(&dir);
    std::fs::write(dir.join("Canon.qbit"), "P. Prose\nId: prose\nKind: project\n").unwrap();
    std::fs::write(
        dir.join("Era0/Strata0/Axioms/A8_Prose.qbit"),
        "A8. Prose\n\\forall a : \\mathfrak{E}, a \\Delta a = \\varnothing\n\
         Phys: events pair and annihilate.\n",
    )
    .unwrap();

    let state = Arc::new(AppState::new(app_root.clone(), Some(app_root.clone())).expect("the server boots"));
    let src = std::fs::read_to_string(dir.join("Era0/Strata0/Axioms/A8_Prose.qbit")).unwrap();
    let nodes = qubitick_parse::parse_content(&state.ctx.read().unwrap(), &src).expect("it parses");

    /* the model keeps it, and labels it the way the author wrote it */
    assert_eq!(
        nodes[0].base().note.as_deref(),
        Some("events pair and annihilate."),
        "the prose survives the parse"
    );
    assert!(
        matches!(nodes[0], qubitick_model::Node::Axiom(ref a) if a.tag == qubitick_model::PhysOrMath::Phys),
        "and a `Phys:` node is labelled Phys, not Math: {:?}",
        nodes[0]
    );

    /* the Lean emission comments it */
    let lean = qubitick_emit::emit_lean_text(
        &state.ctx.read().unwrap(),
        &nodes,
        "Qubitick.HL",
        &mut qubitick_rewrite::rules::RuleTable::new(),
    )
    .expect("it emits");
    assert!(lean.contains("/-!"), "a doc comment block: {lean}");
    assert!(lean.contains("Phys: events pair and annihilate."), "carrying the author's reading: {lean}");
    assert!(lean.find("/-!").unwrap() < lean.find("namespace").unwrap(), "above the namespace, where a reader looks");
    assert!(!lean.contains("Phys:") || lean.find("Phys:").unwrap() < lean.find("inductive").unwrap_or(usize::MAX), "and never inside a declaration");

    /* the PDF prints it (from the file text, the same prose) */
    let doc = {
        let ctx = state.ctx.read().unwrap();
        let headers = {
            let conn = state.db.lock().unwrap();
            qubitick_db::read::get_node_list(&conn).unwrap_or_default()
        };
        let sections = qubitick_build::pdf_sections(&ctx, &headers).expect("the corpus reads");
        qubitick_pdf::latex::document_sections(&sections)
    };
    assert!(doc.contains("\\textit{Phys}: events pair and annihilate."), "the PDF prints the reading");

    let _ = std::fs::remove_dir_all(&app_root);
}
