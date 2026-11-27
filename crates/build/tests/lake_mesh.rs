//! LAKE, BOTH SHAPES: the ship must build as one module per node AND as one package.
//!
//! `qubitick lake` has two shapes, and only one of them was ever exercised:
//!
//!   · `Shape::PerNode` — one module per node, each checked in its own environment;
//!   · `Shape::Meshed` — one era ground that DECLARES the classes and the definitions, plus
//!     one module per scope that imports it. "One environment, nothing declared twice" is the
//!     property the second shape exists for, and it is the shape a corpus of several eras needs.
//!
//! The meshed shape was broken twice over, and both failures were invisible because nothing
//! built it (the CLI's `lake --mesh` is the only caller, and it has no test):
//!
//!  1. `emit_lean_text_meshed` handed the module an EMPTY `home_modules`, so the first
//!     `Sema:` definition a scope module named had no home and the emit aborted. The map
//!     cannot be rebuilt from the parsed nodes: `home_node` is a DB node id (4 for `times`)
//!     while `parse_content` numbers nodes per file (0, 1, 2).
//!  2. The era ground wrote its definitions BEFORE the classes and wrote a class field bare
//!     (`plus`) where the field is only reachable through the instance (`R.plus`) — so the
//!     ground did not compile either.
//!
//! This file holds both: the packages build, and the ground's own text keeps the order that
//! makes it compile. The TEXT assertions run without Lean installed (they are about the
//! emitter); the BUILD assertions skip when there is no `lean`/`lake` to run.

use std::path::{Path, PathBuf};

/// A private copy of the ship: `write_package` writes the package INSIDE the corpus
/// (`<corpus>/.lake`), so building the repository's template would leave a package in
/// the tree every test shares.
fn ship_copy(tag: &str) -> PathBuf {
    let from = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus-templates/algebra");
    let to = std::env::temp_dir().join(format!("qb-lake-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&to);
    copy_tree(&from, &to);
    to
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        // build residue: the package is what we are about to write, not what was there
        if name == ".lake" {
            continue;
        }
        let src = entry.path();
        let dst = to.join(&name);
        if src.is_dir() {
            copy_tree(&src, &dst);
        } else {
            std::fs::copy(&src, &dst).unwrap();
        }
    }
}

/// Ingest the corpus and build the context the way `qubitick lake` does: the corpus's own
/// first node is the scope (a command with no file argument has no file's scope to use).
fn ctx_of(corpus: &Path) -> (qubitick_db::Db, qubitick_ctx::Ctx) {
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, corpus).expect("ingest the ship");
    let scope = qubitick_build::walk_qbit(corpus)
        .first()
        .map(|f| qubitick_build::pipeline::scope_of_path(f))
        .unwrap_or_else(|| qubitick_build::pipeline::scope_of_path(corpus));
    let ctx = qubitick_ctx::build_ctx(&db, &scope).expect("context for the corpus scope");
    (db, ctx)
}

fn lean_is_here() -> bool {
    std::process::Command::new("lean").arg("--version").output().is_ok()
}

fn lake_is_here() -> bool {
    std::process::Command::new("lake")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn both_shapes_write_their_package() {
    let corpus = ship_copy("write");
    let (db, ctx) = ctx_of(&corpus);
    let out = qubitick_build::lake::package_dir(&corpus);

    let per_node =
        qubitick_build::lake::write_package(&ctx, &db, &corpus, &out, qubitick_build::lake::Shape::PerNode)
            .expect("the per-node package writes");
    assert_eq!(per_node.len(), 5, "one module per node: {per_node:?}");

    let meshed =
        qubitick_build::lake::write_package(&ctx, &db, &corpus, &out, qubitick_build::lake::Shape::Meshed)
            .expect("the meshed package writes");
    // An era ground plus one module per scope: two, for a corpus with one era and one stratum.
    assert_eq!(meshed.len(), 2, "one ground and one scope module: {meshed:?}");

    /* THE SCOPE MODULE IMPORTS THE GROUND. This is what makes the shape's promise — one
       environment, nothing declared twice — true: the classes and the definitions are the
       ground's, and a scope module names them through the import. */
    let scope_module = out.join("src/Qubitick/Corpus/Era0/Strata0.lean");
    let text = std::fs::read_to_string(&scope_module).expect("the scope module is on disk");
    assert!(
        text.contains("import Qubitick.Corpus.Era0.Ground"),
        "the scope module must import the era ground:\n{text}"
    );
}

#[test]
fn the_meshed_ground_declares_the_class_before_the_definition_that_uses_its_field() {
    let corpus = ship_copy("ground");
    let (db, ctx) = ctx_of(&corpus);
    let out = qubitick_build::lake::package_dir(&corpus);
    qubitick_build::lake::write_package(&ctx, &db, &corpus, &out, qubitick_build::lake::Shape::Meshed)
        .expect("the meshed package writes");

    let ground = std::fs::read_to_string(out.join("src/Qubitick/Corpus/Era0/Ground.lean"))
        .expect("the era ground is on disk");

    let class_at = ground.find("class ISA where").expect("the ground declares the root class");
    let def_at = ground.find("def times").expect("the ground declares the definition");
    assert!(
        class_at < def_at,
        "the class that carries the field must come before the definition that uses it:\n{ground}"
    );

    /* THE FIELD IS NAMED THROUGH THE INSTANCE. `plus` is a field of `class ISA`; outside the
       class it is only reachable as `R.plus`, and the section is what binds `R`. Written
       bare the module is `Unknown identifier \`plus\`` — the failure that made this shape
       unusable. */
    assert!(
        ground.contains("R.plus"),
        "the definition's body must reach the field through the instance:\n{ground}"
    );
    let section_at = ground.find("variable [R : ").expect("the ground hypothesizes the instance");
    assert!(
        section_at < def_at,
        "the section must open before the definition that needs it:\n{ground}"
    );
    assert!(
        ground[def_at..].contains("end"),
        "the section must close after the definition:\n{ground}"
    );
}

#[test]
fn both_shapes_build_the_ship() {
    if !lean_is_here() || !lake_is_here() {
        eprintln!("skipping: no lean/lake on PATH, so neither package can be built");
        return;
    }
    let corpus = ship_copy("build");
    let (db, ctx) = ctx_of(&corpus);
    let out = qubitick_build::lake::package_dir(&corpus);

    for (shape, label) in [
        (qubitick_build::lake::Shape::PerNode, "per-node"),
        (qubitick_build::lake::Shape::Meshed, "meshed"),
    ] {
        qubitick_build::lake::write_package(&ctx, &db, &corpus, &out, shape)
            .unwrap_or_else(|e| panic!("the {label} package writes: {e}"));
        let report = qubitick_build::lake::build(&out)
            .unwrap_or_else(|e| panic!("lake build runs for {label}: {e}"));
        assert!(
            report.built,
            "the {label} package builds; failures: {:#?}",
            report.failures
        );
    }
}