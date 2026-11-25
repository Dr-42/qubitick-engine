//! Semantic elaboration and diagnostics (LexParseAst §6, §9).

use qubitick_model::{AtomicComposite, Expr};

use crate::lexer::{LexError, Span};

/// Severity of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// Which pipeline phase produced a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Lex,
    Atomic,
    Structural,
    Scope,
    Role,
    Elaboration,
}

/// A structured diagnostic (LexParseAst §9.2).
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub phase: Phase,
    pub span: Span,
    pub message: String,
    pub hints: Vec<String>,
    /// What kind of refusal this is, so a caller can act on ONE kind without
    /// matching prose. `operator.undeclared` is the one a build escalates to an
    /// error: a glyph in operator position that the corpus never declares.
    pub code: &'static str,
}

/// The parse-crate error type, wrapping either a lex error or an elaboration error.
#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    Lex(LexError),
    Elab(Diagnostic),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Lex(e) => write!(
                f,
                "lex error at {}..{}: {}",
                e.span.start, e.span.end, e.message
            ),
            ParseError::Elab(d) => write!(
                f,
                "{:?} {:?} at {}..{}: {}",
                d.severity, d.phase, d.span.start, d.span.end, d.message
            ),
        }
    }
}

impl std::error::Error for ParseError {}

/// Phase 2: elaborate a structural expression into a `model::Expr`.
///
/// A full ModRole/ElabRule/ApplyForm resolver is substantial; this provides a
/// faithful core: `Sym` and `Var` pass through, applications become `Apply`,
/// brackets become `Bracket`, and binders are lowered. It is the seam the full
/// elaborator extends.
pub fn elab(e: &crate::parser::StructExpr) -> Result<Expr, ParseError> {
    use crate::parser::StructExpr as S;
    Ok(match e {
        S::Sym(ac) => Expr::Sym(ac.clone()),
        S::Var(v) => Expr::Var(v.clone()),
        S::Application(head, args) => {
            let args = args.iter().map(elab).collect::<Result<Vec<_>, _>>()?;
            Expr::Apply(head.clone(), args)
        }
        S::Bracket(id, args) => {
            let args = args.iter().map(elab).collect::<Result<Vec<_>, _>>()?;
            Expr::Bracket(*id, args)
        }
        S::Matrix(rows) => {
            let rows = rows
                .iter()
                .map(|r| r.iter().map(elab).collect::<Result<Vec<_>, _>>())
                .collect::<Result<Vec<_>, _>>()?;
            Expr::Matrix(rows)
        }
        S::Vector(els) => {
            let els = els.iter().map(elab).collect::<Result<Vec<_>, _>>()?;
            Expr::Vector(els)
        }
        S::Set(els) => {
            let els = els.iter().map(elab).collect::<Result<Vec<_>, _>>()?;
            Expr::Set(els)
        }
        S::BinderBlock(kind, binders, body) => {
            let binders = binders
                .iter()
                .map(|b| {
                    Ok(qubitick_model::Binder {
                        vars: b.vars.clone(),
                        ty: elab(&b.ty)?,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Expr::Binder(*kind, binders, Box::new(elab(body)?))
        }
        S::Bin(id, l, r) => {
            // Flat binary collapses to Apply in the model per LexParseAst §6.4.
            let l = elab(l)?;
            let r = elab(r)?;
            Expr::Apply(AtomicComposite::new(*id), vec![l, r])
        }
        S::RecursorBlock(scrut, cases) => {
            let scrut = elab(scrut)?;
            let cases = cases
                .iter()
                .map(|(pat, body)| Ok((pat.clone(), elab(body)?)))
                .collect::<Result<Vec<_>, _>>()?;
            Expr::Recursor(Box::new(scrut), cases)
        }
    })
}

/// Parse a source fragment to a single `model::Expr`, running lex → structural → elab.
pub fn parse_expr(src: &str, ctx: &qubitick_ctx::Ctx) -> Result<Expr, ParseError> {
    let toks = crate::lexer::lex(src, ctx).map_err(ParseError::Lex)?;
    let (se, _) = crate::parser::parse_struct(&toks, ctx).map_err(ParseError::Elab)?;
    elab(&se)
}

/// Helper to make an `AtomicComposite` from an id (for the Bin fallback and tests).
pub fn atomic_of(id: u32) -> AtomicComposite {
    AtomicComposite::new(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_db::queries::SymbolRow;

    /// A row that DECLARES its glyph with `role` (empty = the fixture does not
    /// classify it), which is what a corpus node's glyph line does.
    fn declared(id: i64, tex: &str, role: &str) -> SymbolRow {
        SymbolRow { facets: 0,
            id,
            scope: "Era0/Strata0".into(),
            tex: tex.into(),
            display: tex.into(),
            lean_name: tex.trim_start_matches('\\').to_string(),
            trigger: None,
            kind: "notation".into(),
            role: (!role.is_empty()).then(|| role.to_string()),
            precedence: None,
            home_node: 1,
            elab: None,
            binder_kind: None,
        }
    }

    fn test_ctx() -> qubitick_ctx::Ctx {
        qubitick_ctx::Ctx::from_rows(
            "Era0/Strata0",
            vec![
                declared(1, "\\Delta", "infix"),
                declared(2, "\\cap", "infix"),
                declared(3, "\\to", "infix"),
            ],
        )
    }

    #[test]
    fn z_ctor_patterns_parse() {
        // Deterministic check of the induction-pattern strings: do the bare
        // ℤ-ctor tex forms parse as operands?
        let row = |id: i64, tex: &str| SymbolRow { facets: 0,
            id,
            scope: "".into(),
            tex: tex.into(),
            display: tex.into(),
            lean_name: tex
                .trim_start_matches('\\')
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect::<String>()
                .trim_matches('_')
                .to_string(),
            trigger: None,
            kind: "notation".into(),
            role: None,
            precedence: None,
            home_node: 1,
            elab: None,
            binder_kind: None,
        };
        let ctx = qubitick_ctx::Ctx::from_rows(
            "",
            vec![
                row(100, "\\mathbb{Z}"),
                row(101, "\\mathbb{Z}_{0}"),
                row(102, "\\mathbb{Z}_0"),
                row(103, "\\mathbb{Z}^{+}"),
                row(104, "\\mathbb{Z}^{-}"),
                row(105, "^+"),
                row(106, "\\tau_0"),
            ],
        );
        for (tex, expect) in [
            ("\\mathbb{Z}_0", true),
            ("\\mathbb{Z}_{0}", true),
            ("\\mathbb{Z}^{+} n", true),
            ("\\mathbb{Z}^{-} n", true),
            ("\\tau_0", true),
            ("^+ a", true),
        ] {
            let r = parse_expr(tex, &ctx);
            let ok = r.is_ok();
            eprintln!("PARSETEST {tex:?} -> {ok} {r:?}");
            assert_eq!(ok, expect, "tex {tex}");
        }
    }

    #[test]
    fn parses_arrow_as_right_assoc_infix() {
        let ctx = test_ctx();
        // `\Delta -> (\cap -> \Delta)` — the arrow is a right-assoc infix.
        let e = parse_expr("\\Delta \\to \\cap \\to \\Delta", &ctx).unwrap();
        if let Expr::Apply(head, args) = &e {
            assert_eq!(head.primary, 3, "arrow symbol id");
            assert_eq!(args.len(), 2);
            assert!(
                matches!(&args[1], Expr::Apply(inner, _) if inner.primary == 3),
                "right-assoc nesting"
            );
        } else {
            panic!("expected an arrow application");
        }
    }

    #[test]
    fn elabs_application() {
        let ctx = test_ctx();
        let e = parse_expr("\\Delta \\cap \\Delta", &ctx).unwrap();
        assert!(matches!(e, Expr::Apply(_, _)));
    }

    #[test]
    fn elabs_infix_symbol_uses_symbol_as_head() {
        // `a \cap b` must elaborate to `Apply(cap, [a, b])` (cap id 2), not a
        // headless application with a dummy id.
        let ctx = test_ctx();
        let e = parse_expr("a \\cap b", &ctx).unwrap();
        if let Expr::Apply(head, args) = &e {
            assert_eq!(head.primary, 2, "infix cap should be the application head");
            assert_eq!(args.len(), 2);
            assert!(matches!(&args[0], Expr::Var(v) if v == "a"));
            assert!(matches!(&args[1], Expr::Var(v) if v == "b"));
        } else {
            panic!("expected an application");
        }
    }

    #[test]
    fn elabs_equality_as_eq_infix() {
        // `a = b` binds into an `Apply` rooted at the reserved EQ_SYM id.
        let ctx = test_ctx();
        let e = parse_expr("a = b", &ctx).unwrap();
        if let Expr::Apply(head, args) = &e {
            assert_eq!(head.primary, qubitick_model::EQ_SYM);
            assert_eq!(args.len(), 2);
        } else {
            panic!("expected an equality application");
        }
    }

    #[test]
    fn the_charter_level_orders_the_infix_fold() {
        // The charter's master table (D10) says `+` binds looser than `\times`, so
        // `a + b \times c` is `Plus a (Times b c)` and NOT `Times (Plus a b) c`.
        let mut plus = declared(1, "+", "infix");
        plus.precedence = Some(1);
        let mut times = declared(2, "\\times", "infix");
        times.precedence = Some(2);
        let ctx = qubitick_ctx::Ctx::from_rows("Era0/Strata0", vec![plus, times]);

        let e = parse_expr("a + b \\times c", &ctx).unwrap();
        let Expr::Apply(head, args) = &e else {
            panic!("expected an application, got {e:?}");
        };
        assert_eq!(head.primary, 1, "the LOOSER operator is the root: {e:?}");
        match &args[1] {
            Expr::Apply(inner, inner_args) => {
                assert_eq!(inner.primary, 2, "the tighter operator binds into the right operand: {e:?}");
                assert_eq!(inner_args.len(), 2);
            }
            other => panic!("the right operand should be the \\times application, got {other:?}"),
        }

        // Equal levels nest LEFT: `a + b + c` is `Plus (Plus a b) c`, because
        // associativity is a theorem and not metadata.
        let nested = parse_expr("a + b + c", &ctx).unwrap();
        let Expr::Apply(head, args) = &nested else {
            panic!("expected an application, got {nested:?}");
        };
        assert_eq!(head.primary, 1);
        assert!(
            matches!(&args[0], Expr::Apply(inner, _) if inner.primary == 1),
            "equal levels nest left: {nested:?}"
        );
    }

    #[test]
    fn elabs_equation_with_infix_terms() {
        // `a \Delta b = b \Delta a` -> Apply(EQ, [Apply(Delta,[a,b]), Apply(Delta,[b,a])]).
        let ctx = test_ctx();
        let e = parse_expr("a \\Delta b = b \\Delta a", &ctx).unwrap();
        if let Expr::Apply(head, args) = &e {
            assert_eq!(head.primary, qubitick_model::EQ_SYM);
            assert_eq!(args.len(), 2);
            for side in args {
                if let Expr::Apply(h, a) = side {
                    assert_eq!(h.primary, 1, "Delta is the head of each side");
                    assert_eq!(a.len(), 2);
                } else {
                    panic!("each side should be a Delta application");
                }
            }
        } else {
            panic!("expected an equality application");
        }
    }

    #[test]
    fn elabs_forall_binder_over_equation() {
        // `\forall a b : \mathfrak{E}, a \Delta b = b \Delta a` must parse to a
        // Forall binder whose body is an equality, not a raw Var fallback.
        let ctx = qubitick_ctx::Ctx::from_rows(
            "Era0/Strata0",
            vec![
                declared(1, "\\mathfrak{E}", ""),
                declared(2, "\\Delta", "infix"),
                declared(3, "\\cap", "infix"),
            ],
        );
        let e = parse_expr(
            "\\forall a b : \\mathfrak{E}, a \\Delta b = b \\Delta a",
            &ctx,
        )
        .unwrap();
        match e {
            Expr::Binder(kind, binders, body) => {
                assert_eq!(kind, qubitick_model::BinderKind::Forall);
                assert!(
                    binders
                        .iter()
                        .any(|b| b.vars == vec!["a".to_string(), "b".to_string()]),
                    "binder should group vars a b, got {binders:?}"
                );
                assert!(matches!(*body, Expr::Apply(h, _) if h.primary == qubitick_model::EQ_SYM));
            }
            other => panic!("expected a Forall binder, got {other:?}"),
        }
    }
}
