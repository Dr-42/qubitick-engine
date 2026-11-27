//! THE CORPUS DOCUMENT COMPILES — the PDF export is a deliverable, not a shape.
//!
//! `/api/pdf` used to answer 500 whenever `tectonic` was absent, and the LaTeX it produced had
//! three defects that no test could see because nothing ever ran a TeX engine over it:
//!
//! * `\noindent\texttt{Type}:\ \[E\]` — display maths opened inside a text-mode paragraph,
//!   which pdflatex reports as "Bad math environment delimiter" and refuses;
//! * `\usepackage{...blue!50!black...}` with no `xcolor`, so `hyperref`'s colour spec was an
//!   undefined macro;
//! * `×` and the other Unicode operators, which the T1 font encoding does not carry.
//!
//! This test compiles the starter ship's document when a TeX engine is installed, and says so
//! when none is. The document is also checked without an engine: the anchor test needs no
//! compiler, and it is the one that would have caught the duplicate `D.A.02`.

use std::path::{Path, PathBuf};


fn ship(tag: &str) -> PathBuf {
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus-templates/algebra");
    let dir = std::env::temp_dir().join(format!("qb-pdf-{tag}-{}", std::process::id()));
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
        if name == ".lake" || name.ends_with(".bit") || name.starts_with("qubitick.db")
            || name == "prelude"
        {
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

/// The document the ship produces, exactly as `/api/pdf` would build it.
fn document(dir: &Path) -> String {
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, dir).expect("ingest");
    let ctx = qubitick_ctx::build_ctx(&db, "Era0/Strata0").expect("ctx");
    let headers = qubitick_db::read::get_node_list(&db).expect("headers");
    let sections = qubitick_build::pdf_sections(&ctx, &headers).expect("sections");
    qubitick_pdf::latex::document_sections(&sections)
}

/// Every `\hypertarget` and `\hyperlink` key in a document.
fn keys(doc: &str, verb: &str) -> Vec<String> {
    let mut out = Vec::new();
    let needle = format!("\\{verb}{{cite:");
    let mut rest = doc;
    while let Some(at) = rest.find(&needle) {
        rest = &rest[at + needle.len()..];
        let Some(end) = rest.find('}') else { break };
        out.push(rest[..end].to_string());
        rest = &rest[end..];
    }
    out
}

#[test]
fn two_nodes_never_share_a_cite_id() {
    /* THE CARRIER IS `D.A.00`, AND IT WENT IN AS `D.A.02`.
     *
     * The ship's first definition is numbered 00; the parser reads that header number as 0,
     * and every surface took 0 to mean "carries no number" — so the label fell back to the
     * ledger ordinal (2), which is Mul's number. Two nodes, one anchor: `hyperref` warns, and
     * every citation of Mul landed on the carrier.
     *
     * A corpus that numbers a node 00 means it, so the label is `D.A.00`. The guard here is
     * what keeps the collision from coming back in some other corpus. */
    let dir = ship("anchors");
    let doc = document(&dir);
    let mut targets = keys(&doc, "hypertarget");
    assert!(targets.contains(&"D.A.00".to_string()), "the carrier is numbered 00: {targets:?}");
    assert!(targets.contains(&"D.A.02".to_string()), "Mul is numbered 02: {targets:?}");
    let total = targets.len();
    targets.sort();
    targets.dedup();
    assert_eq!(targets.len(), total, "no two nodes share an anchor: {doc}");
    // Every link has a target. A document full of links to nothing is worse than one with none.
    for link in keys(&doc, "hyperlink") {
        assert!(targets.contains(&link), "`{link}` is linked but never declared");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_preamble_carries_what_the_body_uses() {
    let dir = ship("preamble");
    let doc = document(&dir);
    assert!(doc.contains("\\usepackage{xcolor}"), "hyperref's blue!50!black needs xcolor");
    assert!(doc.contains("\\usepackage{newunicodechar}"), "the corpus writes `×`");
    assert!(doc.contains("\\newunicodechar{×}"), "and it is mapped to maths");
    assert!(doc.contains("\\title{Qubitick Corpus}"), "the title is spelled right");
    assert!(!doc.contains("Quibitick") && !doc.contains("Quibque"), "no misspelling survives");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn no_display_maths_opens_inside_a_text_run() {
    /* `\noindent\texttt{Type}:\ [E]` is the exact shape pdflatex refuses: `\[` directly after
       a text-mode group in a live paragraph. The display gets its own paragraph. */
    let dir = ship("mathmode");
    let doc = document(&dir);
    for line in doc.lines() {
        if !line.contains("\\[") {
            continue;
        }
        assert!(
            !line.contains("\\texttt{"),
            "display maths shares a line with a text-mode group: {line}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_ship_document_compiles() {
    let dir = ship("compile");
    let doc = document(&dir);
    let Ok(engine) = qubitick_pdf::engine::find() else {
        eprintln!("skipping: no LaTeX engine installed (tectonic, pdflatex or xelatex)");
        let _ = std::fs::remove_dir_all(&dir);
        return;
    };
    let work = dir.join("pdf");
    match qubitick_pdf::engine::compile_with(engine, &doc, &work) {
        Ok(bytes) => {
            assert!(bytes.starts_with(b"%PDF"), "{} produced a PDF", engine.as_str());
            assert!(bytes.len() > 1024, "and it has a page in it: {} bytes", bytes.len());
        }
        Err(e) => panic!("{} refused the ship's document: {e}\n--- source ---\n{doc}", engine.as_str()),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_machine_with_no_engine_is_told_which_ones_and_what_to_do() {
    if qubitick_pdf::engine::ENGINES
        .iter()
        .any(|e| qubitick_pdf::engine::installed(e.as_str()))
    {
        eprintln!("skipping: this machine has a LaTeX engine");
        return;
    }
    let said = qubitick_pdf::engine::find().unwrap_err().to_string();
    for name in ["tectonic", "pdflatex", "xelatex"] {
        assert!(said.contains(name), "{name} is named: {said}");
    }
    assert!(said.contains("--latex"), "and the source is offered: {said}");
}
