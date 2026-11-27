//! A `Sema:` WITH SEVERAL CASES IS ONE EXPRESSION, AND SAYING SO IS THE PARSER'S JOB.
//!
//! `\iota_0 \oplus b = b` and `(\bullet a) \oplus b = \bullet (a \oplus b)` are two equations
//! about one operator. Written as two LINES, the second used to be dropped without a word:
//! the node kept the first case, `\oplus` looked like a half-defined operator, and the emitter
//! declared it uninterpreted — a hole in the corpus that nothing reported and no build error
//! pointed at, several files away.
//!
//! Written unparenthesised on one line, `=` is left-associative and a connective is a TERM
//! inside a `Sema:` body, so `A = B \land C = D` parses as `((A = B) = C) ∧ D`: the node's
//! symbol then came out as the FORMAT's equality, and deriving a Lean name for it overflowed
//! and panicked the emitter. Both shapes are refused or reported now, and neither panics.

use qubitick_ctx::Ctx;
use qubitick_db::queries::SymbolRow;

fn ctx() -> Ctx {
    let row = |id: i64, tex: &str, role: Option<&str>| SymbolRow {
        facets: 0,
        id,
        scope: "Era0/Strata0".into(),
        tex: tex.into(),
        display: tex.into(),
        lean_name: tex.trim_start_matches('\\').to_string(),
        trigger: None,
        kind: "notation".into(),
        role: role.map(str::to_string),
        precedence: None,
        home_node: 1,
        elab: None,
        binder_kind: None,
    };
    Ctx::from_rows(
        "Era0/Strata0",
        vec![
            row(1, "\\oplus", None),
            row(2, "\\land", Some("connective")),
            row(3, "\\iota_0", None),
            row(4, "\\bullet", None),
        ],
    )
}

const HEAD: &str = "D.S.01. Plus\na \\oplus b\nSema:\n";

#[test]
fn a_second_bare_line_of_a_definition_is_refused_by_name() {
    let src = format!("{HEAD}\\iota_0 \\oplus b = b\n\\bullet a \\oplus b = \\bullet (a \\oplus b)\n");
    let err = qubitick_parse::parse_content(&ctx(), &src)
        .expect_err("a definition carries one body, and the second line is not part of it");
    let said = format!("{err:#}");
    assert!(
        said.contains("carries one glyph line"),
        "the refusal names what the definition carries: {said}"
    );
    assert!(
        said.contains("\\bullet a \\oplus b"),
        "and quotes the line nobody would otherwise see dropped: {said}"
    );
    assert!(
        said.contains("\\land"),
        "and says how several cases are written: {said}"
    );
}

#[test]
fn the_parenthesised_chain_parses_as_two_equations_over_one_operator() {
    let src = format!(
        "{HEAD}(\\iota_0 \\oplus b = b) \\land ((\\bullet a) \\oplus b = \\bullet (a \\oplus b))\n"
    );
    let nodes = qubitick_parse::parse_content(&ctx(), &src).unwrap();
    let qubitick_model::Node::Definition(d) = &nodes[0] else {
        panic!("expected a definition");
    };
    assert_eq!(
        d.symbol.primary, 1,
        "the node's symbol is the operator the cases define, not the format's `=`"
    );
    assert!(
        matches!(
            d.d_block.form,
            Some(qubitick_model::FormBody::MacroExpr(_))
        ),
        "two cases are one body: {:?}",
        d.d_block.form
    );
}

/* THE UNPARENTHESISED SHAPE IS A CORPUS MISTAKE, NOT A CRASH.
 *
 * It cannot be read as two equations — `=` binds left and the connective is a term — so what
 * the parser must guarantee is that the NODE does not claim to define equality, and that the
 * emitter is never handed `u32::MAX - 1` as if it were a glyph id.
 */
#[test]
fn an_unparenthesised_chain_never_makes_the_node_define_equality() {
    let src = format!("{HEAD}\\iota_0 \\oplus b = b \\land \\bullet a \\oplus b = \\bullet (a \\oplus b)\n");
    let nodes = qubitick_parse::parse_content(&ctx(), &src).unwrap();
    let qubitick_model::Node::Definition(d) = &nodes[0] else {
        panic!("expected a definition");
    };
    assert_ne!(
        d.symbol.primary,
        qubitick_model::EQ_SYM,
        "the format's `=` is never an operator a node declares"
    );
    assert_ne!(
        d.symbol.primary,
        u32::MAX - 1,
        "and the reserved id never reaches the emitter's name derivation"
    );
}