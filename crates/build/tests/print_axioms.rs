//! PRINT-AXIOMS: what a theorem stands on, as LEAN reports it.
//!
//! Reported, never policed. The ground decides the answer, so the same theorem can print
//! two different footprints and both are true: a project on an Init ground stands on the
//! three axioms Lean's core carries, and a project on the engine's bare prelude stands on
//! nothing. `crates/build/src/lean_check.rs::axioms_footprint` hands Lean the module text
//! plus `#print axioms <name>` and reads its own lines back.

use std::path::{Path, PathBuf};

fn root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qb-ax-{tag}-{}", std::process::id()));
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

/// Footprint of one file's node, through the same path the CLI takes.
fn footprint(dir: &Path, rel: &str) -> Vec<String> {
    let file = dir.join(rel);
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, dir).unwrap();
    let ctx = qubitick_ctx::build_ctx(&db, "Era0/Strata0").unwrap();
    let src = std::fs::read_to_string(&file).unwrap();
    let nodes = qubitick_parse::parse_content(&ctx, &src).unwrap();
    let names: Vec<String> = nodes.iter().map(|n| n.base().name.clone()).collect();
    let emitted = qubitick_build::pipeline::emit_file(&ctx, &db, &file).unwrap();
    let ground = qubitick_build::ground::for_check(dir).unwrap();
    qubitick_build::lean_check::axioms_footprint(Some(&ground), &emitted, &names)
        .unwrap()
        .into_iter()
        .flat_map(|f| f.axioms)
        .collect()
}

/// A corpus whose theorem's TYPE mentions a name from Init's `Classical`, with a `rfl`
/// proof — so the footprint is the type's, not the proof term's.
fn init_ground_corpus(dir: &Path) {
    wit(dir, "Canon.qbit", "P. on Init\nId: init\nLeanDir: lean\nModule: Qubitick.HL\n");
    // The ground is Init itself: its 16 contract names are Init's own.
    wit(dir, "lean/Qubitick/HL.lean", "import Init\n");
    wit(
        dir,
        "Era0/Strata0/Definitions/D_S_01_Truth.qbit",
        "D.S.01. Truth\n\\truth\nLean: True\nRole: null\n",
    );
    wit(
        dir,
        "Era0/Strata0/Definitions/D_S_02_Em.qbit",
        "D.S.02. Em\n\\em\nLean: Classical.em\n",
    );
    wit(
        dir,
        "Era0/Strata0/Theorems/T_01_EmRefl.qbit",
        "T.S.01. EmRefl\n\\em(\\truth) = \\em(\\truth)\nProof:\n\\em(\\truth)\n  = \\em(\\truth)\n",
    );
}

/// The same theorem on the engine's BARE prelude: `Classical` is not imported, so nothing
/// of Init's is reachable and the proof is a kernel reflexive one.
fn bare_ground_corpus(dir: &Path) {
    wit(dir, "Canon.qbit", "P. bare\nId: bare\n");
    wit(
        dir,
        "Era0/Strata0/Definitions/D_S_01_Truth.qbit",
        "D.S.01. Truth\n\\truth\nLean: True\nRole: null\n",
    );
    wit(
        dir,
        "Era0/Strata0/Theorems/T_01_Refl.qbit",
        "T.S.01. Refl\n\\truth = \\truth\nProof:\n\\truth\n  = \\truth\n",
    );
}

#[test]
fn an_init_ground_theorem_prints_inits_three_axioms() {
    if !lean_is_here() {
        eprintln!("skipping: lean not found");
        return;
    }
    let dir = root("init");
    init_ground_corpus(&dir);
    let mut found = footprint(&dir, "Era0/Strata0/Theorems/T_01_EmRefl.qbit");
    found.sort();
    let mut expected = vec![
        "propext".to_string(),
        "Classical.choice".to_string(),
        "Quot.sound".to_string(),
    ];
    expected.sort();
    assert_eq!(
        found, expected,
        "the project's Init ground is what a theorem stands on, named as Lean names it"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_bare_prelude_theorem_prints_nothing() {
    if !lean_is_here() {
        eprintln!("skipping: lean not found");
        return;
    }
    let dir = root("bare");
    bare_ground_corpus(&dir);
    let found = footprint(&dir, "Era0/Strata0/Theorems/T_01_Refl.qbit");
    assert!(
        found.is_empty(),
        "the engine's bare prelude carries no axioms, so its theorems stand on none: {found:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_ship_prints_the_admissions_it_does_make() {
    if !lean_is_here() {
        eprintln!("skipping: lean not found");
        return;
    }
    // Not a gate, a fact: the tutorial ship's theorem stands on the axioms the ENGINE
    // emitted for its own declarations (`plus` is the corpus's primitive, `times` and
    // `times_def` are its definition seen from a module that cannot unfold it). The
    // footprint shows them, and the number is the corpus author's to lower — by binding
    // a glyph to what the ground already provides, or by declaring the carrier.
    let ship = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus-templates/algebra");
    let found = footprint(&ship, "Era0/Strata0/Theorems/T_A_01_MulTwo.qbit");
    assert!(
        found.contains(&"plus".to_string()) && found.contains(&"times".to_string()),
        "the ship's theorem stands on its own declarations, named as Lean names them: {found:?}"
    );
}

#[test]
fn leans_two_sentences_are_read_as_they_come() {
    // The reader handles exactly the two shapes Lean prints, including an empty
    // footprint, without inventing a third.
    let output = "'t4' depends on axioms: [propext, Classical.choice, Quot.sound]\n\
                  't5' does not depend on any axioms\n\
                  some unrelated line\n";
    let printed: Vec<(String, usize)> = qubitick_build::lean_check::footprint_lines(output)
        .into_iter()
        .map(|f| (f.name, f.axioms.len()))
        .collect();
    assert_eq!(
        printed,
        vec![
            ("t4".to_string(), 3),
            ("t5".to_string(), 0)
        ]
    );
}