//! The static pages the server owns.
//!
//! `web-next/` is served by `ServeDir`, which answers a directory with `index.html`. The
//! workspace has two pages and the DOCKING SHELL is the one `/` should land on, so the page
//! names live here — one function, one content type, and a 404 that says which file is
//! missing instead of an empty body.

use std::path::Path;

use axum::response::{IntoResponse, Response};

/// Serve one named page out of `web_dir`.
///
/// A page that is not there is a 404 that NAMES it: a packaged binary pointed at the wrong
/// `QUBITICK_WEB` produces "dock.html is missing from /some/dir" rather than a blank page
/// and a console that says nothing.
pub fn page(web_dir: &Path, name: &str) -> Response {
    let path = web_dir.join(name);
    match std::fs::read_to_string(&path) {
        Ok(html) => (
            [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
            html,
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            [(axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            format!("{name} could not be read from {}: {e}", web_dir.display()),
        )
            .into_response(),
    }
}