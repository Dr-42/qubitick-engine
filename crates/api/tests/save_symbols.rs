//! A SAVE PUTS THE FILE'S SYMBOLS IN THE DB — over the handler the router mounts.
//!
//! `POST /api/file` was a bare `fs::write`: the file changed on disk and the engine's symbol table did
//! not, so a definition you had just saved was unknown until a full build re-ingested the corpus. The
//! save now injects that file's symbols (`qubitick_build::ingest_file_symbols`), and this test is the
//! route's half of the promise — the derivation itself is proved in `crates/build/src/ingest.rs`, where
//! the kind follows `Type:` vs `Sema:` and a later ingest is shown to REPLACE the rows rather than
//! collide with them.
//!
//! It calls the handler rather than making an HTTP request for the reason `external_project.rs` gives:
//! driving the router needs `tower::ServiceExt::oneshot`, and `Cargo.lock` is protected here.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde_json::{Value, json};

use qubitick_api::handlers_gap::{self, FileSaveReq};
use qubitick_api::state::AppState;

fn scratch(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("qb-save-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

/// A project the engine will boot on: a charter, a ground, and one definition.
fn project(dir: &Path) {
    std::fs::create_dir_all(dir.join("Era0/Strata0/Definitions")).unwrap();
    std::fs::create_dir_all(dir.join("lean/Qubitick")).unwrap();
    std::fs::write(dir.join("lean/Qubitick/HL.lean"), qubitick_build::ground::HL_LEAN).unwrap();
    std::fs::write(
        dir.join("Canon.qbit"),
        "P. Save\nId: own\nKind: project\nModule: Qubitick.HL\nLeanDir: lean\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Era0/Strata0/Definitions/D_A_01_Twist.qbit"),
        "D.A.01. Twist\n\\twist a\nType: \\mathfrak{E} \\to \\mathfrak{E}\n",
    )
    .unwrap();
}

async fn answer(res: axum::response::Response) -> (u16, Value) {
    let status = res.status().as_u16();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8_lossy(&bytes).to_string();
    (status, serde_json::from_str(&text).unwrap_or(Value::String(text)))
}

#[tokio::test]
async fn saving_a_definition_injects_its_symbol() {
    let app_root = scratch("app");
    std::fs::write(app_root.join("web-settings.json"), "{}").unwrap();
    let corpus = app_root.join("boot");
    project(&corpus);

    let state = Arc::new(AppState::new(app_root.clone(), Some(app_root.clone())).expect("the server boots"));
    let st = State(state.clone());

    let kind_of = |state: &AppState| -> Option<String> {
        let conn = state.db.lock().unwrap();
        conn.query_row("SELECT kind FROM symbols WHERE tex = '\\twist'", [], |r| r.get(0)).ok()
    };
    assert_eq!(kind_of(&state).as_deref(), Some("primitive"), "the boot ingest saw Type: only");

    /* THE SAVE: the same glyph, now with a Sema: — a definition rather than a primitive */
    let res = handlers_gap::post_file(
        st.clone(),
        Json(FileSaveReq {
            path: "Era0/Strata0/Definitions/D_A_01_Twist.qbit".to_string(),
            content: "D.A.01. Twist\n\\twist a\nSema:\n\\twist a = a\n".to_string(),
        }),
    )
    .await;
    let (status, body) = answer(res).await;

    assert_eq!(status, 200, "a save answers ok: {body}");
    assert_eq!(body["ok"], json!(true));
    assert_eq!(body["inLedger"], json!(true), "the file has a node row: {body}");
    assert!(body["symbols"].as_u64().unwrap_or(0) >= 1, "rows were written: {body}");
    assert_eq!(
        kind_of(&state).as_deref(),
        Some("notation"),
        "the saved Sema: is what the symbol table has now, without a rebuild"
    );
}

/* A FILE THE ENGINE HAS NOT INGESTED YET keeps its save and gets no symbol: `symbols.home_node` is a
 * foreign key, and guessing a node id would attach the author's notation to someone else's node. The
 * answer says `inLedger: false` so the interface can tell the person what happened. */
#[tokio::test]
async fn saving_a_file_outside_the_ledger_says_so() {
    let app_root = scratch("app-unledgered");
    std::fs::write(app_root.join("web-settings.json"), "{}").unwrap();
    let corpus = app_root.join("boot");
    project(&corpus);

    let state = Arc::new(AppState::new(app_root.clone(), Some(app_root.clone())).expect("the server boots"));
    let st = State(state.clone());

    let res = handlers_gap::post_file(
        st.clone(),
        Json(FileSaveReq {
            path: "Era0/Strata0/Definitions/D_A_02_New.qbit".to_string(),
            content: "D.A.02. New\n\\fresh a\nType: \\mathfrak{E} \\to \\mathfrak{E}\n".to_string(),
        }),
    )
    .await;
    let (status, body) = answer(res).await;

    assert_eq!(status, 200, "the save still happened: {body}");
    assert_eq!(body["ok"], json!(true));
    assert_eq!(body["symbols"], json!(0));
    assert_eq!(body["inLedger"], json!(false), "and the answer says why nothing was injected: {body}");

    /* the file IS on disk — the save is the save */
    assert!(corpus.join("Era0/Strata0/Definitions/D_A_02_New.qbit").is_file());
}