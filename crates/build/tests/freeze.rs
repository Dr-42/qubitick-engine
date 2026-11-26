//! THE FREEZE: a ground is one file, and its hash is how a build says which one.
//!
//! The threat a corpus faces is somebody making a proof pass by editing the ground — the
//! one file that carries a corpus's assumptions — and adding an `axiom`. `--frozen` is the
//! audit trail for that: the hash is printed by `qubitick hash-ground`, and a build whose
//! ground does not hash to the frozen value is refused before a line of Lean is written.

use std::path::{Path, PathBuf};

fn root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qb-freeze-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Era0/Strata0/Definitions")).unwrap();
    std::fs::write(
        dir.join("Canon.qbit"),
        "P. frozen\nId: frozen\nKind: project\nModule: Qubitick.HL\nLeanDir: prelude\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Era0/Strata0/Definitions/D_01_Truth.qbit"),
        "D.X.01. Truth\n\\truth\nType: True\n",
    )
    .unwrap();
    dir
}

#[test]
fn the_hash_is_stable_and_follows_the_file() {
    let dir = root("stable");
    let path = qubitick_build::presets::seed(&dir, "init").expect("seed");
    assert!(path.ends_with("Qubitick/HL.lean"), "{}", path.display());
    // Seeding names the ground in the charter, or the build never reads it.
    let charter = std::fs::read_to_string(dir.join("Canon.qbit")).unwrap();
    assert!(charter.contains("LeanDir: prelude"), "{charter}");

    let first = qubitick_build::presets::hash(&dir).unwrap();
    let again = qubitick_build::presets::hash(&dir).unwrap();
    assert_eq!(first, again, "the same ground hashes the same");

    // An edit is a different ground.
    std::fs::write(&path, "import Init\naxiom whatever : False\n").unwrap();
    let edited = qubitick_build::presets::hash(&dir).unwrap();
    assert_ne!(first, edited, "an edit changes the hash");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_frozen_build_accepts_its_own_hash_and_refuses_another() {
    let dir = root("frozen");
    qubitick_build::presets::seed(&dir, "init").expect("seed");
    let hash = qubitick_build::presets::hash(&dir).unwrap();
    qubitick_build::presets::check_frozen(&dir, &hash).expect("the frozen hash matches");

    let d = qubitick_build::presets::freeze_diagnostic(&dir, "0000000000000000")
        .expect("a readable ground")
        .expect("a different hash is refused");
    assert_eq!(d.code, "ground.frozen");
    assert_eq!(d.message, "the ground is not the file that was frozen");
    let said = d.hints.join("\n");
    assert!(said.contains("Qubitick/HL.lean"), "the refusal names the file: {said}");
    assert!(said.contains(&hash), "and both hashes: {said}");
    assert!(
        said.contains("update the frozen hash"),
        "and says what a person can do about it: {said}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_preset_is_a_ground_a_project_can_start_from() {
    for (preset, expected) in [
        ("init", "import Init"),
        ("mathlib", "import Mathlib"),
        ("bare", "prelude"),
        ("custom", ""),
    ] {
        let text = qubitick_build::presets::text(preset).expect(preset);
        if expected.is_empty() {
            assert!(text.is_empty(), "`custom` starts blank: {text}");
        } else {
            assert!(text.starts_with(expected), "`{preset}` starts with {expected}: {text}");
        }
    }
    assert!(qubitick_build::presets::text("nope").is_err(), "an unknown preset is refused");
}

#[test]
fn a_scaffolded_ground_names_the_chain_and_writes_no_axiom() {
    let ship = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus-templates/algebra");
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &ship).unwrap();
    let text = qubitick_build::scaffold::scaffold(&db, &ship, "Qubitick.HL").unwrap();
    assert!(text.contains("class ISA where"), "{text}");
    assert!(text.contains("plus : "), "the named symbol is a field: {text}");
    assert!(text.contains("class Comm extends ISA"), "{text}");
    assert!(text.contains("ax_0"), "{text}");
    assert!(!text.contains("axiom"), "a scaffolded ground assumes nothing by axiom: {text}");
    assert!(!text.contains("import "), "a ground does not import itself: {text}");
}