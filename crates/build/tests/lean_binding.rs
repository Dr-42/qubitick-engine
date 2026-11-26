//! A DECLARATION MAY BIND ITS GLYPH TO A LEAN NAME THAT ALREADY EXISTS.
//!
//! `Type: HMul.hMul` says the glyph IS that name: the engine declares nothing for it, so
//! what a proof calls is what the project's ground provides — Mathlib's own notation and
//! its own lemmas — instead of an `axiom` of ours sitting beside it.
//!
//! The fixture binds to names the EMBEDDED ground already carries (`True`, `Not`), because
//! the point is the declaration's effect, not the name: no `axiom`/`def` mentions the
//! glyph, the bound name appears, and the corpus still verifies.

use std::path::{Path, PathBuf};

fn root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qb-bind-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn wit(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// `\truth` IS Lean's `True`; the axiom states it.
fn corpus(dir: &Path) {
    wit(dir, "Canon.qbit", "P. a bound corpus\nId: bound\n");
    wit(
        dir,
        "Era0/Strata0/Definitions/D_S_01_Truth.qbit",
        "D.S.01. Truth\n\\truth\nType: True\nRole: null\n",
    );
    // No A block: it would emit nothing at all (that is the A-block invariant), and the
    // claim here is about the DECLARATION — a bound glyph declares nothing, anywhere.

}

fn lean_is_here() -> bool {
    std::process::Command::new("lean")
        .arg("--version")
        .output()
        .is_ok()
}

#[test]
fn a_bound_glyph_gets_no_declaration_of_its_own() {
    let dir = root("bound");
    corpus(&dir);
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &dir).unwrap();
    let file = dir.join("Era0/Strata0/Definitions/D_S_01_Truth.qbit");
    let ctx = qubitick_ctx::build_ctx(&db, "Era0/Strata0").unwrap();
    let emitted =
        qubitick_build::pipeline::emit_file(&ctx, &db, &file).expect("the declaration emits");

    for banned in ["truth", "True", "axiom", "def "] {
        assert!(
            !emitted.contains(banned),
            "the engine declares nothing for a bound glyph, found `{banned}`:\n{emitted}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_bound_corpus_verifies_against_the_embedded_ground() {
    if !lean_is_here() {
        eprintln!("skipping: lean not found");
        return;
    }
    let dir = root("verify");
    corpus(&dir);
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &dir).unwrap();
    let ground = qubitick_build::ground::for_check(&dir).expect("the ground assembles");
    let results =
        qubitick_build::run_pipeline(&db, &qubitick_build::walk_qbit(&dir), Some(&ground)).unwrap();
    let errors: Vec<String> = results
        .iter()
        .flat_map(|r| r.diagnostics.iter())
        .filter(|d| d.is_error())
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert!(
        errors.is_empty(),
        "a corpus binding to a name its ground carries must build clean: {errors:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_ground_name_and_a_sema_at_once_is_refused() {
    // A named `Type:` says the ground declares the glyph; a `Sema:` says the corpus does.
    // One node cannot say both, and the refusal says so.
    let dir = root("both");
    wit(&dir, "Canon.qbit", "P. a confused corpus\nId: confused\n");
    wit(
        &dir,
        "Era0/Strata0/Definitions/D_S_01_Both.qbit",
        "D.S.01. Both\n\\thing\nType: True\nSema:\n\\thing = \\thing\n",
    );
    /* THE FILE IS REFUSED; THE CORPUS STILL OPENS. One bad file must not make the rest
       unopenable — the sentence belongs to the file, and the build reports it there. */
    let mut db = qubitick_db::open_in_memory().unwrap();
    let report = qubitick_build::ingest_corpus(&mut db, &dir).expect("the corpus still opens");
    assert_eq!(report.nodes, 0, "the bad node is not in the ledger");
    let (file, said) = report
        .refused
        .first()
        .expect("and it is recorded as refused");
    assert!(file.ends_with("D_S_01_Both.qbit"), "the file is named: {file}");
    assert!(
        said.contains("both") && said.contains("means one thing"),
        "the refusal says which two things the node did: {said}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}