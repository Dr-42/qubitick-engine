//! AN OPERATOR THE CORPUS NEVER DECLARES IS NAMED, IN A STATEMENT AND IN A PROOF ROW.
//!
//! The parser's operator gate already refuses `a \Delta b` when `\Delta` has no
//! declaration, and it already says what a declaration looks like. What the report used to
//! say instead was:
//!
//! * in a T block's PROOF, `cannot justify rewrite (a → b)`, because the parser had
//!   accepted the fragment `a` and dropped the rest — the `\Delta` never reached the gate;
//! * in a T block's STATEMENT, `Failed to infer type of binder \`a\``, because the statement
//!   collapsed to a lump of text and emitted a Lean binder with no type.
//!
//! Both are the same corpus mistake, and the corpus should hear it once, naming the glyph.

use std::path::{Path, PathBuf};

fn ship(tag: &str) -> PathBuf {
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus-templates/algebra");
    let dir = std::env::temp_dir().join(format!("qb-op-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy(&from, &dir);
    dir
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name == ".lake" || name.ends_with(".bit") || name.starts_with("qubitick.db") {
            continue;
        }
        let target = to.join(entry.file_name());
        if path.is_dir() {
            copy(&path, &target);
        } else {
            std::fs::copy(&path, &target).unwrap();
        }
    }
}

/// Every diagnostic the build produced, as `(code, message, file, line, col)`.
fn said(dir: &Path) -> Vec<(String, String, String, Option<u32>, Option<u32>)> {
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, dir).expect("ingest");
    let ground = qubitick_build::ground::for_check(dir).expect("the ground assembles");
    qubitick_build::run_pipeline(&db, &qubitick_build::walk_qbit(dir), Some(&ground))
        .expect("the pipeline runs")
        .into_iter()
        .flat_map(|r| {
            let path = r.path.clone();
            r.diagnostics.into_iter().map(move |d| {
                (
                    d.code.clone(),
                    d.message.clone(),
                    path.clone(),
                    d.span.line,
                    d.span.col,
                )
            })
        })
        .collect()
}

#[test]
fn a_proof_row_using_an_undeclared_operator_names_the_glyph() {
    let dir = ship("row");
    std::fs::write(
        dir.join("Era0/Strata0/Theorems/T_A_09_Delta.qbit"),
        "T.A.09. Delta\n\\forall a b : \\mathfrak{R}, a + b = b + a\nProof:\na \\Delta b\n  = b \\Delta a [A1.Comm]\n",
    )
    .unwrap();
    let all = said(&dir);
    // THE DOCUMENTED CODE, and the generic rewrite sentence is GONE: the gate fired before
    // the rewriter ever saw a term.
    assert!(
        all.iter().any(|(code, msg, _, line, col)| code == "parse.operator.undeclared"
            && msg.contains("`\\Delta`")
            && msg.contains("nothing declares it")
            && *line == Some(4)
            && *col == Some(3)),
        "the row's glyph is named, with the caret on it: {all:?}"
    );
    assert!(
        !all.iter().any(|(_, msg, _, _, _)| msg.contains("cannot justify")),
        "the misleading rewrite error is not what the author reads: {all:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_statement_using_an_undeclared_operator_names_the_glyph() {
    let dir = ship("statement");
    std::fs::write(
        dir.join("Era0/Strata0/Theorems/T_A_09_Delta.qbit"),
        "T.A.09. Delta\n\\forall a b : \\mathfrak{R}, a \\Delta b = b \\Delta a\n",
    )
    .unwrap();
    let all = said(&dir);
    assert!(
        all.iter().any(|(code, msg, _, line, _)| code == "parse.operator.undeclared"
            && msg.contains("`\\Delta`")
            && *line == Some(2)),
        "the statement's glyph is named: {all:?}"
    );
    assert!(
        !all.iter().any(|(_, msg, _, _, _)| msg.contains("infer type of binder")),
        "the Lean type error is not the corpus's diagnostic: {all:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
