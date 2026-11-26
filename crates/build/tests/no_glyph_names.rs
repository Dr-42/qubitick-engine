//! THE ENGINE NAMES NO MATHEMATICAL GLYPH — a guard, not a feature.
//!
//! A corpus declares every glyph it writes: an operator, a sort, a constructor, the
//! empty element. The only alphabet the engine knows is the DECLARATION FORMAT's
//! (`qubitick_model::format`), and no engine source may carry a corpus glyph as a
//! string literal.
//!
//! So this test reads the engine's own sources and fails if a corpus glyph appears as a
//! string literal in production code: the table this replaced grew back once, quietly,
//! labelled "substrate".
//!
//! What it allows, and why:
//! - `#[cfg(test)]` code: a fixture may write a glyph, but only one its corpus declares.
//! - comments: prose has to be able to name a glyph to explain a decision.
//! - `qubitick-model`'s format module: the format's own four tokens, which no corpus
//!   declares and which are listed there with their reason.

use std::path::{Path, PathBuf};

/// Corpus glyphs, as they are WRITTEN in Rust source (`"\\cap"` is the literal a
/// production file would carry).
const BANNED: &[&str] = &[
    r#""\\cap""#,
    r#""\\cup""#,
    r#""\\Delta""#,
    r#""\\varnothing""#,
    r#""\\times""#,
    r#""\\land""#,
    r#""\\wedge""#,
    r#""\\lor""#,
    r#""\\vee""#,
    r#""\\cdot""#,
    r#""\\subseteq""#,
    r#""\\in""#,
    r#""\\ne""#,
    r#""\\le""#,
    r#""\\ge""#,
    r#""\\cong""#,
    r#""\\hookrightarrow""#,
    r#""\\lnot""#,
    r#""\\langle""#,
    r#""\\rangle""#,
    r#""\\Vert""#,
    r#""\\ulcorner""#,
    r#""\\urcorner""#,
    r#""\\tau""#,
    r#""\\tau_0""#,
    r#""\\sigma""#,
    r#""\\rho""#,
    r#""\\mu""#,
    r#""\\nu""#,
    r#""\\mathbb{Z}""#,
    r#""\\mathfrak""#,
    r#""^+""#,
    r#""mathbb_Z""#,
    r#""tau""#,
    r#""tau_0""#,
    r#""tick""#,
];

/// Files whose production code may still write glyphs, each with the reason.
fn exempt(rel: &str) -> bool {
    // The declaration format's alphabet (iff/arrow/binder/citation marks), which no
    // corpus declares, and the LaTeX LINT's command dictionary — a list of the control
    // words LaTeX itself knows, kept to report a malformed formula. Neither decides
    // what a glyph MEANS in a corpus.
    rel == "crates/model/src/format.rs" || rel == "crates/build/src/latex.rs"
}

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            // `tests/` holds integration fixtures; `target/` is build output.
            if name == "tests" || name == "target" || name == ".git" {
                continue;
            }
            rs_files(&path, out);
        } else if name.ends_with(".rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_corpus_glyph_appears_in_the_engines_sources() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let mut files = Vec::new();
    rs_files(&repo.join("crates"), &mut files);
    assert!(!files.is_empty(), "the scan found no sources under {}", repo.display());

    let mut found: Vec<String> = Vec::new();
    for path in &files {
        let rel = path
            .strip_prefix(&repo)
            .unwrap_or(path)
            .display()
            .to_string();
        if exempt(&rel) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        // This codebase keeps its tests in a TRAILING `#[cfg(test)]` module, so the
        // production half is everything before the first one. The guard depends on that
        // convention, and says so rather than pretending to parse Rust.
        let production = match text.find("#[cfg(test)]") {
            Some(i) => &text[..i],
            None => text.as_str(),
        };
        for (n, line) in production.lines().enumerate() {
            let code = match line.find("//") {
                Some(i) => &line[..i],
                None => line,
            };
            for banned in BANNED {
                if code.contains(banned) {
                    found.push(format!("{rel}:{}: {banned}", n + 1));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "the engine must name no corpus glyph in production code — a corpus declares its \
         own, and the engine's only alphabet is the declaration format's \
         (qubitick_model::format). Found:\n{}",
        found.join("\n")
    );
}