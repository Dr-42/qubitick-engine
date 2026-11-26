//! THE COVERAGE INVARIANT: `glyphs(A) ⊆ glyphs(D)`, per era/stratum.
//!
//! An A block is the corpus's grammar — symbols and the ways they compose — and it emits
//! nothing. Every terminal it writes must still be homed by a D node in the same stratum,
//! or the display cannot be resolved at all. The engine refuses to invent that meaning,
//! which is what it used to do by emitting the glyph as an `axiom`.
//!
//! Measured on the SHIPPED ship rather than a hand-written snippet: the criterion is that
//! removing a D node from `corpus-templates/algebra` produces the error that names the
//! glyph it orphaned.

use std::path::{Path, PathBuf};

/// The shipped ship, copied as `.qbit` files only — the `.lake` tree beside it holds the
/// previous emission and is not part of a corpus.
fn ship(tag: &str) -> PathBuf {
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus-templates/algebra");
    let dir = std::env::temp_dir().join(format!("qb-cov-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy_qbit(&from, &dir);
    dir
}

fn copy_qbit(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        // `.lake` is a previous emission and `canon.bit`/the db are build artifacts;
        // everything else — the nodes AND the project's own ground — is the corpus.
        if name == ".lake" || name.ends_with(".bit") || name.starts_with("qubitick.db") {
            continue;
        }
        let target = to.join(entry.file_name());
        if path.is_dir() {
            copy_qbit(&path, &target);
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

fn build(dir: &Path) -> Vec<qubitick_build::pipeline::BuildFileResult> {
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, dir).expect("ingest");
    let ground = qubitick_build::ground::for_check(dir).expect("the ground assembles");
    qubitick_build::run_pipeline(&db, &qubitick_build::walk_qbit(dir), Some(&ground))
        .expect("the pipeline runs")
}

#[test]
fn a_glyph_an_a_block_writes_and_no_d_node_homes_is_refused_by_name() {
    if !lean_is_here() {
        eprintln!("skipping: lean not found");
        return;
    }
    let dir = ship("orphan");
    // The carrier's home: `A1. Comm` writes `\mathfrak{R}` in its binder, so without this
    // node the A block composes over a glyph the stratum does not declare.
    let carrier = dir.join("Era0/Strata0/Definitions/D_A_00_Carrier.qbit");
    assert!(carrier.is_file(), "the ship homes its carrier");
    std::fs::remove_file(&carrier).unwrap();

    let results = build(&dir);
    let said: Vec<(String, String)> = results
        .iter()
        .flat_map(|r| r.diagnostics.iter())
        .filter(|d| d.is_error())
        .map(|d| (d.code.clone(), d.message.clone()))
        .collect();
    assert!(
        said.iter()
            .any(|(code, m)| code == "coverage.unhomed" && m.contains("`\\mathfrak{R}`")),
        "the refusal names the glyph it orphaned: {said:?}"
    );
    assert!(
        said.iter().any(|(_, m)| m.contains("Era0/Strata0")),
        "the refusal names the stratum whose D nodes are silent: {said:?}"
    );
    assert!(
        said.iter().any(|(_, m)| m.contains("A1_Comm.qbit")),
        "the refusal names the A block the glyph appears in: {said:?}"
    );
    assert!(
        results.iter().any(|r| !r.path.ends_with("A1_Comm.qbit") && r.verified),
        "a coverage hole is reported PER FILE: the rest of the corpus still builds"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn scaffold_writes_the_chain_the_a_blocks_imply() {
    let dir = ship("scaffold");
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &dir).expect("ingest");
    let text = qubitick_build::scaffold::scaffold(&db, &dir, "Qubitick.HL").expect("scaffold");
    assert!(text.contains("class ISA"), "{text}");
    assert!(text.contains("class Comm extends ISA"), "{text}");
    assert!(text.contains("ax_0"), "{text}");
    assert!(!text.contains("axiom"), "a scaffolded ground writes no axiom: {text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_shipped_ship_homes_every_glyph_its_a_blocks_write() {
    if !lean_is_here() {
        eprintln!("skipping: lean not found");
        return;
    }
    let dir = ship("whole");
    let results = build(&dir);
    let said: Vec<String> = results
        .iter()
        .flat_map(|r| r.diagnostics.iter())
        .filter(|d| d.is_error())
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert!(said.is_empty(), "the ship covers its own grammar: {said:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
#[test]
fn a_line_that_resolves_to_nothing_names_the_glyphs_it_writes() {
    /* THE FALSE GREEN, CLOSED.
     *
     * A corpus that names a glyph NOTHING declares still opens: ingestion writes a use-only
     * row, the parser keeps the statement it cannot read as an opaque `Var(<whole line>)`,
     * and an A block emits nothing anyway — so the build came out `verified: 1, errors: 0`
     * and said nothing at all about a statement that means nothing. The line's control words
     * are read off its text now, and each one nothing declares is named. */
    let dir = std::env::temp_dir().join(format!("qb-cov-unresolved-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Era0/Strata0/Axioms")).unwrap();
    std::fs::write(dir.join("Canon.qbit"), "P. unresolved\nId: unresolved\n").unwrap();
    std::fs::write(
        dir.join("Era0/Strata0/Axioms/A1_Base.qbit"),
        "A1. Base\n\\forall a b : \\mathfrak{E}, a \\Delta b = b \\Delta a\n",
    )
    .unwrap();

    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &dir).unwrap();
    let said: Vec<String> = qubitick_build::coverage::gaps(&db, &qubitick_build::walk_qbit(&dir))
        .unwrap()
        .into_iter()
        .map(|g| g.sentence())
        .collect();
    assert!(
        said.iter().any(|m| m.contains("`\\Delta`")),
        "the operator the corpus never declares is named: {said:?}"
    );
    assert!(
        said.iter().any(|m| m.contains("`\\mathfrak{E}`")),
        "and so is the carrier: {said:?}"
    );
    assert!(
        said.iter().all(|m| m.contains("never declares it")),
        "each names the glyph and says the line resolves to nothing: {said:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
