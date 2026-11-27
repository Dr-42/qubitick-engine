//! Axum router assembly (CodeArch §5.13).

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;

use crate::handlers;
use crate::handlers_gap;
use crate::state::AppState;

/// Build the full router with all `/api/*`, `/ws`, and static routes.
pub fn router(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new().allow_origin(Any);

    let api = Router::new()
        .route("/api/files", get(handlers::get_files))
        .route("/api/file", get(handlers_gap::get_file).post(handlers_gap::post_file))
        .route("/api/symbols", get(handlers::get_symbols).post(handlers_gap::post_symbols))
        .route("/api/palette", get(handlers::get_palette))
        .route("/api/stats", get(handlers::get_stats))
        .route("/api/settings", get(handlers::get_settings).post(handlers::post_settings))
        .route(
            "/api/projects",
            get(handlers::get_projects).post(handlers::post_projects),
        )
        .route(
            "/api/projects/forget",
            post(handlers::post_projects_forget),
        )
        .route("/api/project", get(handlers::get_project).post(handlers::post_project))
        .route("/api/parse", post(handlers::post_parse))
        .route("/api/ast", post(handlers_gap::post_ast))
        .route("/api/ir", get(handlers_gap::get_ir).post(handlers_gap::post_ir))
        .route("/api/hl", post(handlers_gap::post_hl))
        .route("/api/lean", get(handlers_gap::get_lean))
        .route("/api/graph", get(handlers_gap::get_graph))
        .route("/api/diagnostics", get(handlers_gap::get_diagnostics))
        .route("/api/cite", get(handlers_gap::get_cite))
        .route("/api/citations", get(handlers_gap::get_citations))
        .route("/api/autocite", post(handlers::post_autocite))
        .route("/api/steps", post(handlers::post_steps))
        .route("/api/rows", post(handlers::post_rows))
        .route("/api/rewrite", post(handlers::post_rewrite))
        .route("/api/build", post(handlers::post_build))
        .route("/api/slate", post(handlers_gap::post_slate))
        .route("/api/goal", post(handlers_gap::post_goal))
        .route("/api/latex", post(handlers_gap::post_latex))
        .route("/api/induce", post(handlers_gap::post_induce))
        .route("/api/expand", post(handlers_gap::post_expand))
        .route("/api/handwrite", post(handlers_gap::post_handwrite))
        .route("/api/pdf", get(handlers::get_pdf))
        .route("/ws/check", get(handlers_gap::ws_check))
        .route("/ws/slate", get(handlers_gap::ws_slate))
        .route("/ws/build", get(handlers_gap::ws_build));

    // Static web assets: `web-next/` is the only frontend, served under the API/WS
    // routes above. There is no legacy shell to fall back to any more, so a missing
    // `web-next/` is an empty 404 rather than a quietly older app.
    //
    // `/` IS THE DOCKING SHELL (dock.html) and `/index.html` is the canvas. A directory
    // default served by `ServeDir` would always be `index.html`, so the two pages get
    // their names here: the shell is what you land on, and the canvas is one link away.
    // A single executable runs from anywhere, so the client is looked for where a
    // packaged one would put it: `QUBITICK_WEB`, then `web-next/` beside the binary,
    // then the app root (the development checkout).
    let web_dir = std::env::var("QUBITICK_WEB")
        .ok()
        .map(std::path::PathBuf::from)
        .filter(|p| p.join("index.html").is_file())
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|dir| dir.join("web-next")))
                .filter(|p| p.join("index.html").is_file())
        })
        .unwrap_or_else(|| state.app_root.join("web-next"));

    /* `/` → the docking shell, and a bare `/shell`/`/canvas` keep working as names. */
    let web_for_route = web_dir.clone();
    Router::new()
        .merge(api)
        .route("/", get(move || async move { crate::static_pages::page(&web_for_route, "dock.html") }))
        .fallback_service(ServeDir::new(web_dir))
        .layer(cors)
        .with_state(state)
}