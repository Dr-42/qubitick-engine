//! THE PROOF SURFACE IS NOTATION: a chain, the term that closes it (`⊢`), the
//! end-of-proof mark (`∎`), and nothing else.
//!
//! `⊢` is the derivation sign mathematics writes for "this proves the goal" (the
//! turnstile, U+22A2, read "proves"; `\vdash` in LaTeX). `∎` is the end-of-proof mark
//! (U+220E, the tombstone). A row that is neither a term, nor `= <term> [cite]`, nor
//! `⊢ <term>` is prose, and it is refused by name — a proof holds no English words.

use qubitick_ctx::Ctx;
use qubitick_db::queries::SymbolRow;
use qubitick_model::ProofStep;

fn ctx() -> Ctx {
    let row = |id: i64, tex: &str| SymbolRow {
        facets: 0,
        id,
        scope: "Era0/Strata0".into(),
        tex: tex.into(),
        display: tex.into(),
        lean_name: tex.trim_start_matches('\\').chars().collect::<String>(),
        trigger: None,
        kind: "notation".into(),
        role: None,
        precedence: None,
        home_node: 1,
        elab: None,
        binder_kind: None,
    };
    Ctx::from_rows("Era0/Strata0", vec![row(1, "\\mathfrak{R}")])
}

const HEAD: &str = "T.A.01. Refl\n\\forall a : \\mathfrak{R}, a = a\nProof:\n";

#[test]
fn the_turnstile_closes_the_proof_with_a_term() {
    let src = format!("{HEAD}a\n  = a    [A1.1]\n⊢ h\n∎\n");
    let nodes = qubitick_parse::parse_content(&ctx(), &src).unwrap();
    let qubitick_model::Node::Theorem(t) = &nodes[0] else {
        panic!("expected a theorem");
    };
    assert_eq!(t.proof.len(), 2, "a chain and its closing term: {:?}", t.proof);
    assert!(matches!(t.proof[0], ProofStep::Chain { .. }), "{:?}", t.proof);
    match &t.proof[1] {
        ProofStep::Conclude { term } => {
            assert!(matches!(term, qubitick_model::Expr::Var(v) if v == "h"), "{term:?}");
        }
        other => panic!("`⊢ h` must be the closing TERM, got {other:?}"),
    }
}

#[test]
fn the_latex_spelling_of_the_turnstile_means_the_same_thing() {
    let src = format!("{HEAD}a\n  = a    [A1.1]\n\\vdash h\n");
    let nodes = qubitick_parse::parse_content(&ctx(), &src).unwrap();
    let qubitick_model::Node::Theorem(t) = &nodes[0] else {
        panic!("expected a theorem");
    };
    assert!(
        matches!(t.proof.last(), Some(ProofStep::Conclude { .. })),
        "{:?}",
        t.proof
    );
}

#[test]
fn a_word_where_notation_belongs_is_refused_and_named() {
    // The corpus wrote `conclude <cite>` here. There is no such step: the word is
    // prose, and a proof holds only the corpus's own notation.
    let src = format!("{HEAD}a\nconclude T.BT.08 a a (A1.4 a)\n");
    let err = qubitick_parse::parse_content(&ctx(), &src)
        .expect_err("prose in a proof must not parse");
    let said = format!("{err:#}");
    assert!(
        said.contains("conclude") && said.contains("is not notation"),
        "the refusal names the row and says what a row may be: {said}"
    );
}

#[test]
fn the_end_of_proof_mark_carries_no_step() {
    let src = format!("{HEAD}a\n  = a    [A1.1]\n∎\n");
    let nodes = qubitick_parse::parse_content(&ctx(), &src).unwrap();
    let qubitick_model::Node::Theorem(t) = &nodes[0] else {
        panic!("expected a theorem");
    };
    assert_eq!(t.proof.len(), 1, "the tombstone is a mark, not a step: {:?}", t.proof);
}