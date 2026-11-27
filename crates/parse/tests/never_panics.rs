//! A PREFIX OF A VALID FILE IS INVALID INPUT, AND THE PARSER MUST NOT PANIC ON IT.
//!
//! The editor parses a buffer while someone types: every keystroke is a prefix of the file the
//! author is writing, which means every keystroke is input the parser has never seen a complete
//! version of. A panic there does not produce a diagnostic — it takes the request down and the
//! pane that asked for it shows nothing. So the property is not "parses correctly", it is:
//!
//!   * no `panic` on ANY prefix of ANY of these files,
//!   * and no prefix is silently accepted as something it is not — `parse_content` either
//!     returns nodes or an error, and a truncated row is reported when it is asked to be strict.
//!
//! The same holds for a byte-level truncation that lands in the middle of a multi-byte
//! character, which is what a prefix of a unicode operator looks like.

use qubitick_ctx::Ctx;
use qubitick_db::queries::SymbolRow;

fn ctx() -> Ctx {
    // A row is DECLARED when it carries an elaboration rule, a non-`notation` kind, a role or
    // a facet (`Ctx::row_is_declared`): a corpus's operators answer the parser's gate, and a
    // `notation` row with nothing else is a use-only mention.
    let row = |id: i64, tex: &str, kind: &str, role: Option<&str>| SymbolRow {
        facets: 0,
        id,
        scope: "Era0/Strata0".into(),
        tex: tex.into(),
        display: tex.into(),
        lean_name: tex.trim_start_matches('\\').to_string(),
        trigger: None,
        kind: kind.into(),
        role: role.map(str::to_string),
        precedence: None,
        home_node: 1,
        elab: None,
        binder_kind: None,
    };
    Ctx::from_rows(
        "Era0/Strata0",
        vec![
            row(1, "\\vee", "notation", Some("connective")),
            row(2, "\\land", "notation", Some("connective")),
            row(3, "\\tau", "type", None),
            row(4, "\\tau_0", "ctor", None),
            row(5, "^+", "ctor", None),
            // A declared glyph in the sense the parser's gate asks: `elab` is what a real
            // declaration carries (this crate has no ingest), so it is set here.
            SymbolRow { elab: Some("Apply".into()), ..row(6, "\\oplus", "notation", None) },
            SymbolRow { elab: Some("Apply".into()), ..row(7, "×", "notation", None) },
        ],
    )
}

/// The shapes a corpus file takes, each one exercised at every prefix.
const SOURCES: &[&str] = &[
    // a definition with cases over a declared carrier
    "D.S.05. Plus\na \\oplus b\nSema:\n(\\tau_0 \\oplus b = b) \\land ((^+ a) \\oplus b = ^+ (a \\oplus b))\n",
    // a formation
    "D.S.02. Tick\n\\tau\nSema:\n\\tau = \\tau_0 \\vee (^+ \\tau)\n",
    // a theorem with a chain and a citation
    "T.S.01. Comm\n\\forall a b : \\tau, a \\oplus b = b \\oplus a\nProof:\na \\oplus b\n  = b \\oplus a  [D.S.05]\n⊢ h\n∎\n",
    // a signature, a binding, and prose
    "D.S.07. Bound\na \\oplus b\nType: R.plus : \\tau \\to \\tau \\to \\tau\nMath: prose with a \\frobnicate in it, and a } brace.\n",
    // a unicode operator, whose prefixes end mid-character
    "D.S.08. Times\na × b\nSema:\n(\\tau_0 × b = b) \\land ((^+ a) × b = ^+ (a × b))\n",
    // a file whose every line is malformed
    "\n\n::::\n\\\n{{\n(((\n]]]\nD.S.09. Junk\nType:\nSema:\nProof:\n",
];

#[test]
fn no_prefix_of_a_corpus_file_panics() {
    let ctx = ctx();
    for src in SOURCES {
        for cut in 0..=src.len() {
            // `src.get(..cut)` is `None` exactly when `cut` splits a character: both sides of a
            // multi-byte character are inputs the editor can produce, so the byte slice is what
            // gets parsed.
            let prefix = &src.as_bytes()[..cut];
            let text = String::from_utf8_lossy(prefix).to_string();
            qubitick_parse::tolerate_prose(true);
            let tolerant = qubitick_parse::parse_content(&ctx, &text);
            qubitick_parse::tolerate_prose(false);
            let strict = qubitick_parse::parse_content(&ctx, &text);
            // Nothing to assert about the RESULT: the property is that the calls returned.
            // A tolerant pass is allowed to keep raw text; a strict one either understands the
            // input or refuses it, and both are answers.
            let _ = (tolerant.is_ok(), strict.is_ok());
        }
    }
}

#[test]
fn what_tolerance_keeps_as_text_is_named_by_the_strict_reading() {
    let ctx = ctx();
    // The editor's parse is TOLERANT (a project it cannot open is one nobody can fix): a body
    // it cannot read is kept as the line's own text so the node survives. That is a REPORT owed
    // to the author, and `sema_refusal` is the reading that owes it — the invariant here is
    // that nothing is kept as text without a sentence to show for it.
    let whole = "(\\tau_0 \\oplus b = b) \\land ((^+ a) \\oplus b = ^+ (a \\oplus b))";
    let mut kept = 0usize;
    for cut in 0..=whole.len() {
        let text = String::from_utf8_lossy(&whole.as_bytes()[..cut]).to_string();
        let src = format!("D.S.01. P\na \\oplus b\nSema:\n{text}\n");
        qubitick_parse::tolerate_prose(true);
        let parsed = qubitick_parse::parse_content(&ctx, &src);
        qubitick_parse::tolerate_prose(false);
        if let Ok(nodes) = parsed
            && let Some(qubitick_model::Node::Definition(d)) = nodes.first()
            && matches!(
                d.d_block.form,
                Some(qubitick_model::FormBody::MacroExpr(qubitick_model::Expr::Var(_)))
            )
        {
            kept += 1;
            assert!(
                qubitick_parse::sema_refusal(&ctx, d).is_some(),
                "a body kept as text at cut {cut} is named by the strict reading: {text:?}"
            );
        }
    }
    assert!(kept > 0, "a truncated body is kept as text (tolerance is what opens the file)");
}
