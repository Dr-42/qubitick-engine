//! A PROOF HOLDS NOTATION, AND THE BUILD SAYS SO WHEN IT DOES NOT.
//!
//! The parser refuses a row that is not a term, `= <term> [cite]` or `⊢ <term>`; the build
//! reports that refusal per file, naming the row. Ingestion stays tolerant on purpose: a
//! corpus whose ledger cannot be built is a corpus nobody can open to fix.

use std::path::{Path, PathBuf};

fn root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qb-prose-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn wit(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A corpus that verifies, except that one proof carries a sentence.
fn corpus(dir: &Path) {
    wit(dir, "Canon.qbit", "P. a corpus with prose\nId: prose\n");
    wit(
        dir,
        "Era0/Strata0/Definitions/D_S_01_Add.qbit",
        "D.S.01. Add\na + b\nType: \\mathfrak{R} \\to \\mathfrak{R} \\to \\mathfrak{R}\n",
    );
    wit(
        dir,
        "Era0/Strata0/Axioms/A1_Comm.qbit",
        "A1. Comm\n\\forall a b : \\mathfrak{R}, a + b = b + a\n",
    );
    wit(
        dir,
        "Era0/Strata0/Theorems/T_01_Prose.qbit",
        // The opening expression is notation; the row after it is an English word, which is
        // how the corpus used to close a proof before `⊢` existed.
        "T.S.01. Prose\n\\forall a b : \\mathfrak{R}, a + b = b + a\n\
         Proof:\na + b\nconclude by A1.Comm with a b\n",
    );
}

#[test]
fn a_sentence_in_a_proof_is_reported_and_the_corpus_still_opens() {
    let dir = root("reported");
    corpus(&dir);
    // The ledger is built: the corpus opens.
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &dir)
        .expect("ingestion builds the ledger from what it can read");

    // And the file that carries the sentence fails to build, saying which row.
    let results = qubitick_build::run_pipeline(&db, &qubitick_build::walk_qbit(&dir), None)
        .expect("the pipeline runs");
    let said: Vec<(String, String)> = results
        .iter()
        .flat_map(|r| r.diagnostics.iter())
        .map(|d| (d.code.clone(), d.message.clone()))
        .collect();
    assert!(
        said.iter().any(|(code, msg)| code == "parse.not-notation"
            && msg.contains("conclude by A1.Comm with a b")),
        "the build reports the row it refused: {said:?}"
    );
    assert!(
        results
            .iter()
            .any(|r| r.path.contains("T_01_Prose") && !r.verified),
        "the theorem that holds the sentence does not verify"
    );
    /* ONE MISTAKE, ONE DIAGNOSTIC. The tolerant pass keeps a refused row out of the step
       list, so the theorem LOOKS admitted — and the report used to say both "this row is
       not notation" and "this theorem is stated, not proved". Only the first is what the
       author did: admitted means no `Proof:` rows or `\sorry`, never a row we could not
       read. */
    assert_eq!(
        said.iter().filter(|(c, _)| c == "parse.not-notation").count(),
        1,
        "exactly one diagnostic for the prose row: {said:?}"
    );
    assert!(
        !said.iter().any(|(c, _)| c == "proof.admitted"),
        "a refused row is not an admitted proof: {said:?}"
    );
    assert_eq!(
        qubitick_build::pipeline::admitted_count(&results),
        0,
        "and the report's admitted count agrees"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_same_proof_written_with_the_turnstile_is_notation() {
    let dir = root("turnstile");
    corpus(&dir);
    wit(
        &dir,
        "Era0/Strata0/Theorems/T_01_Prose.qbit",
        "T.S.01. Prose\n\\forall a b : \\mathfrak{R}, a + b = b + a\nProof:\na + b\n  = b + a    [A1.Comm]\n⊢ h\n",
    );
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &dir).unwrap();
    let results = qubitick_build::run_pipeline(&db, &qubitick_build::walk_qbit(&dir), None)
        .expect("the pipeline runs");
    let said: Vec<String> = results
        .iter()
        .flat_map(|r| r.diagnostics.iter())
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert!(
        !said.iter().any(|m| m.starts_with("parse.not-notation")),
        "`⊢` is notation, so nothing is refused: {said:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}