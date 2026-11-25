//! `qubitick-pdf` — corpus → PDF via Tectonic (L2).
//!
//! Depends on `model` and `db`. Tectonic is invoked as the `tectonic` binary
//! (provided by nix-portable) rather than linked as a Rust crate, because the
//! `tectonic` 0.15 crate does not compile against the current registry.

pub mod latex;
pub mod render;
pub mod tectonic;

pub use render::{render_sections_pdf, render_pdf, render_pdf_default};