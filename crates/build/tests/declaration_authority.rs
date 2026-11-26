//! W1's acceptance, measured on a corpus rather than asserted about the code.
//!
//! Two claims, one corpus apart:
//!
//! 1. A corpus that uses a glyph as an operator and never declares it **fails**, and
//!    the failure NAMES the glyph. The engine ships no mathematical vocabulary, so
//!    there is no builtin classification to fall back on.
//! 2. The same corpus, with three declaration nodes added, builds and verifies — so
//!    what failed was the missing declaration and nothing else.
//!
//! The declarations are the whole surface a corpus needs. A glyph line says the
//! FIXITY (`a \Delta b` is infix); a `Role:` says what no shape can (`nullity` for the
//! operator a proof rewrites through, `null` for the carrier's empty element).

use std::path::{Path, PathBuf};

fn root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qb-w1-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn wit(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// The corpus: one axiom, one definition, one theorem that cites both — the trio the
/// pipeline tests use, except that `\Delta`/`\cap`/`\varnothing` are DECLARED only when
/// `declared` is asked for.
fn corpus(dir: &Path, declared: bool) {
    wit(
        dir,
        "Era0/Strata0/Axioms/A1_Base.qbit",
        "A1. Base\n\\forall a b c : \\mathfrak{E}, (a \\Delta b) \\Delta c = a \\Delta (b \\Delta c)\n\
         a \\Delta \\varnothing = a\n\
         a \\Delta a = \\varnothing\n\
         a \\cap \\mathbb{1} = a\n",
    );
    wit(
        dir,
        "Era0/Strata0/Definitions/D_01_Wedge.qbit",
        "D.X.01. Wedge\na \\wedge b\nSema:\na \\wedge b = \\mathbb{1} \\Delta (a \\Delta b)\n",
    );
    wit(
        dir,
        "Era0/Strata0/Theorems/T_01_Self.qbit",
        "T.X.01. WedgeSelf\n\\forall a : \\mathfrak{E}, a \\wedge a = \\mathbb{1}\n\
         Proof:\na \\wedge a\n  = \\mathbb{1} \\Delta (a \\Delta a)    [D.X.01]\n\
         \x20 = \\mathbb{1} \\Delta \\varnothing     [A1.3]\n\
         \x20 = \\mathbb{1}                     [A1.2]\n",
    );
    if !declared {
        return;
    }
    // One node per glyph (a node declares the glyph on its own glyph line), and distinct
    // division numbers because two definitions claiming one number is an ingest error.
    /* `Type: R.<field>` NAMES THE HOME. The glyph IS that field of the structure the
       proofs are parametric over, so the engine declares nothing for it and the corpus's
       own ground (see `ground_with_chain`) is where it lives. */
    wit(
        dir,
        "Era0/Strata0/Definitions/D_00_Delta.qbit",
        "D.X.00. Delta\na \\Delta b\nType: R.Delta : \\mathfrak{E} \\to \\mathfrak{E} \\to \\mathfrak{E}\nRole: nullity\n",
    );
    wit(
        dir,
        "Era0/Strata0/Definitions/D_02_Cap.qbit",
        "D.X.02. Cap\na \\cap b\nType: R.cap : \\mathfrak{E} \\to \\mathfrak{E} \\to \\mathfrak{E}\n",
    );
    wit(
        dir,
        "Era0/Strata0/Definitions/D_03_Empty.qbit",
        "D.X.03. Empty\n\\varnothing\nType: R.varnothing : \\mathfrak{E}\nRole: null\n",
    );
    // The carrier: the axiom writes `\mathfrak{E}` as a binder sort, so the stratum must
    // home it — every glyph an A block writes needs a D node (crates/build/src/coverage.rs).
    wit(
        dir,
        "Era0/Strata0/Definitions/D_04_Carrier.qbit",
        "D.X.04. Carrier\n\\mathfrak{E}\nType: E\n",
    );
    // ...and the unit, which the axiom and the wedge both write.
    wit(
        dir,
        "Era0/Strata0/Definitions/D_05_Unit.qbit",
        "D.X.05. Unit\n\\mathbb{1}\nType: R.unit : \\mathfrak{E}\n",
    );
    // The ground those homes live in: the contract, plus the chain the axiom implies.
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, dir).expect("ingest for the scaffold");
    let chain =
        qubitick_build::scaffold::scaffold(&db, dir, "Qubitick.HL").expect("the chain scaffolds");
    wit(
        dir,
        "lean/Qubitick/HL.lean",
        &format!("{}\n\n{chain}", qubitick_build::ground::HL_LEAN),
    );
    wit(
        dir,
        "Canon.qbit",
        "P. authority\nId: authority\nKind: project\nModule: Qubitick.HL\nLeanDir: lean\n",
    );
}

fn lean_is_here() -> bool {
    std::process::Command::new("lean")
        .arg("--version")
        .output()
        .is_ok()
}

fn build(dir: &Path) -> Vec<qubitick_build::pipeline::BuildFileResult> {
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, dir).expect("ingest");
    let ground = qubitick_build::ground::for_check(dir).expect("the ground assembles");
    qubitick_build::run_pipeline(&db, &qubitick_build::walk_qbit(dir), Some(&ground))
        .expect("the pipeline runs")
}

#[test]
fn an_undeclared_operator_fails_and_names_the_glyph() {
    if !lean_is_here() {
        eprintln!("skipping: lean not found");
        return;
    }
    let dir = root("undeclared");
    corpus(&dir, false);
    let results = build(&dir);
    let said: Vec<String> = results
        .iter()
        .flat_map(|r| r.diagnostics.iter())
        .map(|d| d.message.clone())
        .collect();
    assert!(
        results.iter().any(|r| !r.verified),
        "a corpus using an undeclared operator must not verify: {said:?}"
    );
    assert!(
        said.iter().any(|m| m.contains("`\\Delta`")),
        "the failure must NAME the glyph, not leak a Lean token error: {said:?}"
    );
    assert!(
        said.iter()
            .any(|m| m.contains("is not declared in Era0/Strata0") || m.contains("nothing declares it")),
        "and say where a home would go: {said:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_same_corpus_verifies_once_the_glyphs_are_declared() {
    if !lean_is_here() {
        eprintln!("skipping: lean not found");
        return;
    }
    let dir = root("declared");
    corpus(&dir, true);
    let results = build(&dir);
    let errors: Vec<String> = results
        .iter()
        .flat_map(|r| r.diagnostics.iter())
        .filter(|d| d.is_error())
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert_eq!(results.len(), 8, "three nodes and the five declarations they use");
    assert!(errors.is_empty(), "the declared corpus must build clean: {errors:?}");
    assert!(
        results.iter().all(|r| r.verified),
        "all eight verify: {:?}",
        results.iter().filter(|r| !r.verified).map(|r| r.path.clone()).collect::<Vec<_>>()
    );
    let _ = std::fs::remove_dir_all(&dir);
}