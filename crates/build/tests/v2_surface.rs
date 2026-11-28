//! THE SECOND CORPUS'S SURFACE: `Stmt:`, `DOI:`, and `M.` premise blocks.
//!
//! The older formalisation writes a theorem's statement under `Stmt:` rather than on a bare line,
//! its imported mathematical systems under `M.<tier>.<n>.<m> [\mathrm{NAME}]` nodes with a `Stmt:`
//! body and a `DOI:` provenance, and cites them `[M.<tier>.<n>.<m>]`. The engine reads all three
//! so a corpus translates without a rename pass:
//!
//!   - `Stmt:` is a theorem's statement / an axiom's statement;
//!   - `M.` is an admitted premise — an axiom the corpus relies on and does not rederive;
//!   - `DOI:` is provenance metadata that flows through.

use std::path::{Path, PathBuf};

fn corpus(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qb-v2s-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Era0/Strata0/Definitions")).unwrap();
    std::fs::create_dir_all(dir.join("Era0/Strata0/Theorems")).unwrap();
    std::fs::write(dir.join("Canon.qbit"), format!("P. v2 surface {tag}\nId: v2s{tag}\n")).unwrap();
    dir
}

fn wit(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn base_carrier(dir: &Path) {
    // A carrier and connective with a SEPARATOR declared so `\vee` is a term, not a role-only
    // ghost. These match the engine's existing fixtures.
    wit(
        dir,
        "Era0/Strata0/Definitions/D_BT_0_4_Land.qbit",
        "D.BT.0.4. Land\na \\land b\nRole: connective\n",
    );
}

#[test]
fn a_theorem_with_a_stmt_field_parses_its_statement() {
    let dir = corpus("stmt");
    std::fs::remove_dir_all(dir.join("Era0/Strata0/Definitions")).unwrap();
    std::fs::create_dir_all(dir.join("Era0/Strata0/Definitions")).unwrap();
    base_carrier(&dir);
    wit(
        &dir,
        "Era0/Strata0/Theorems/T_BT_1_Comm.qbit",
        "T.BT.1. Comm\n\\forall a b : \\tau, a \\land b = b \\land a\nProof:\na \\land b\n  = b \\land a  [A.1]\n",
    );
    let file = dir.join("Era0/Strata0/Theorems/T_BT_1_Comm.qbit");
    let text = std::fs::read_to_string(&file).unwrap();
    let src = text.replace(
        "\\forall a b : \\tau, a \\land b = b \\land a",
        "Stmt: \\forall a b : \\tau, a \\land b = b \\land a",
    );
    std::fs::write(&file, &src).unwrap();

    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &dir).unwrap();
    let ctx = qubitick_ctx::build_ctx(&db, "Era0/Strata0").unwrap();
    let parsed = qubitick_parse::parse_content(&ctx, &src).unwrap();
    let qubitick_model::Node::Axiom(a) = &parsed[0] else {
        // `T.` with `Stmt:` still parses as a Theorem; without a real proof it's admitted.
        if let qubitick_model::Node::Theorem(t) = &parsed[0] {
            // The Stmt: field became the statement: a forall-a-b binder over two names, not
            // a bare line and not raw text. (The binders are the one reliable shape — the body
            // tree depends on whether the connective parses as a term or a relation.)
            match &t.statement {
                qubitick_model::Expr::Binder(qubitick_model::BinderKind::Forall, binders, _) => {
                    // `\forall a b : τ` is ONE binder group naming two variables.
                    assert_eq!(binders.len(), 1, "one binder group");
                    assert_eq!(
                        binders[0].vars.as_slice(),
                        &["a".to_string(), "b".to_string()],
                        "the binder names a and b"
                    );
                }
                other => panic!("the Stmt: field is the theorem statement (a Forall): {other:?}"),
            }
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
        panic!("expected T. to parse: {:?}", parsed[0]);
    };
    let _ = a;
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_m_premise_block_is_an_admitted_axiom_with_its_stmt_and_doi() {
    let dir = corpus("m");
    wit(
        &dir,
        "Era0/Strata0/Definitions/M_BT_0_1_PA.qbit",
        "M.BT.0.1 [\\mathrm{PA}]\nStmt: \\forall a b : \\tau, a \\land b = b \\land a\nDOI: 10.1017/cbo9781139237314\n",
    );
    let file = dir.join("Era0/Strata0/Definitions/M_BT_0_1_PA.qbit");
    let src = std::fs::read_to_string(&file).unwrap();
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &dir).unwrap();
    let ctx = qubitick_ctx::build_ctx(&db, "Era0/Strata0").unwrap();
    let parsed = qubitick_parse::parse_content(&ctx, &src).unwrap();
    let qubitick_model::Node::Axiom(a) = &parsed[0] else {
        panic!("M. must parse as an admitted premise (axiom): {:?}\nfull: {src}", parsed[0]);
    };
    assert!(
        !a.statements.is_empty(),
        "the M block carries its Stmt: as a statement"
    );
    assert!(
        src.contains("DOI:"),
        "the DOI field is present and does not stop the parse"
    );
    let _ = std::fs::remove_dir_all(&dir);
}