//! `qubitick-api` — axum HTTP server (L7).
//!
//! Exposes every engine as a `/api/*` route plus `/ws` and static web serving.
//! Wires request → engine call → response; handlers are thin orchestration.

pub mod handlers;
pub mod handlers_gap;
pub mod respond;
pub mod router;
pub mod settings;
pub mod state;

pub use router::router;
pub use state::AppState;