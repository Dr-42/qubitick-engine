//! THE GROUND A CORPUS CAN ACTUALLY COMPILE: seed a preset, scaffold its chain, build.
//!
//! A scaffold is the one thing a corpus with A blocks needs and cannot write for itself —
//! the class chain its laws become. Two things went wrong here and both are pinned now:
//!
//! * the carrier collapsed to `Prop`. The corpus says `Type: E`, and a `Sema:` signature
//!   over it read `plus : (Prop) → (Prop) → Prop`: proof-irrelevant, and not what the
//!   corpus says. A glyph the GROUND names is the ground's, carrier included;
//! * the carrier was not declared at all, so the scaffolded ground did not compile on a
//!   fresh seed — `Unknown identifier Comm` was the first thing a new project met.

use std::path::{Path, PathBuf};

/// The shipped ship, copied so seeding and scaffolding do not touch the template.
fn ship(tag: &str) -> PathBuf {
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus-templates/algebra");
    let dir = std::env::temp_dir().join(format!("qb-scaffold-{tag}-{}", std::process::id()));
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
        // The template's own ground is what seeding replaces; the rest is the corpus.
        if name == ".lake" || name.ends_with(".bit") || name.starts_with("qubitick.db") || name == "prelude" {
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

fn lean_is_here() -> bool {
    std::process::Command::new("lean")
        .arg("--version")
        .output()
        .is_ok()
}

/// Seed a preset, append the scaffold to it — what `qubitick seed` then `qubitick scaffold
/// --write` do — and return the corpus.
fn seeded_and_scaffolded(dir: &Path, preset: &str) {
    let ground = qubitick_build::presets::seed(dir, preset).expect("seed");
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, dir).expect("ingest");
    let chain = qubitick_build::scaffold::scaffold(&db, dir, "Qubitick.HL").expect("scaffold");
    let mut text = std::fs::read_to_string(&ground).unwrap_or_default();
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&chain);
    text.push('\n');
    std::fs::write(&ground, text).unwrap();
}

#[test]
fn the_scaffold_keeps_the_carrier_the_corpus_names() {
    let dir = ship("carrier");
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &dir).expect("ingest");
    let chain = qubitick_build::scaffold::scaffold(&db, &dir, "Qubitick.HL").expect("scaffold");
    assert!(
        chain.contains("def E : Type := Prop"),
        "the carrier the corpus names is declared: {chain}"
    );
    assert!(
        chain.contains("plus : (E) → (E) → E"),
        "and the symbols over it are typed over IT, not over `Prop`: {chain}"
    );
    assert!(
        chain.contains("∀ (a : E)"),
        "the law quantifies over the carrier: {chain}"
    );
    assert!(
        !chain.contains("(Prop) → (Prop)"),
        "nothing collapses the carrier to `Prop`: {chain}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn seed_then_scaffold_gives_a_ground_the_ship_builds_against() {
    if !lean_is_here() {
        eprintln!("skipping: lean not found");
        return;
    }
    for preset in ["init", "bare"] {
        let dir = ship(&format!("build-{preset}"));
        seeded_and_scaffolded(&dir, preset);
        let mut db = qubitick_db::open_in_memory().unwrap();
        qubitick_build::ingest_corpus(&mut db, &dir).expect("ingest");
        let ground = qubitick_build::ground::for_check(&dir).expect("the seeded ground assembles");
        let results =
            qubitick_build::run_pipeline(&db, &qubitick_build::walk_qbit(&dir), Some(&ground))
                .unwrap();
        let errors: Vec<String> = results
            .iter()
            .flat_map(|r| r.diagnostics.iter())
            .filter(|d| d.is_error())
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect();
        assert!(
            errors.is_empty(),
            "`{preset}` + scaffold must build clean: {errors:?}"
        );
        assert_eq!(results.len(), 5, "the ship is five files on `{preset}`");
        assert!(
            results.iter().all(|r| r.verified),
            "all five verify on `{preset}`: {:?}",
            results
                .iter()
                .filter(|r| !r.verified)
                .map(|r| (r.path.clone(), r.diagnostics.first().cloned()))
                .collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn a_scaffold_that_is_appended_keeps_the_contract() {
    // The preset carries `prelude`/`import Init`; the scaffold is the corpus's own part of
    // the same file. Overwriting it dropped the contract and the ground never compiled.
    let dir = ship("append");
    let ground = qubitick_build::presets::seed(&dir, "init").expect("seed");
    let before = std::fs::read_to_string(&ground).unwrap();
    assert!(before.starts_with("import Init"), "{before}");
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &dir).expect("ingest");
    let chain = qubitick_build::scaffold::scaffold(&db, &dir, "Qubitick.HL").expect("scaffold");
    std::fs::write(&ground, format!("{before}{chain}\n")).unwrap();
    let after = std::fs::read_to_string(&ground).unwrap();
    assert!(after.starts_with("import Init"), "the contract survives: {after}");
    assert!(after.contains("class ISA where"), "and the chain is there: {after}");
    let _ = std::fs::remove_dir_all(&dir);
}