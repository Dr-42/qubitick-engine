//! `qubitick-pdf` — corpus → LaTeX → PDF (L2).
//!
//! Depends on `model` and `db`. The LaTeX is OURS (see [`latex`]); the engine that renders it
//! is the machine's, and [`engine`] is the choice: `tectonic` when it is there, then
//! `pdflatex`, then `xelatex`. A machine with none of them gets a message naming all three and
//! pointing at the LaTeX source, which is the deliverable a PDF is one rendering of.

pub mod engine;
pub mod latex;
pub mod render;

pub use render::{render_pdf, render_pdf_default, render_sections_pdf};
