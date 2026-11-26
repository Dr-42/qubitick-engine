//! D10/W2's acceptance, on a corpus that is BUILT rather than on a parse of one line.
//!
//! The charter holds the master precedence table (`Precedence:` lines, loosest
//! first); ingestion writes each declared glyph's level into the symbol row; the
//! parser folds by that level. So a corpus can write `a + b × a` and mean
//! `a + (b × a)` — and the parentheses it does need, it still has to write.
//!
//! Association is NOT part of this: `(a + b) + c = a + (b + c)` is a theorem a corpus
//! states, so the parser nests equal levels left and the table never mentions it.

use std::path::{Path, PathBuf};

fn root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qb-w2-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn wit(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A school-notation corpus: the charter orders `+` below `×`, `+` is a primitive,
/// `×` is defined by repeated addition, and the theorem's statement is written WITHOUT
/// the parentheses precedence makes unnecessary.
fn corpus(dir: &Path) {
    wit(
        dir,
        "Canon.qbit",
        "P. school — the precedence corpus\nId: school\nLeanDir: lean\nNote: the charter orders the operators; nothing else decides.\nPrecedence: +\nPrecedence: ×\n",
    );
    /* `Type: plus` NAMES THE GROUND'S SYMBOL — a global one, not a field of an instance:
       this corpus has no A block, so it has no class chain to hypothesize, and `R` would
       name nothing. A global `def plus` is the whole story, and the engine declares none
       of it. */
    wit(
        dir,
        "Era0/Strata0/Definitions/D_01_Add.qbit",
        "D.S.01. Add\na + b\nType: plus : \\mathfrak{R} \\to \\mathfrak{R} \\to \\mathfrak{R}\n",
    );
    wit(
        dir,
        "lean/Qubitick/HL.lean",
        &format!(
            "{}\n\ndef plus (a b : Prop) : Prop := a\n",
            qubitick_build::ground::HL_LEAN
        ),
    );
    wit(
        dir,
        "Era0/Strata0/Definitions/D_02_Mul.qbit",
        "D.S.02. Mul\na × b\nSema:\na × b = a + a\n",
    );
    wit(
        dir,
        "Era0/Strata0/Theorems/T_01_Precedence.qbit",
        // `a + b × a` — no parentheses around `b × a`, and the RIGHT side keeps the
        // ones its tree really needs.
        "T.S.01. Precedence\n\\forall a b : \\mathfrak{R}, a + b × a = a + (b + b)\n\
         Proof:\na + b × a\n  = a + (b + b)    [D.S.02]\n",
    );
}

fn lean_is_here() -> bool {
    std::process::Command::new("lean")
        .arg("--version")
        .output()
        .is_ok()
}

#[test]
fn the_charter_level_reaches_the_symbol_rows() {
    let dir = root("rows");
    corpus(&dir);
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &dir).unwrap();
    let level = |tex: &str| {
        qubitick_db::queries::symbols_all(&db)
            .unwrap()
            .into_iter()
            .find(|r| r.tex == tex)
            .and_then(|r| r.precedence)
    };
    assert_eq!(level("+"), Some(1), "the first Precedence: line is the loosest");
    assert_eq!(level("×"), Some(2), "the second line binds tighter");
    assert_eq!(level("\\mathfrak{R}"), None, "a glyph the table does not name has no level");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_statement_written_without_parentheses_binds_by_the_table() {
    if !lean_is_here() {
        eprintln!("skipping: lean not found");
        return;
    }
    let dir = root("built");
    corpus(&dir);
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &dir).unwrap();
    let file = dir.join("Era0/Strata0/Theorems/T_01_Precedence.qbit");
    let ctx = qubitick_ctx::build_ctx(&db, "Era0/Strata0").unwrap();
    let emitted = qubitick_build::pipeline::emit_file(&ctx, &db, &file).expect("the theorem emits");
    // `a + b × a` must be the PLUS application holding the TIMES application: the
    // statement's left side is `plus a (times b a)`.
    assert!(
        emitted.contains("plus a (times b a)"),
        "the looser operator must be the root of `a + b × a`:\n{emitted}"
    );
    // And the corpus verifies, so the parse fed the proof rows that grouping too.
    let ground = qubitick_build::ground::for_check(&dir).unwrap();
    let results =
        qubitick_build::run_pipeline(&db, &qubitick_build::walk_qbit(&dir), Some(&ground)).unwrap();
    let errors: Vec<String> = results
        .iter()
        .flat_map(|r| r.diagnostics.iter())
        .filter(|d| d.is_error())
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert!(errors.is_empty(), "the precedence corpus must build clean: {errors:?}");
    assert!(
        results.iter().all(|r| r.verified),
        "all of it verifies: {:?}",
        results
            .iter()
            .filter(|r| !r.verified)
            .map(|r| r.path.clone())
            .collect::<Vec<_>>()
    );
    let _ = std::fs::remove_dir_all(&dir);
}