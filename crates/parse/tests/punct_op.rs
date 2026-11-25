use qubitick_db::queries::SymbolRow;
use qubitick_parse::lexer::Token;

fn row(id: i64, tex: &str, lean: &str, role: Option<&str>) -> SymbolRow {
    SymbolRow {
        facets: 0,
        id,
        scope: "Era0/Strata0".into(),
        tex: tex.into(),
        display: tex.into(),
        lean_name: lean.into(),
        trigger: None,
        kind: "notation".into(),
        role: role.map(|s| s.into()),
        precedence: None,
        home_node: 1,
        elab: None,
        binder_kind: None,
    }
}

#[test]
fn lexer_resolves_bare_punctuation_operator() {
    // A bare punctuation operator like `+` registered in the symbol table must be
    // resolved by the lexer (longest-prefix match in the fallback), not treated as
    // an unknown character.
    let ctx = qubitick_ctx::Ctx::from_rows(
        "Era0/Strata0",
        vec![row(42, "+", "plus", Some("infix"))],
    );
    let toks = qubitick_parse::lex("a + b", &ctx).unwrap();
    // Expect: Var(a), Symbol(+, infix), Var(b)
    assert_eq!(toks.len(), 4); // 3 tokens + Eof
    match &toks[0] {
        Token::Var(v, _) => assert_eq!(v, "a"),
        other => panic!("expected Var(a), got {other:?}"),
    }
    match &toks[1] {
        Token::Symbol(id, _) => assert_eq!(*id, 42),
        other => panic!("expected Symbol(plus), got {other:?}"),
    }
    match &toks[2] {
        Token::Var(v, _) => assert_eq!(v, "b"),
        other => panic!("expected Var(b), got {other:?}"),
    }
}

#[test]
fn parser_builds_infix_application_for_punctuation_operator() {
    // With the symbol registered as role=infix, parsing `x + y` must produce an
    // Apply(plus, [x, y]) expression, not a raw Var fallback.
    let ctx = qubitick_ctx::Ctx::from_rows(
        "Era0/Strata0",
        vec![row(42, "+", "plus", Some("infix"))],
    );
    let e = qubitick_parse::parse_expr("x + y", &ctx).unwrap();
    match e {
        qubitick_model::Expr::Apply(h, args) => {
            assert_eq!(h.primary, 42);
            assert_eq!(args.len(), 2);
            match (&args[0], &args[1]) {
                (qubitick_model::Expr::Var(v1), qubitick_model::Expr::Var(v2)) => {
                    assert_eq!(v1, "x");
                    assert_eq!(v2, "y");
                }
                other => panic!("expected [Var(x), Var(y)], got {other:?}"),
            }
        }
        other => panic!("expected Apply(plus, [x, y]), got {other:?}"),
    }
}

#[test]
fn multi_char_punctuation_operator_lexes_as_one_symbol() {
    // A multi-char punctuation run like `**` registers as one symbol and lexes
    // as one token (longest prefix), not two `*` tokens.
    let ctx = qubitick_ctx::Ctx::from_rows(
        "Era0/Strata0",
        vec![row(99, "**", "star_star", Some("infix"))],
    );
    let toks = qubitick_parse::lex("x ** y", &ctx).unwrap();
    assert_eq!(toks.len(), 4); // 3 tokens + Eof
    match &toks[1] {
        Token::Symbol(id, _) => assert_eq!(*id, 99),
        other => panic!("expected Symbol(**), got {other:?}"),
    }
}

#[test]
fn equality_operator_parses_as_application() {
    // The `=` operator (registered globally by ingest) parses as an Apply of the
    // equality symbol when it has role=infix. The equality symbol has a fixed ID.
    let ctx = qubitick_ctx::Ctx::from_rows(
        "Era0/Strata0",
        vec![row(qubitick_model::EQ_SYM as i64, "=", "eq", Some("infix"))],
    );
    let e = qubitick_parse::parse_expr("a = b", &ctx).unwrap();
    match e {
        qubitick_model::Expr::Apply(h, args) => {
            assert_eq!(h.primary, qubitick_model::EQ_SYM);
            assert_eq!(args.len(), 2);
        }
        other => panic!("expected Apply(eq, [a, b]), got {other:?}"),
    }
}