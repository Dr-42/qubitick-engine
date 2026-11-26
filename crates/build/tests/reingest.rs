use std::path::PathBuf;

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("qb-reingest-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("Era0/Strata0/Definitions")).unwrap();
    std::fs::write(d.join("Canon.qbit"), "P. re\nId: re\n").unwrap();
    std::fs::write(
        d.join("Era0/Strata0/Definitions/D_01_Both.qbit"),
        "D.X.01. Both\n\\oplus a b\nType: Some.name\nSema:\n\\oplus a b = a\n",
    )
    .unwrap();
    std::fs::write(
        d.join("Era0/Strata0/Definitions/D_02_Truth.qbit"),
        "D.X.02. Truth\n\\truth\nType: True\n",
    )
    .unwrap();
    d
}

/// A CORPUS WITH A FILE IT CANNOT READ OPENS, TWICE.
///
/// The interface opens a project (one ingest) and then builds it (another ingest into
/// the same DB), so `IngestReport::refused` has to be stable across both: a corpus that
/// opens once and refuses the second time is a project the interface cannot build — which
/// is exactly what a corpus with a bad file did before the refusal moved from the corpus
/// to the file.
#[test]
fn a_corpus_with_a_refused_file_ingests_twice() {
    let d = dir("twice");
    let mut db = qubitick_db::open_in_memory().unwrap();
    let first = qubitick_build::ingest_corpus(&mut db, &d);
    println!("first: {:?}", first.as_ref().map(|r| (r.nodes, r.refused.len())).map_err(|e| format!("{e:#}")));
    let second = qubitick_build::ingest_corpus(&mut db, &d);
    println!("second: {:?}", second.as_ref().map(|r| (r.nodes, r.refused.len())).map_err(|e| format!("{e:#}")));
    let _ = std::fs::remove_dir_all(&d);
    assert!(first.is_ok(), "the first ingest opens the corpus");
    assert!(second.is_ok(), "and so does a re-ingest into the same DB");
}
