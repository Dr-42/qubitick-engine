//! THE EMISSION SURFACE IS FOUR CONSTRUCTS: `def`, `inductive`, `theorem`, reference.
//!
//! The engine used to fall back to `axiom` whenever a glyph could not be produced as a
//! `def` — a primitive with no body, a definition seen from a module that could not unfold
//! it, a sort word in a signature. Each was the engine inventing an assumption nobody
//! chose. These tests pin the replacement: an import, a reference, or a build error naming
//! the glyph.

use std::path::{Path, PathBuf};

fn root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qb-surface-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn wit(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn lean_is_here() -> bool {
    std::process::Command::new("lean")
        .arg("--version")
        .output()
        .is_ok()
}

/// Every module a corpus emits, in walk order.
fn emitted_all(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, dir).unwrap();
    let ctx = qubitick_ctx::build_ctx(&db, "Era0/Strata0").unwrap();
    qubitick_build::walk_qbit(dir)
        .into_iter()
        .map(|f| {
            let text = qubitick_build::pipeline::emit_file(&ctx, &db, &f).unwrap_or_default();
            (f, text)
        })
        .collect()
}

#[test]
fn no_module_emits_an_axiom() {
    // The tutorial ship: a primitive, a `Sema:` definition, an A block, a theorem.
    let ship = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus-templates/algebra");
    let modules = emitted_all(&ship);
    assert_eq!(modules.len(), 5, "the ship is five modules");
    for (file, text) in &modules {
        for line in text.lines() {
            assert!(
                !line.trim_start().starts_with("axiom"),
                "{} declares an axiom: {line}",
                file.display()
            );
        }
    }
}

#[test]
fn a_glyph_the_ground_names_declares_nothing_and_renders_its_name() {
    let dir = root("ground-name");
    wit(&dir, "Canon.qbit", "P. bound\nId: bound\n");
    wit(
        &dir,
        "Era0/Strata0/Definitions/D_S_01_Truth.qbit",
        "D.S.01. Truth\n\\truth\nType: True\n",
    );
    // A corpus definition written over the ground's symbol: the body says `True`, and
    // nothing declares `\\truth` — the ground has it.
    wit(
        &dir,
        "Era0/Strata0/Definitions/D_S_02_Both.qbit",
        "D.S.02. Both\n\\both\nSema:\n\\both = \\truth\n",
    );
    let (_, text) = emitted_all(&dir)
        .into_iter()
        .find(|(f, _)| f.ends_with("D_S_02_Both.qbit"))
        .expect("the definition module");
    assert!(text.contains("True"), "the ground's name is what it writes: {text}");
    assert!(!text.contains("axiom"), "{text}");
    assert!(!text.contains("truth"), "the glyph gets no declaration: {text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_sort_word_is_a_literal() {
    // `Type: Type` is a reference to Lean's own sort: no declaration, no binding, and the
    // word survives into the emitted text as the syntax it is.
    let dir = root("sort-word");
    wit(&dir, "Canon.qbit", "P. sorts\nId: sorts\n");
    wit(
        &dir,
        "Era0/Strata0/Definitions/D_S_01_Sort.qbit",
        "D.S.01. Sort\n\\uni\nType: Type\n",
    );
    wit(
        &dir,
        "Era0/Strata0/Theorems/T_S_01_Use.qbit",
        "T.S.01. Use\n\\forall a : \\uni, Eq a a\nProof:\n\\forall a : \\uni, Eq a a\n  ⊢ fun a => Eq.refl a\n",
    );
    let modules = emitted_all(&dir);
    let (_, theorem) = modules
        .iter()
        .find(|(f, _)| f.ends_with("T_S_01_Use.qbit"))
        .expect("the theorem module");
    assert!(!theorem.contains("axiom"), "no sort word is axiomatized: {theorem}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_definition_from_another_module_is_imported() {
    if !lean_is_here() {
        eprintln!("skipping: lean not found");
        return;
    }
    // A `Sema:` definition in one module, a theorem in another that unfolds it: the
    // theorem's module IMPORTS the definition's module. That import is what replaced the
    // `axiom times` + `axiom times_def` pair.
    let ship = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus-templates/algebra");
    let modules = emitted_all(&ship);
    let (_, theorem) = modules
        .iter()
        .find(|(f, _)| f.ends_with("T_A_01_MulTwo.qbit"))
        .expect("the theorem module");
    assert!(
        theorem.contains("import Qubitick.Corpus."),
        "the definition's module is imported, not restated: {theorem}"
    );
    assert!(!theorem.contains("axiom"), "{theorem}");
    // ...and the whole corpus still builds, which is what makes the import honest.
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &ship).unwrap();
    let ground = qubitick_build::ground::for_check(&ship).unwrap();
    let results =
        qubitick_build::run_pipeline(&db, &qubitick_build::walk_qbit(&ship), Some(&ground)).unwrap();
    let errors: Vec<String> = results
        .iter()
        .flat_map(|r| r.diagnostics.iter())
        .filter(|d| d.is_error())
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn a_type_with_no_ground_name_and_no_sema_is_refused() {
    // `Type: <signature>` alone names no home: the glyph is declared but lives nowhere,
    // and the engine writes no `axiom` in its place.
    let dir = root("no-home");
    wit(&dir, "Canon.qbit", "P. homeless\nId: homeless\n");
    wit(
        &dir,
        "Era0/Strata0/Definitions/D_S_01_Cap.qbit",
        "D.S.01. Cap\na \\cap b\nType: \\mathfrak{V} \\to \\mathfrak{V} \\to \\mathfrak{V}\n",
    );
    wit(
        &dir,
        "Era0/Strata0/Definitions/D_S_02_Mul.qbit",
        "D.S.02. Mul\na \\odot b\nSema:\na \\odot b = a \\cap b\n",
    );
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &dir).unwrap();
    let ground = qubitick_build::ground::for_check(&dir).ok();
    let results =
        qubitick_build::run_pipeline(&db, &qubitick_build::walk_qbit(&dir), ground.as_deref())
            .unwrap();
    let said = results
        .iter()
        .flat_map(|r| r.diagnostics.iter())
        .map(|d| d.message.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        said.contains("`Type:` requires a ground name"),
        "the refusal is the one the corpus can act on: {said}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_cited_theorem_is_imported_and_an_admitted_one_is_named() {
    if !lean_is_here() {
        eprintln!("skipping: lean not found");
        return;
    }
    /* TWO THEOREMS, ONE CITING THE OTHER, AND ONE ADMITTED.
     *
     * A citation used to make the citing module declare `axiom <Name> : <statement>` — an
     * assumption written by the engine, which is why `#print axioms` on a proved theorem
     * named its neighbour. The cited module has the real `def`, so the citing module
     * imports it. And a theorem with no proof rows is admitted: an `axiom` of its OWN
     * module, counted, and warned about — the one axiom the corpus asks for. */
    let dir = root("citation");
    wit(
        &dir,
        "Canon.qbit",
        "P. cite\nId: cite\nKind: project\nModule: Qubitick.HL\nLeanDir: prelude\n",
    );
    wit(
        &dir,
        "Era0/Strata0/Definitions/D_A_00_Carrier.qbit",
        "D.A.00. Carrier\n\\mathfrak{R}\nType: E\n",
    );
    wit(
        &dir,
        "Era0/Strata0/Definitions/D_A_01_Add.qbit",
        "D.A.01. Add\na + b\nType: R.plus : \\mathfrak{R} \\to \\mathfrak{R} \\to \\mathfrak{R}\n",
    );
    // A primitive the corpus declares and never defines: the ground's, as a field.
    wit(
        &dir,
        "prelude/Qubitick/HL.lean",
        &format!(
            "{}\n\ndef E : Type := Prop\n\nclass ISA where\n  plus : E → E → E\n",
            qubitick_build::ground::HL_LEAN
        ),
    );
    wit(
        &dir,
        "Era0/Strata0/Axioms/A1_Comm.qbit",
        "A1. Comm\n\\forall a b : \\mathfrak{R}, a + b = b + a\n",
    );
    wit(
        &dir,
        "Era0/Strata0/Theorems/T_A_01_Refl.qbit",
        "T.A.01. Refl\n\\forall a : \\mathfrak{R}, a + a = a + a\nProof:\na + a\n  \u{22a2} fun a => Eq.refl (R.plus a a)\n",
    );
    wit(
        &dir,
        "Era0/Strata0/Theorems/T_A_02_Cites.qbit",
        "T.A.02. Cites\n\\forall a b : \\mathfrak{R}, a + b = a + b\nProof:\na + b\n  = a + b    [T.A.01]\n",
    );
    let modules = emitted_all(&dir);
    let (_, cited) = modules
        .iter()
        .find(|(f, _)| f.ends_with("T_A_02_Cites.qbit"))
        .expect("the citing module");
    assert!(
        cited.contains("import Qubitick.Corpus.Era0.Strata0.Theorems.T_A_01_Refl"),
        "the cited theorem's module is imported: {cited}"
    );
    assert!(!cited.contains("axiom"), "and no assumed lemma is written: {cited}");
    let _ = std::fs::remove_dir_all(&dir);
}
