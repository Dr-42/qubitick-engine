//! Tutorial ship test: the algebra starter corpus must build and verify with
//! school notation (+ and ×) — this is the fixture that must never rot.
//!
//! This test asserts that the engine's tutorial ship (corpus-templates/algebra/)
//! builds 4 nodes, verifies 4, with 0 errors and 0 warnings. It uses the actual
//! `+` and `×` (U+00D7) characters as written in the tutorial, not control words
//! like `\oplus`/`\otimes`. If this test fails, the tutorial is broken.

use std::path::PathBuf;

#[test]
fn tutorial_ship_builds_with_school_notation() {
    // Skip if lean is not available (mirrors other tests in this crate)
    if std::process::Command::new("lean")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping tutorial_ship test: lean not found");
        return;
    }

    let corpus_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus-templates/algebra");

    let mut db = qubitick_db::open_in_memory().unwrap();
    let report = qubitick_build::ingest_corpus(&mut db, &corpus_root)
        .expect("ingest algebra corpus");

    assert_eq!(report.nodes, 4, "algebra corpus has 4 nodes");
    assert!(report.is_clean(), "corpus has no ambiguous references: {:?}", report.duplicates);

    let files: Vec<PathBuf> = qubitick_build::walk_qbit(&corpus_root);
    assert_eq!(files.len(), 4, "algebra corpus has 4 .qbit files");

    let ground = qubitick_build::ground::for_check(&corpus_root).unwrap();
    let results = qubitick_build::run_pipeline(&db, &files, Some(&ground))
        .expect("run pipeline on algebra corpus");

    let built = results.iter().filter(|r| r.ok).count();
    let verified = results.iter().filter(|r| r.verified).count();
    let error_count: usize = results.iter().flat_map(|r| &r.diagnostics).filter(|d| d.is_error()).count();
    let warning_count = qubitick_build::warning_count(&results);

    // The tutorial ship: 4 nodes, all built, all verified, no errors, no warnings.
    assert_eq!(built, 4, "built: expected 4, got {built}");
    assert_eq!(verified, 4, "verified: expected 4, got {verified}");
    assert_eq!(error_count, 0, "error_count: expected 0, got {error_count}; failures: {:?}", results.iter().filter(|r| !r.verified).map(|r| &r.diagnostics).collect::<Vec<_>>());
    assert_eq!(warning_count, 0, "warning_count: expected 0, got {warning_count}");

    // Sanity: the files verified are exactly the four we expect.
    let verified_files: Vec<&str> = results
        .iter()
        .filter(|r| r.verified)
        .map(|r| r.path.as_str())
        .collect();
    assert_eq!(verified_files.len(), 4);
    for f in &verified_files {
        assert!(f.contains("A1_Comm") || f.contains("D_A_01_Add") || f.contains("D_A_02_Mul") || f.contains("T_A_01_MulTwo"),
            "unexpected verified file: {f}");
    }
}