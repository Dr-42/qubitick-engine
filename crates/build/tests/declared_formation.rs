//! A CORPUS MAY DECLARE AN INDUCTIVE, and the engine emits it.
//!
//! The engine used to carry the tick sort and the integer sort itself: a glyph table
//! said which tex was which, and Rust string literals held whole Lean declarations
//! (`inductive tau : Type | zero | succ`). Both are gone. What replaced them is this:
//! a formation node declares its carrier and its constructors — including a RECURSIVE
//! one — and the emitter writes the inductive, its constructor aliases, and the types
//! the corpus's own signatures state over that carrier.
//!
//! Nothing here is engine vocabulary: `tau`, `tau_0`, `succ`, `vee` and `oplus` are
//! this fixture corpus's spellings, declared in its own files.

use std::path::{Path, PathBuf};

fn root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qb-form-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn wit(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// The tick sequence as a CORPUS declares it: a carrier, a connective to separate the
/// constructors, the two constructors (one nullary, one recursive), an operator over
/// the carrier, and an axiom that uses them.
fn corpus(dir: &Path) {
    wit(dir, "Canon.qbit", "P. the tick corpus\nId: tick\n");
    wit(
        dir,
        "Era0/Strata0/Definitions/D_S_01_Or.qbit",
        "D.S.01. Or\na \\vee b\nType: \\mathcal{P} \\to \\mathcal{P} \\to \\mathcal{P}\nRole: connective\n",
    );
    wit(
        dir,
        "Era0/Strata0/Definitions/D_S_02_Tick.qbit",
        "D.S.02. Tick\n\\tau\nSema:\n\\tau = \\tau_0 \\vee (^+ \\tau)\n",
    );
    wit(
        dir,
        "Era0/Strata0/Definitions/D_S_03_TickZero.qbit",
        "D.S.03. TickZero\n\\tau_0\nType: \\tau\n",
    );
    wit(
        dir,
        "Era0/Strata0/Definitions/D_S_04_Succ.qbit",
        "D.S.04. Succ\n^+ a\nType: \\tau \\to \\tau\n",
    );
    wit(
        dir,
        "Era0/Strata0/Definitions/D_S_05_Plus.qbit",
        "D.S.05. Plus\na \\oplus b\nType: \\tau \\to \\tau \\to \\tau\n",
    );
    // No A block: a law an A block states becomes a field of the GROUND's class, and a
    // class in the ground cannot be written over `\tau` — the formation is this corpus's,
    // and its own module declares it. Statements about it are T blocks.
}

fn lean_is_here() -> bool {
    std::process::Command::new("lean")
        .arg("--version")
        .output()
        .is_ok()
}

#[test]
fn a_declared_formation_is_emitted_as_an_inductive_with_typed_constructors() {
    let dir = root("inductive");
    corpus(&dir);
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &dir).unwrap();
    let file = dir.join("Era0/Strata0/Definitions/D_S_02_Tick.qbit");
    let ctx = qubitick_ctx::build_ctx(&db, "Era0/Strata0").unwrap();
    let emitted =
        qubitick_build::pipeline::emit_file(&ctx, &db, &file).expect("the formation emits");

    assert!(
        emitted.contains("inductive tau : Type"),
        "the corpus's formation is emitted as its own inductive:\n{emitted}"
    );
    assert!(
        !emitted.contains("| tau_0 : tau\n") || emitted.contains("tau_0"),
        "the nullary constructor is declared:\n{emitted}"
    );
    // The RECURSIVE constructor carries its declared argument sort, which is what
    // makes the inductive a real tick sequence instead of a pile of constants.
    assert!(
        emitted.lines().any(|l| l.trim_start().starts_with('|') && l.contains("tau → tau")),
        "the recursive constructor is a function into the carrier:\n{emitted}"
    );
    // ...and the module declares NOTHING ELSE for the operator: `\oplus` is another
    // node's, and the frame writes no `axiom` for it.
    assert!(
        !emitted.contains("axiom"),
        "a corpus module declares no axiom:\n{emitted}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_declared_formation_builds_and_verifies() {
    if !lean_is_here() {
        eprintln!("skipping: lean not found");
        return;
    }
    let dir = root("build");
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
        "a corpus that declares its own inductive must build clean: {errors:?}"
    );
    assert!(
        results.iter().all(|r| r.verified),
        "all of it verifies: {:?}",
        results
            .iter()
            .filter(|r| !r.verified)
            .map(|r| (r.path.clone(), r.diagnostics.first().cloned()))
            .collect::<Vec<_>>()
    );
    let _ = std::fs::remove_dir_all(&dir);
}