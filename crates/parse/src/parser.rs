//! Phase 1 — structural parser (LexParseAst §5).
//!
//! Converts the token stream into a structural `StructExpr` via Pratt precedence
//! and form dispatch. Mods/roles/elab are left unresolved to Phase 2.

use qubitick_ctx::Ctx;
use qubitick_model::{AtomicComposite, BinderKind, EQ_SYM};
use qubitick_model::corpus::Facet;

// While parsing a Definition's own `Sema:`, the connective being DEFINED
// (`\leftrightarrow`, `\land`, `\lor`, …) must bind TIGHT — not via the global
// `is_logic_connective` loose loop — or `a \leftrightarrow b = rhs` would parse
// with the connective OUTERMOST (`Apply(leftrightarrow,[a, Eq b rhs])`) instead
// of as a defining equation `Eq ((leftrightarrow a b)) rhs`, so `macro_def_text`
// could never see a definition and would emit an axiom (nothing to unfold).
thread_local! {
    static EXCLUDED_CONNECTIVE: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}
/// Register a connective id to treat as TIGHT for the next expression parse
/// (0 resets). Used by `parse_definition` for the definiendum's own Sema.
pub fn exclude_connective(id: u32) {
    EXCLUDED_CONNECTIVE.with(|c| c.set(id));
}

thread_local! {
    /// True while a definition's `Sema:` is being parsed.
    ///
    /// A `Sema:` is `<glyph> = <body>`: the format fixes that shape, so no CORPUS
    /// connective inside the body may outrank the `=` that links it — a formation
    /// whose alternatives are separated by the corpus's `Role: connective` glyph
    /// would otherwise absorb the `=` into the separator's tree and read as
    /// `separator(glyph = first, second)`. The format's own iff stays loose, because
    /// that is what links a relational definition to its body.
    static SEMA_BODY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Mark the start/end of a `Sema:` parse (see [`SEMA_BODY`]).
pub fn in_sema_body(inside: bool) {
    SEMA_BODY.with(|c| c.set(inside));
}

use crate::atomic::parse_atomic;
use crate::lexer::{LexError, Span, Token};
use crate::semantic::{Diagnostic, Phase, Severity};

/// Structural expression — the Phase 1 intermediate (LexParseAst §5.2).
#[derive(Debug, Clone)]
pub enum StructExpr {
    Sym(AtomicComposite),
    Var(String),
    Application(AtomicComposite, Vec<StructExpr>),
    Bracket(u32, Vec<StructExpr>),
    Matrix(Vec<Vec<StructExpr>>),
    Vector(Vec<StructExpr>),
    Set(Vec<StructExpr>),
    BinderBlock(BinderKind, Vec<StructBinder>, Box<StructExpr>),
    Bin(u32, Box<StructExpr>, Box<StructExpr>),
    RecursorBlock(Box<StructExpr>, Vec<(qubitick_model::Pattern, StructExpr)>),
}

/// Structural binder (LexParseAst §5.2).
#[derive(Debug, Clone)]
pub struct StructBinder {
    pub vars: Vec<String>,
    pub ty: StructExpr,
}

/// Parse a full expression from a token stream.
pub fn parse_struct(tokens: &[Token], ctx: &Ctx) -> Result<(StructExpr, usize), Diagnostic> {
    let (e, pos) = parse_flat(tokens, 0, 0, ctx)?;
    Ok((e, pos))
}

/// Whether a glyph acts as a prefix (unary) operator.
///
/// The answer is the corpus's: a declaration whose glyph line puts its operand AFTER
/// the glyph (`op a`) derives the role `prefix`, and a `Role: prefix` states it
/// outright. The engine names no unary glyph of its own.
///
/// This is consulted only where an OPERAND is expected, so a glyph that also has an
/// infix use (`a +_{\tau} b`) is unaffected: after an operand it is consumed by the
/// infix arm instead.
fn is_prefix_glyph(ctx: &Ctx, id: u32) -> bool {
    // The persisted surface role decides: `prefix` comes from the glyph line
    // (`op a`) or from a `Role:` the corpus stated. A glyph used unarily that the
    // corpus never declared is not an operator at all.
    ctx.by_id(id as i64).and_then(|r| r.role.as_deref()) == Some("prefix")
}

/// Whether a token can begin an operand (so a prefix glyph applies to it).
fn starts_operand(t: Option<&Token>) -> bool {
    matches!(
        t,
        Some(Token::Symbol(_, _))
            | Some(Token::Var(_, _))
            | Some(Token::LParen(_))
            | Some(Token::Forall(_))
            | Some(Token::Exists(_))
    )
}
/// The binder a big-operator glyph carries in its own subscript.
///
/// A modified operator is registered as ONE symbol, so `\bigcup_{t : \tau}` is
/// a single glyph (`SurfaceFormat`: operators with mods are single symbols).
/// When its subscript has the binder shape `<vars> : <sort>` the glyph binds
/// those variables over the operand that follows it. Returns `(vars, sort_tex)`.
///
/// D-DATA-DRIVEN: this is purely structural — ANY registered `\command` whose tex
/// is led by a `\command` and carries a `_{... : ...}` binder-shaped subscript
/// qualifies. There is no hardcoded list of binder operator names, so an author
/// may define and register `\sum`, `\int`, `\oint`, `\bigvee`, or any custom
/// binder and the engine binds its variables with no source change.
fn bigop_binder_spec(ctx: &Ctx, id: u32) -> Option<(Vec<String>, Option<String>, BinderKind)> {
    let tex = ctx.by_id(id as i64)?.tex.clone();
    // Must be a `\command` tex (not a bare glyph or an ASCII operator), carrying a
    // binder-shaped subscript.
    let cmd = tex.strip_prefix('\\')?;
    let head: String = cmd.chars().take_while(|c| c.is_ascii_alphabetic() || *c == '@').collect();
    if head.is_empty() {
        return None;
    }
    let inner = tex.split_once("_{")?.1.split_once('}')?.0;
    // Collect the leading identifiers of the subscript head.
    let ids_of = |s: &str| -> Vec<String> {
        s.split(',')
            .map(|v| v.trim().to_string())
            .filter(|v| {
                !v.is_empty()
                    && v.chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '\'')
            })
            .collect()
    };
    // Form 1: `_{ vars : sort }` — a typed binder (Lambda/fold).
    if let Some((vars_part, sort_part)) = inner.split_once(':') {
        let vars = ids_of(vars_part);
        if vars.is_empty() {
            return None;
        }
        return Some((vars, Some(sort_part.trim().to_string()), BinderKind::Lambda));
    }
    // Form 2: `_{ var \to target }` — a LIMIT-style binder (`\lim_{x \to 0}`).
    if let Some((vars_part, _target)) = inner.split_once("\\to") {
        let vars = ids_of(vars_part);
        if vars.is_empty() {
            return None;
        }
        return Some((vars, None, BinderKind::Limit));
    }
    None
}

/// Parse a binder's sort from its LaTeX text (`\tau`).
fn parse_sort_text(ctx: &Ctx, src: &str) -> Result<StructExpr, Diagnostic> {
    let tokens = crate::lexer::lex(src, ctx).map_err(lex_diag)?;
    let (e, _) = parse_flat(&tokens, 0, 0, ctx)?;
    Ok(e)
}

fn parse_atom(tokens: &[Token], pos: usize, ctx: &Ctx) -> Result<(StructExpr, usize), Diagnostic> {
    match tokens.get(pos) {
        Some(Token::Symbol(id, _)) => {
            let (ac, next) = parse_atomic(tokens, pos).map_err(lex_diag)?;
            // Application?  head( args ). When the symbol's persisted D-block elab
            // is `CurriedApply``, a following `(…)(…)` nests (per ApplyForm), else a
            // single `(…)` is one flat call. Consults the DB metadata (single source
            // of truth); symbols without a persisted elab fall back to flat Apply.
            if matches!(tokens.get(next), Some(Token::LParen(_))) {
                if is_curried(ctx, *id) {
                    return parse_curried_apply(&ac, tokens, next, ctx);
                }
                let (args, next) = parse_arg_list(tokens, next + 1, ctx)?;
                return Ok((StructExpr::Application(ac, args), next));
            }
            // A braced-ARGUMENT macro head: the lexer classified the command by a
            // structural count of consecutive `{…}` groups (≥2 ⇒ args), so it left
            // the braces in the token stream (`LBrace n RBrace LBrace d RBrace`)
            // and emitted the bare head as a `Symbol`. Fold the groups into an
            // application: `\frac{n}{d}` = `Apply(frac, [n, d])`.
            if matches!(tokens.get(next), Some(Token::LBrace(_))) {
                let (args, next) = parse_braced_args(tokens, next, ctx)?;
                return Ok((StructExpr::Application(ac, args), next));
            }
            // A prefix operator applies to the following operand without parens:
            // `\lnot a`, `^+t`.
            if is_prefix_glyph(ctx, *id) && starts_operand(tokens.get(next)) {
                let (arg, next) = parse_atom(tokens, next, ctx)?;
                return Ok((StructExpr::Application(ac, vec![arg]), next));
            }
            // A big operator with a BINDER subscript (`\bigcup_{t : \tau} A(t)`)
            // binds its subscript variables over the following operand. The
            // modified operator is registered as ONE symbol, so the binder's
            // variables and sort are recovered from the glyph's own tex. The
            // decision whether it binds comes from the DB D-block elab (`Binder`)
            // when recorded; otherwise the structural probe applies.
            let is_db_binder = ctx.elab_rule_of(*id as i64) == Some("Binder");
            if starts_operand(tokens.get(next))
                && (is_db_binder || ctx.elab_rule_of(*id as i64).is_none())
                && let Some((vars, sort_tex, kind)) = bigop_binder_spec(ctx, *id)
            {
                let ty = match sort_tex {
                    Some(s) => parse_sort_text(ctx, &s)?,
                    // A limit binder has no explicit sort; the variable's type is
                    // inferred (rendered `_`), which is the documented placeholder.
                    None => StructExpr::Var("_".to_string()),
                };
                let (body, next) = parse_atom(tokens, next, ctx)?;
                // The binder KIND comes from the DB when recorded (single source
                // of truth), else the structural default from the subscript form.
                let kind = match ctx.binder_kind_of(*id as i64) {
                    Some("Integral") => BinderKind::Integral,
                    Some("Limit") => BinderKind::Limit,
                    Some("Forall") => BinderKind::Forall,
                    Some("Exists") => BinderKind::Exists,
                    _ => kind,
                };
                let binder = StructExpr::BinderBlock(
                    kind,
                    vec![StructBinder { vars, ty }],
                    Box::new(body),
                );
                return Ok((StructExpr::Application(ac, vec![binder]), next));
            }
            Ok((StructExpr::Sym(ac), next))
        }
        Some(Token::LParen(_)) => {
            // A comma-separated `( a, b, c )` group is a VECTOR literal
            // (LexParseAst §7.1); a single `( expr )` is a plain grouping. The
            // comma test is paren-aware and excludes a leading BINDER (a
            // `\forall b : τ, …` group uses its comma to separate binder from
            // body — NOT a vector element list), so `f(x, y)` and
            // `(\forall b : τ, a + (^+ b) = …)` never become vectors.
            if !starts_with_binder(tokens.get(pos + 1))
                && has_top_level_comma_until(tokens, pos + 1)
            {
                let (els, next) = parse_arg_list(tokens, pos + 1, ctx)?;
                return Ok((StructExpr::Vector(els), next));
            }
            let (inner, next) = parse_flat(tokens, pos + 1, 0, ctx)?;
            if !matches!(tokens.get(next), Some(Token::RParen(_))) {
                return Err(synth_diag(next, "expected ')'"));
            }
            Ok((inner, next + 1))
        }
        Some(Token::Var(v, _)) => Ok((StructExpr::Var(v.clone()), pos + 1)),
        Some(Token::Forall(_)) => parse_binder(tokens, pos, BinderKind::Forall, ctx),
        Some(Token::Exists(_)) => parse_binder(tokens, pos, BinderKind::Exists, ctx),
        // A matrix environment `\begin{matrix} … \end{matrix}` (LexParseAst
        // §5.4/§7.1): rows split by `\\`, cells by `&` → `StructExpr::Matrix`.
        Some(Token::Begin(_)) => {
            let (rows, next) = parse_matrix(tokens, pos + 1, ctx)?;
            Ok((StructExpr::Matrix(rows), next))
        }
        // A braced single symbol `{^+}`, `{\mathbb{Z}^{+}}` acts as a normal head,
        // possibly applied to a following operand: `{^+}(a +_\tau b)`. A
        // COMPMA-SEPARATED `{ … }` group is a SET literal (`\{a, b, c\}` → `Set`,
        // LexParseAst §7.1). Distinguish by shape, not by any glyph name.
        Some(Token::LBrace(_)) => {
            if set_open_has_comma(tokens, pos) {
                let (elems, next) = parse_set_literal(tokens, pos, ctx)?;
                return Ok((StructExpr::Set(elems), next));
            }
            let (ac, after) = parse_braced_atom(tokens, pos).map_err(lex_diag)?;
            match after {
                // head( args )
                a if matches!(tokens.get(a), Some(Token::LParen(_))) => {
                    let (args, next) = parse_arg_list(tokens, a + 1, ctx)?;
                    Ok((StructExpr::Application(ac, args), next))
                }
                // prefix application: `{^+} x`
                a if is_prefix_glyph(ctx, ac.primary) && starts_operand(tokens.get(a)) => {
                    let (arg, next) = parse_atom(tokens, a, ctx)?;
                    Ok((StructExpr::Application(ac, vec![arg]), next))
                }
                a => Ok((StructExpr::Sym(ac), a)),
            }
        }
        _ => Err(synth_diag(pos, "expected an expression")),
    }
}

/// A `{ Sym }` head, returning its atomic and the position after `}`.
fn parse_braced_atom(
    tokens: &[Token],
    pos: usize,
) -> Result<(AtomicComposite, usize), LexError> {
    let p1 = pos + 1;
    let (ac, p2) = parse_atomic(tokens, p1)?;
    if !matches!(tokens.get(p2), Some(Token::RBrace(_))) {
        return Err(LexError {
            span: Span::new(p2, p2),
            message: "expected '}' after braced symbol".into(),
        });
    }
    Ok((ac, p2 + 1))
}

/// Parse a quantifier binder group `\forall a b : ty, body` (LexParseAst §5.2).
///
/// Binds one or more adjacent metavariables, optionally behind a `: ty` sort,
/// then a body following the `,`. The binder's bound variables shadow any symbol
/// of the same name inside the body.
fn parse_binder(
    tokens: &[Token],
    pos: usize,
    kind: BinderKind,
    ctx: &Ctx,
) -> Result<(StructExpr, usize), Diagnostic> {
    let mut pos = pos + 1; // consume the quantifier token
    let mut vars: Vec<String> = Vec::new();
    while let Some(Token::Var(v, _)) = tokens.get(pos) {
        vars.push(v.clone());
        pos += 1;
    }
    if vars.is_empty() {
        return Err(synth_diag(
            pos,
            "expected a bound variable after quantifier",
        ));
    }
    let ty = if matches!(tokens.get(pos), Some(Token::Colon(_))) {
        pos += 1;
        let (t, next) = parse_flat(tokens, pos, 0, ctx)?;
        pos = next;
        t
    } else {
        // No explicit sort: the binder quantifies over an inferred type.
        StructExpr::Var("_".to_string())
    };
    if matches!(tokens.get(pos), Some(Token::Comma(_))) {
        pos += 1;
    }
    let (body, next) = parse_flat(tokens, pos, 0, ctx)?;
    Ok((
        StructExpr::BinderBlock(kind, vec![StructBinder { vars, ty }], Box::new(body)),
        next,
    ))
}

fn lex_diag(e: LexError) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        phase: Phase::Lex,
        span: e.span,
        message: e.message,
        hints: Vec::new(),
        code: "lex",
    }
}

fn synth_diag(pos: usize, msg: &str) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        phase: Phase::Structural,
        span: Span::new(pos, pos),
        message: msg.into(),
        hints: Vec::new(),
        code: "structural",
    }
}

/// Whether a symbol's persisted D-block elab is `CurriedApply` — its calls nest
/// per ApplyForm (DataSchema §6.5). From the DB (single source of truth).
fn is_curried(ctx: &Ctx, id: u32) -> bool {
    ctx.elab_rule_of(id as i64) == Some("CurriedApply")
}

/// Parse a Curried application `head( a0 )( a1 )( a2 )` into nested
/// `Apply(Apply(Apply(head,[a0]),[a1]),[a2])` per `ApplyForm::Curried`.
fn parse_curried_apply(
    ac: &AtomicComposite,
    tokens: &[Token],
    pos: usize,
    ctx: &Ctx,
) -> Result<(StructExpr, usize), Diagnostic> {
    let (groups, next) = collect_paren_groups(tokens, pos, ctx)?;
    Ok((fold_curried(ac, groups), next))
}

/// Collect consecutive `( … )` argument groups starting at pos.
fn collect_paren_groups(
    tokens: &[Token],
    pos: usize,
    ctx: &Ctx,
) -> Result<(Vec<Vec<StructExpr>>, usize), Diagnostic> {
    let mut groups = Vec::new();
    let mut pos = pos;
    while let Some(Token::LParen(_)) = tokens.get(pos) {
        let (args, next) = parse_arg_list(tokens, pos + 1, ctx)?;
        groups.push(args);
        pos = next;
    }
    Ok((groups, pos))
}

/// Nest `head(a0)(a1)(a2)` = `Apply(Apply(Apply(head,[a0]),[a1]),[a2])`.
fn fold_curried(ac: &AtomicComposite, groups: Vec<Vec<StructExpr>>) -> StructExpr {
    // Curried f a0 a1 = Apply(Apply(f,a0),a1): f applied to a0, that to a1, …
    if groups.is_empty() {
        return StructExpr::Application(ac.clone(), vec![]);
    }
    let mut cur = StructExpr::Application(ac.clone(), Vec::new());
    for g in groups {
        cur = StructExpr::Application(
            ac.clone(),
            vec![cur, StructExpr::Application(ac.clone(), g)],
        );
    }
    cur
}

/// Parse a comma-separated argument list `( a, b, ... )` starting at first arg.
fn parse_arg_list(
    tokens: &[Token],
    pos: usize,
    ctx: &Ctx,
) -> Result<(Vec<StructExpr>, usize), Diagnostic> {
    let mut args = Vec::new();
    let mut pos = pos;
    loop {
        match tokens.get(pos) {
            Some(Token::RParen(_)) => return Ok((args, pos + 1)),
            None => return Err(synth_diag(pos, "unclosed argument list")),
            _ => {
                let (arg, next) = parse_flat(tokens, pos, 0, ctx)?;
                args.push(arg);
                pos = next;
                match tokens.get(pos) {
                    Some(Token::Comma(_)) => pos += 1,
                    Some(Token::RParen(_)) => return Ok((args, pos + 1)),
                    _ => return Err(synth_diag(pos, "expected ',' or ')' in argument list")),
                }
            }
        }
    }
}

/// Whether the braced group starting at `pos` (`{ … }`) contains a top-level
/// comma — the SET-literal signal (`\{a, b, c\}`). A single-element `{x}` stays a
/// braced-atom head. Scans to the matching `}`.
fn set_open_has_comma(tokens: &[Token], pos: usize) -> bool {
    let mut i = pos + 1;
    while let Some(tok) = tokens.get(i) {
        match tok {
            Token::RBrace(_) => return false,
            Token::Comma(_) => return true,
            _ => i += 1,
        }
    }
    false
}

/// Whether a token opens a BINDER group (`\forall`/`\exists`), whose comma is
/// binder/body punctuation rather than a vector element separator.
fn starts_with_binder(t: Option<&Token>) -> bool {
    matches!(t, Some(Token::Forall(_)) | Some(Token::Exists(_)))
}

/// Whether a bracketed region opened at `parse_pos` contains a TOP-LEVEL comma
/// before its matching closing `)` — the VECTOR-literal signal (`(a, b, c)`) as
/// opposed to a single grouping `(expr)` or an application `f(x, y)`. Paren-aware:
/// a comma inside a nested `(...)` or `{…}` is NOT a top-level comma, so
/// `(\mathbb{Q}^{+}(n, m) \cup \ldots)` stays a grouping, not a vector.
fn has_top_level_comma_until(tokens: &[Token], pos: usize) -> bool {
    let mut depth = 0usize;
    let mut i = pos;
    while let Some(tok) = tokens.get(i) {
        match tok {
            Token::LBrace(_) => depth += 1,
            Token::RBrace(_) => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            Token::LParen(_) => depth += 1,
            Token::RParen(_) => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            Token::Comma(_) if depth == 0 => return true,
            _ => {}
        }
        i += 1;
    }
    false
}

/// Parse a comma-separated set literal `{ a, b, c }` (LexParseAst §7.1), returning
/// its elements and the position after the closing `}`. The opening `{` is at `pos`.
fn parse_set_literal(
    tokens: &[Token],
    pos: usize,
    ctx: &Ctx,
) -> Result<(Vec<StructExpr>, usize), Diagnostic> {
    let mut elems = Vec::new();
    let mut i = pos + 1;
    loop {
        match tokens.get(i) {
            Some(Token::RBrace(_)) => return Ok((elems, i + 1)),
            None => return Err(synth_diag(i, "unclosed set literal")),
            _ => {
                let (e, next) = parse_flat(tokens, i, 0, ctx)?;
                elems.push(e);
                i = next;
                match tokens.get(i) {
                    Some(Token::Comma(_)) => i += 1,
                    Some(Token::RBrace(_)) => return Ok((elems, i + 1)),
                    _ => return Err(synth_diag(i, "expected ',' or '}' in set literal")),
                }
            }
        }
    }
}

/// Parse a matrix environment between `\begin{…}` and `\end{…}`, returning the
/// rows (each a row of cell expressions). Rows split by ``\\``, cells by `&`.
fn parse_matrix(
    tokens: &[Token],
    mut pos: usize,
    ctx: &Ctx,
) -> Result<(Vec<Vec<StructExpr>>, usize), Diagnostic> {
    let mut rows: Vec<Vec<StructExpr>> = Vec::new();
    let mut row: Vec<StructExpr> = Vec::new();
    loop {
        match tokens.get(pos) {
            Some(Token::End(_)) => {
                if !row.is_empty() || !rows.is_empty() {
                    rows.push(row.clone());
                    row.clear();
                }
                return Ok((rows, pos + 1));
            }
            Some(Token::Eof(_)) => return Err(synth_diag(pos, "unclosed matrix environment")),
            Some(Token::Ampersand(_)) => {
                pos += 1;
            }
            Some(Token::RowSep(_)) => {
                rows.push(std::mem::take(&mut row));
                pos += 1;
            }
            _ => {
                let (cell, next) = parse_flat(tokens, pos, 0, ctx)?;
                row.push(cell);
                pos = next;
            }
        }
    }
}

/// Parse one or more consecutive `{ … }` braced groups as application arguments,
/// starting at the first `{`. Each group's content is a full expression, so
/// `\frac{a}{b}` becomes `Application(frac, [a, b])` and nested content
/// (`\frac{a \cdot c}{b \cdot d}`) parses normally inside each group.
fn parse_braced_args(
    tokens: &[Token],
    pos: usize,
    ctx: &Ctx,
) -> Result<(Vec<StructExpr>, usize), Diagnostic> {
    let mut args = Vec::new();
    let mut pos = pos;
    while let Some(Token::LBrace(_)) = tokens.get(pos) {
        let (inner, next) = parse_flat(tokens, pos + 1, 0, ctx)?;
        if !matches!(tokens.get(next), Some(Token::RBrace(_))) {
            return Err(synth_diag(next, "expected '}' after braced argument"));
        }
        args.push(inner);
        pos = next + 1;
    }
    if args.is_empty() {
        Err(synth_diag(pos, "expected braced arguments"))
    } else {
        Ok((args, pos))
    }
}

/// Pratt parser for flat infix/prefix operators.
///
/// Equality (`=`) binds loosest and is left-associative; everything looser is
/// handled by [`parse_bin_term`]. An infix symbol (`\Delta`, `\cap`, …) is bound
/// as the binary head applied to its two operands, so `a \Delta b` elaborates to
/// `Apply(Delta, [a, b])` rather than a headless nested application.
fn parse_flat(
    tokens: &[Token],
    pos: usize,
    _min_bp: u8,
    ctx: &Ctx,
) -> Result<(StructExpr, usize), Diagnostic> {
    let (mut lhs, mut pos) = parse_relation(tokens, pos, ctx)?;
    // The implication/type arrow binds LOOSEST — looser than `=`. So
    // `a = b \to c` reads as `(a = b) → c` (a theorem with an antecedent),
    // NOT `a = (b → c)`. Right-associative, and grouped alongside the loosest
    // propositional connectives.
    loop {
        match tokens.get(pos) {
            Some(Token::Arrow(id, _)) => {
                pos += 1;
                let (rhs, next) = parse_flat(tokens, pos, 0, ctx)?;
                lhs = StructExpr::Application(AtomicComposite::new(*id), vec![lhs, rhs]);
                pos = next;
            }
            Some(Token::Symbol(cid, _)) if is_logic_connective(ctx, *cid) => {
                pos += 1;
                let (rhs, next) = parse_relation(tokens, pos, ctx)?;
                lhs = StructExpr::Application(AtomicComposite::new(*cid), vec![lhs, rhs]);
                pos = next;
            }
            _ => break,
        }
    }
    Ok((lhs, pos))
}

/// Equality: `=` binds tighter than the propositional connectives but looser than
/// the binary operators handled in [`parse_bin_term`]. Left-associative, so a
/// chained `a = b = c` nests left (`(a = b) = c`).
fn parse_eq(
    tokens: &[Token],
    pos: usize,
    ctx: &Ctx,
) -> Result<(StructExpr, usize), Diagnostic> {
    let (mut lhs, mut pos) = parse_bin_term(tokens, pos, ctx)?;
    while matches!(tokens.get(pos), Some(Token::Eq(_))) {
        pos += 1;
        let (rhs, next) = parse_bin_term(tokens, pos, ctx)?;
        lhs = StructExpr::Application(AtomicComposite::new(EQ_SYM), vec![lhs, rhs]);
        pos = next;
    }
    Ok((lhs, pos))
}

/// Whether a symbol is a propositional connective that binds loosest.
fn is_logic_connective(ctx: &Ctx, id: u32) -> bool {
    // A connective excluded while parsing its own definition is NOT loose there.
    if EXCLUDED_CONNECTIVE.with(|c| c.get()) == id {
        return false;
    }
    let declared = ctx.by_id(id as i64).is_some_and(|r| {
        r.role.as_deref() == Some("connective") || ctx.has_facet(&r.tex, Facet::Connective)
    });
    // A corpus's connective is loose in a STATEMENT, where it connects two claims.
    // Inside a definition's `Sema:` it is a term: the `=` that links the body is the
    // format's, and a separator must not swallow it.
    if declared && !SEMA_BODY.with(|c| c.get()) {
        return true;
    }
    // Default: the two iff spellings, which are the connective a `Sema:` is
    // written with and so belong to the declaration FORMAT, not to any corpus's
    // mathematics. Every other connective — the boolean ring's `\land`/`\lor`
    // included — is a corpus glyph, and binds loosely only where its own
    // declaration says `Role: connective`.
    ctx.by_id(id as i64)
        .map(|r| matches!(r.tex.as_str(), "\\leftrightarrow" | "\\iff"))
        .unwrap_or(false)
}

/// Whether a symbol is a RELATION between two terms — a glyph whose own declaration
/// says `Role: relation`.
///
/// A relation binds looser than the arithmetic it compares and tighter than the
/// propositional connectives — a precedence level of its own. Without that level a
/// flat infix fold reads `a \cdot_{\mathbb{Z}} d \le_{\mathbb{Z}} c
/// \cdot_{\mathbb{Z}} b` as `((a \cdot_Z d) \le_Z c) \cdot_Z b`, and a definition
/// of the rational order applies `\cdot_Z` to a proposition; calling the same glyph
/// a `is_logic_connective` is not enough either, because then the iff and the order
/// share a level and the flat connective loop folds them left.
fn is_relation(ctx: &Ctx, id: u32) -> bool {
    ctx.by_id(id as i64)
        .map(|r| ctx.has_facet(&r.tex, Facet::Relation))
        .unwrap_or(false)
}

/// Refuse a glyph used as an operator that no declaration states.
///
/// The engine classifies no mathematical glyph of its own, so `a op b` is legitimate
/// exactly when the corpus declares `op` — by the shape of a glyph line (`a op b` is
/// infix) or with a `Role:`. A glyph nothing declares cannot head a term here, and
/// the message names it and says what a declaration looks like.
fn require_declared_operator(ctx: &Ctx, id: u32, pos: usize) -> Result<(), Diagnostic> {
    // A symbol-free context is a SYNTAX check — the LaTeX lint parses text with no
    // corpus behind it — so there is no corpus that owes a declaration, and no
    // error to report.
    if ctx.symbols().next().is_none() {
        return Ok(());
    }
    let code = "operator.undeclared";
    let Some(row) = ctx.by_id(id as i64) else {
        return Err(Diagnostic {
            severity: Severity::Error,
            phase: Phase::Structural,
            span: Span::new(pos, pos),
            message: "a symbol in operator position has no row in this corpus's table".into(),
            hints: Vec::new(),
            code,
        });
    };
    if ctx.is_declared(&row.tex)
        || qubitick_model::format::is_iff(&row.tex)
        || qubitick_model::format::is_arrow(&row.tex)
        || qubitick_model::format::is_binder(&row.tex)
    {
        // Declared by the corpus, or one of the declaration FORMAT's own tokens.
        return Ok(());
    }
    Err(Diagnostic {
        severity: Severity::Error,
        phase: Phase::Structural,
        span: Span::new(pos, pos),
        message: format!(
            "`{}` is used as an operator, but this corpus never declares it; declare it with a node whose glyph line is `a {} b` and a `Sema:`, a `Type:` or a `Role:`",
            row.tex, row.tex
        ),
        hints: Vec::new(),
        code,
    })
}

/// Relations: looser than `=`, tighter than the propositional connectives.
fn parse_relation(
    tokens: &[Token],
    pos: usize,
    ctx: &Ctx,
) -> Result<(StructExpr, usize), Diagnostic> {
    let (mut lhs, mut pos) = parse_eq(tokens, pos, ctx)?;
    loop {
        match tokens.get(pos) {
            Some(Token::Symbol(id, _)) if is_relation(ctx, *id) => {
                pos += 1;
                let (rhs, next) = parse_eq(tokens, pos, ctx)?;
                lhs = StructExpr::Application(AtomicComposite::new(*id), vec![lhs, rhs]);
                pos = next;
            }
            _ => break,
        }
    }
    Ok((lhs, pos))
}

/// Parse applications, the type arrow, and infix symbols (binding-power 1).
///
/// Consecutive operands become adjacent applications; a `Symbol` in infix
/// position is consumed as the binary head applied to its right operand.
/// Whether a symbol is a symmetric bracket glyph written `\Vert x \Vert`
/// (the corpus norm, D_BT.19 `\Vert x \Vert`, and `\ulcorner x \urcorner`).
/// The opening and closing bars are the SAME tex, so the pair must be threaded
/// through the parser: the first `\Vert` applies to the operand that follows,
/// and a trailing `\Vert` closes it.
fn is_sym_pair_glyph(ctx: &Ctx, id: u32) -> bool {
    // DB-first: a persisted role of "collector" marks a bracket glyph.
    if ctx.by_id(id as i64).and_then(|r| r.role.as_deref()) == Some("collector") {
        return true;
    }
    // Fallback for bracket glyphs that are only ever USED (no definition of their
    // own to derive a role from, e.g. `\ulcorner`/`\urcorner` in citations):
    // the compiled collector facet.
    ctx.by_id(id as i64)
        .map(|r| ctx.has_facet(&r.tex, Facet::Collector))
        .unwrap_or(false)
}

fn parse_bin_term(
    tokens: &[Token],
    pos: usize,
    ctx: &Ctx,
) -> Result<(StructExpr, usize), Diagnostic> {
    let (mut lhs, mut pos) = parse_atom(tokens, pos, ctx)?;
    // A symmetric pair glyph in OPERAND position opens a norm: `\Vert x \Vert`.
    // `parse_atom` returned the bare bar symbol; parse the operand it wraps as an
    // application and skip the closing bar.
    if let StructExpr::Sym(ac) = &lhs
        && is_sym_pair_glyph(ctx, ac.primary)
        && starts_operand(tokens.get(pos))
    {
        let (arg, next) = parse_bin_term(tokens, pos, ctx)?;
        // `parse_bin_term` stops at the closing bar (below), so `next` points at
        // it; consume it as the norm's closing delimiter.
        if matches!(tokens.get(next), Some(Token::Symbol(id2, _)) if is_sym_pair_glyph(ctx, *id2)) {
            pos = next + 1;
        } else {
            pos = next;
        }
        lhs = StructExpr::Application(ac.clone(), vec![arg]);
    }
    fold_infix(tokens, pos, ctx, lhs, 0)
}

/// The infix fold, ordered by the charter's master table (D10).
///
/// `min` is the lowest level this call may consume: an operator below it belongs to
/// the CALLER, which is how a tighter operator binds into the right operand, so
/// `a + b \times c` reads as `a + (b \times c)`. Equal levels nest LEFT — the caller
/// takes the next one — because association is a theorem (`(a + b) + c = a + (b + c)`),
/// not metadata, and left is the default the charter's table never has to say.
fn fold_infix(
    tokens: &[Token],
    pos: usize,
    ctx: &Ctx,
    mut lhs: StructExpr,
    min: i64,
) -> Result<(StructExpr, usize), Diagnostic> {
    let mut pos = pos;
    while let Some(Token::Symbol(id, _)) = tokens.get(pos) {
        let id = *id;
        if is_sym_pair_glyph(ctx, id) {
            // An unmatched closing norm bar terminates this term; the caller decides
            // whether to consume it as a closing delimiter.
            break;
        }
        if is_logic_connective(ctx, id) || is_relation(ctx, id) {
            // Propositional connectives, relations (and the implication arrow,
            // handled at the `parse_flat` level) bind looser than `=`; a binary arm
            // must not swallow them here.
            break;
        }
        // An operator is one a DECLARATION states. The engine owns no vocabulary to
        // fall back on, so a glyph nothing declares cannot head a term here: this is
        // the corpus's error, and it names it.
        require_declared_operator(ctx, id, pos)?;
        let level = infix_level(ctx, id);
        if level < min {
            break;
        }
        pos += 1;
        let (rhs, next) = parse_atom(tokens, pos, ctx)?;
        // The right operand takes only TIGHTER operators; an equal one is the outer
        // loop's, which is what makes the chain left-associative.
        let (rhs, next) = fold_infix(tokens, next, ctx, rhs, level + 1)?;
        lhs = StructExpr::Application(AtomicComposite::new(id), vec![lhs, rhs]);
        pos = next;
    }
    Ok((lhs, pos))
}

/// The level an infix operator binds at: the charter's master table, or 0 — looser
/// than any glyph the table names — for a glyph it does not name.
fn infix_level(ctx: &Ctx, id: u32) -> i64 {
    ctx.by_id(id as i64)
        .and_then(|r| r.precedence)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_db::queries::SymbolRow;

    fn ctx() -> Ctx {
        let row = |id: i64, tex: &str, kind: &str| SymbolRow { facets: 0,
            id, scope: "Era0/Strata0".into(), tex: tex.into(), display: tex.into(),
            lean_name: tex.trim_start_matches('\\').chars().map(|c| if c.is_ascii_alphanumeric(){c}else{'_'}).collect::<String>().trim_matches('_').to_string(),
            trigger: None, kind: kind.into(), role: None, precedence: None, home_node: 1,
            elab: None,
            binder_kind: None,
        };
        Ctx::from_rows("Era0/Strata0", vec![
            row(1, "\\tau", "notation"), row(2, "A", "notation"),
            row(3, "\\sum_{i : \\tau}", "notation"), row(4, "C", "notation"),
            row(5, "x", "notation"), row(6, "\\bigcup_{t : \\tau}", "notation"),
            row(7, "\\prod_{k : \\tau}", "notation"),
        ])
    }

    #[test]
    fn structural_binder_any_command() {
        // No hardcoded name list: a user-defined `\sum`-style composite with a
        // binder-shaped subscript binds over its operand.
        let c = ctx();
        let (e, _) = parse_flat(&crate::lexer::lex("\\sum_{i : \\tau} a_i", &c).unwrap(), 0, 0, &c).unwrap();
        match e {
            StructExpr::Application(ac, args) => {
                assert_eq!(ac.primary, 3, "head is sum binder");
                match &args[0] {
                    StructExpr::BinderBlock(BinderKind::Lambda, bs, _) => {
                        assert_eq!(bs[0].vars, vec!["i".to_string()]);
                    }
                    other => panic!("expected binder arg, got {other:?}"),
                }
            }
            other => panic!("expected application, got {other:?}"),
        }
    }

    #[test]
    fn limit_binder_arrow_subscript() {
        // A `\lim`-style binder uses `_{x \to 0}`, not `_{x : sort}`. It must
        // still bind its variable (BinderKind::Limit) — derived structurally.
        use qubitick_db::queries::SymbolRow;
        let row = |id: i64, tex: &str| SymbolRow { facets: 0,
            id, scope: "Era0/Strata0".into(), tex: tex.into(), display: tex.into(),
            lean_name: tex.trim_start_matches('\\').chars().map(|c| if c.is_ascii_alphanumeric(){c}else{'_'}).collect::<String>().trim_matches('_').to_string(),
            trigger: None, kind: "notation".into(), role: None, precedence: None, home_node: 1,
            elab: None,
            binder_kind: None,
        };
        let c = Ctx::from_rows("Era0/Strata0", vec![
            row(1, "\\lim_{x \\to 0}"), row(2, "f"), row(3, "x"), row(4, "0"),
        ]);
        let toks = crate::lexer::lex("\\lim_{x \\to 0} f(x)", &c).unwrap();
        let (se, _) = parse_struct(&toks, &c).unwrap();
        match se {
            StructExpr::Application(_, args) => match &args[0] {
                StructExpr::BinderBlock(kind, bs, _) => {
                    assert_eq!(*kind, BinderKind::Limit, "arrow subscript => Limit");
                    assert_eq!(bs[0].vars, vec!["x".to_string()]);
                }
                other => panic!("expected a limit binder, got {other:?}"),
            },
            other => panic!("expected application, got {other:?}"),
        }
    }

    #[test]
    fn curried_apply_nests_per_dblock() {
        use qubitick_db::queries::SymbolRow;
        let row = |id: i64, tex: &str, elab: Option<&str>| SymbolRow { facets: 0,
            id, scope: "Era0/Strata0".into(), tex: tex.into(), display: tex.into(),
            lean_name: tex.trim_start_matches('\\').chars().map(|c| if c.is_ascii_alphanumeric(){c}else{'_'}).collect::<String>().trim_matches('_').to_string(),
            trigger: None, kind: "notation".into(), role: None, precedence: None, home_node: 1,
            elab: elab.map(|s| s.to_string()),
            binder_kind: None,
        };
        let c = Ctx::from_rows("Era0/Strata0", vec![
            row(1, "f", Some("CurriedApply")),
            row(2, "a", None), row(3, "b", None),
        ]);
        let toks = crate::lexer::lex("f(a)(b)", &c).unwrap();
        let (se, _) = parse_struct(&toks, &c).unwrap();
        // f(a)(b) => Apply(f,[ Apply(f,[a]), Apply(f,[b]) ]) (nested per currying)
        match se {
            StructExpr::Application(_, outer) => {
                assert_eq!(outer.len(), 2, "two curried steps");
                assert!(matches!(outer[1], StructExpr::Application(_, _)));
            }
            other => panic!("expected nested application, got {other:?}"),
        }
    }

    #[test]
    fn set_literal_parses_as_set() {
        let row = |id: i64, tex: &str| SymbolRow { facets: 0,
            id, scope: "Era0/Strata0".into(), tex: tex.into(), display: tex.into(),
            lean_name: tex.trim_start_matches('\\').chars().map(|c| if c.is_ascii_alphanumeric(){c}else{'_'}).collect::<String>().trim_matches('_').to_string(),
            trigger: None, kind: "notation".into(), role: None, precedence: None, home_node: 1,
            elab: None,
            binder_kind: None,
        };
        let c = Ctx::from_rows("Era0/Strata0", vec![row(1, "a"), row(2, "b"), row(3, "c"), row(4, "S")]);
        let toks = crate::lexer::lex("\\{a, b, c\\}", &c).unwrap();
        let (se, _) = parse_struct(&toks, &c).unwrap();
        let expr = crate::semantic::elab(&se).unwrap();
        match expr {
            qubitick_model::Expr::Set(els) => assert_eq!(els.len(), 3, "three elements"),
            other => panic!("expected Set, got {other:?}"),
        }
    }

    #[test]
    fn vector_literal_parses_as_vector() {
        let row = |id: i64, tex: &str| SymbolRow { facets: 0,
            id, scope: "Era0/Strata0".into(), tex: tex.into(), display: tex.into(),
            lean_name: tex.trim_start_matches('\\').chars().map(|c| if c.is_ascii_alphanumeric(){c}else{'_'}).collect::<String>().trim_matches('_').to_string(),
            trigger: None, kind: "notation".into(), role: None, precedence: None, home_node: 1,
            elab: None,
            binder_kind: None,
        };
        let c = Ctx::from_rows("Era0/Strata0", vec![row(1, "a"), row(2, "b"), row(3, "c")]);
        let toks = crate::lexer::lex("(a, b, c)", &c).unwrap();
        let (se, _) = parse_struct(&toks, &c).unwrap();
        let expr = crate::semantic::elab(&se).unwrap();
        match expr {
            qubitick_model::Expr::Vector(els) => assert_eq!(els.len(), 3, "three elements"),
            other => panic!("expected Vector, got {other:?}"),
        }
    }
}
#[cfg(test)]
mod mtx2 {
    use super::*;
    use qubitick_db::queries::SymbolRow;
    #[test]
    fn matrix_2x2() {
        let row = |id: i64, tex: &str| SymbolRow { facets: 0,
            id, scope: "Era0/Strata0".into(), tex: tex.into(), display: tex.into(),
            lean_name: tex.trim_start_matches('\\').chars().map(|c| if c.is_ascii_alphanumeric(){c}else{'_'}).collect::<String>().trim_matches('_').to_string(),
            trigger: None, kind: "notation".into(), role: None, precedence: None, home_node: 1,
            elab: None,
            binder_kind: None,
        };
        let c = Ctx::from_rows("Era0/Strata0", vec![row(1,"a"),row(2,"b"),row(3,"c"),row(4,"d")]);
        let e = crate::semantic::parse_expr("\\begin{matrix} a & b \\\\ c & d \\end{matrix}", &c).unwrap();
        match e {
            qubitick_model::Expr::Matrix(rows) => {
                assert_eq!(rows.len(), 2, "two rows");
                assert_eq!(rows[0].len(), 2, "two cols");
                assert_eq!(rows[1].len(), 2, "two cols");
            }
            other => panic!("expected Matrix, got {other:?}"),
        }
    }
}