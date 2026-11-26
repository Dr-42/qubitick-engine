//! One way to refuse a request.
//!
//! Eight handlers answered `200 {"ok": false, "error": "…"}`. A 200 says the
//! request succeeded, so every client had to parse the body to find out it had
//! not — and a proxy, a retry, or a cache would all happily treat a failure as a
//! success. The status now says what happened, and the body carries the same
//! [`Diagnostic`] shape the rest of the pipeline reports in, so a client renders a
//! refusal and a warning through one code path.
//!
//! A *build report* is not a refusal: `POST /api/build` returns 200 with
//! `verified < files`, because the request was answered — the answer is that the
//! corpus does not build. Only a request that could not be carried out gets a 4xx
//! or 5xx here.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use qubitick_model::{Diagnostic, Phase};
use serde_json::json;

/// Refuse with `status`, in the diagnostic shape.
///
/// The `error` string is the RENDERED form — the same block the CLI prints, with the
/// position, the underlined run and the `= help:` line — so a person reading a curl
/// answer or a server log sees what a compiler would have told them. `diagnostics`
/// carries the same content as structure, for a client that renders it itself.
pub fn refuse(
    status: StatusCode,
    phase: Phase,
    code: &str,
    message: impl Into<String>,
) -> Response {
    let diagnostic = Diagnostic::error(phase, code, message);
    let rendered = diagnostic.render(&NoSources);
    (
        status,
        Json(json!({
            "ok": false,
            "error": rendered,
            "diagnostics": [serde_json::to_value(&diagnostic).unwrap_or_default()],
        })),
    )
        .into_response()
}

/// A route has no source file in hand: the refusal is about the REQUEST, not a corpus
/// line, so the renderer prints the title and the help and nothing else.
struct NoSources;

impl qubitick_model::SourceLookup for NoSources {
    fn source(&self, _file: &str) -> Option<String> {
        None
    }
}

/// 400 — the request is malformed (a missing or illegal path).
pub fn bad_request(phase: Phase, code: &str, message: impl Into<String>) -> Response {
    refuse(StatusCode::BAD_REQUEST, phase, code, message)
}

/// 422 — the request was well formed and the engine refused its content.
pub fn unprocessable(phase: Phase, code: &str, message: impl Into<String>) -> Response {
    refuse(StatusCode::UNPROCESSABLE_ENTITY, phase, code, message)
}

/// 404 — the thing named does not exist.
pub fn not_found(phase: Phase, code: &str, message: impl Into<String>) -> Response {
    refuse(StatusCode::NOT_FOUND, phase, code, message)
}

/// 409 — the request is fine but the engine is not in a state to take it.
pub fn conflict(phase: Phase, code: &str, message: impl Into<String>) -> Response {
    refuse(StatusCode::CONFLICT, phase, code, message)
}

/// 500 — the engine failed at something the request could not have caused.
pub fn internal(code: &str, message: impl Into<String>) -> Response {
    refuse(
        StatusCode::INTERNAL_SERVER_ERROR,
        Phase::Store,
        code,
        message,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn body_of(response: Response) -> (StatusCode, serde_json::Value) {
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn a_refusal_carries_a_real_status_and_the_diagnostic_shape() {
        let (status, body) =
            body_of(bad_request(Phase::Parse, "server.missing-path", "missing `path`")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["ok"], false);
        // The rendered block, not the bare sentence: a curl answer reads like a compiler.
        assert_eq!(body["error"], "error[server.missing-path]: missing `path`");
        let d = &body["diagnostics"][0];
        assert_eq!(d["severity"], "error");
        assert_eq!(d["phase"], "parse");
        assert_eq!(d["code"], "server.missing-path");
        assert_eq!(d["message"], "missing `path`");
    }

    #[tokio::test]
    async fn an_engine_refusal_is_unprocessable_not_a_bad_request() {
        let (status, body) = body_of(unprocessable(Phase::Parse, "parse.failed", "bad glyph")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["diagnostics"][0]["rate"], serde_json::Value::Null);
        assert_eq!(body["diagnostics"][0]["message"], "bad glyph");
    }

    #[tokio::test]
    async fn storage_failures_are_500() {
        let (status, _) = body_of(internal("store.read-failed", "disk on fire")).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }
}