//! A build report, rendered the way rustc and Lean print diagnostics.
//!
//! The JSON the CLI and the API emit is for tools. A person reading a terminal wants
//! what `cargo build` and `lean` give them: every diagnostic as a block with a title, an
//! arrow to the position, the source line with the offending run underlined, and the
//! advice underneath — then one summary line. That is what this renders, and it renders
//! it from the SAME [`Diagnostic`] values the JSON carries, so the two faces cannot
//! disagree about a message.
//!
//! ```text
//! error[coverage.unhomed]: `\Delta` is not declared in Era0/Strata0
//!  --> Era0/Strata0/Axioms/A1_Base.qbit:1:36
//!   |
//! 1 | \forall a b : \mathfrak{E}, a \Delta b = b \Delta a
//!   |                                ^^^^^^^ used here
//!   |
//!   = help: add a D node whose glyph line is that glyph, with a `Type:` or `Sema:`
//!
//! error: could not build 1 of 1 file (2 errors found)
//! ```

use qubitick_model::{Diagnostic, SourceLookup};

use crate::pipeline::BuildFileResult;

/// Source text read from disk, on demand: a span names a file, and rendering a report is
/// the one place that file is worth opening.
pub struct DiskSources;

impl SourceLookup for DiskSources {
    fn source(&self, file: &str) -> Option<String> {
        std::fs::read_to_string(file).ok()
    }
}

/// Every diagnostic of every file, in file order, then the summary line.
pub fn render(results: &[BuildFileResult], sources: &impl SourceLookup) -> String {
    let mut out = String::new();
    for result in results {
        for diagnostic in &result.diagnostics {
            out.push_str(&diagnostic.render(sources));
            out.push('\n');
        }
    }
    let summary = summary(results);
    if !summary.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&summary);
        out.push('\n');
    }
    out
}

/// One line for the whole build, phrased the way a compiler sums up.
///
/// Nothing is printed for a file that produced nothing: a clean build says so once, and
/// a red one says how many files it could not build and how many errors it found — the
/// two numbers a reader needs to know whether to look at one file or at the corpus.
pub fn summary(results: &[BuildFileResult]) -> String {
    let files = results.len();
    let failed = results.iter().filter(|r| !r.verified).count();
    let errors = results
        .iter()
        .flat_map(|r| r.diagnostics.iter())
        .filter(|d| d.is_error())
        .count();
    let warnings = results
        .iter()
        .flat_map(|r| r.diagnostics.iter())
        .filter(|d| !d.is_error())
        .count();
    if files == 0 {
        return "ok: nothing to build (the corpus has no nodes)".to_string();
    }
    let files_word = if files == 1 { "file" } else { "files" };
    if errors > 0 {
        let errors_word = if errors == 1 { "error" } else { "errors" };
        return format!(
            "error: could not build {failed} of {files} {files_word} ({errors} {errors_word} found)"
        );
    }
    if warnings > 0 {
        let warning_word = if warnings == 1 { "warning" } else { "warnings" };
        return format!("ok: {files} {files_word} verified ({warnings} {warning_word} emitted)");
    }
    format!("ok: {files} {files_word} verified")
}

/// The diagnostics of a set of results, as one block — what a face that shows a single
/// failure (a route answering 500) needs to say.
pub fn render_one(diagnostic: &Diagnostic, sources: &impl SourceLookup) -> String {
    diagnostic.render(sources)
}