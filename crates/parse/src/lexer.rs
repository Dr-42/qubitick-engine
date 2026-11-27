//! The lexer: ASCII LaTeX → token stream (LexParseAst §3).
//!
//! Resolves each LaTeX command (`\tau`) to a `SymbolId` via the context's symbol
//! table (`symbols.tex`). ASCII identifiers that don't match a symbol become
//! `Var`s. Whitespace and `%`/`--` comments are discarded. Non-ASCII bytes are a
//! lex error.

use qubitick_ctx::Ctx;

/// Byte range in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }
}

/// A lexer token.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// A resolved symbol reference.
    Symbol(u32, Span),
    /// A bound variable (ASCII identifier not matching a symbol).
    Var(String, Span),
    /// Punctuation.
    LParen(Span),
    RParen(Span),
    LBrace(Span),
    RBrace(Span),
    LBracket(Span),
    RBracket(Span),
    LAngle(Span),
    RAngle(Span),
    Superscript(Span),
    Subscript(Span),
    Comma(Span),
    Colon(Span),
    Semicolon(Span),
    Bar(Span),
    /// Right-associative type arrow (`\to` / `\rightarrow`), carrying the
    /// resolved symbol id of the arrow (0 when unknown).
    Arrow(u32, Span),
    /// The ASCII equality operator `=`.
    Eq(Span),
    /// Universal quantifier `\forall` (a binder form).
    Forall(Span),
    /// Existential quantifier `\exists` (a binder form).
    Exists(Span),
    /// A recognized field keyword (e.g. `Thm:`).
    Field(FieldKind, Span),
    /// Matrix environment delimiters. `\begin{matrix}` and `\end{matrix}` are
    /// STRUCTURAL tokens (LexParseAst §5.4/§7.1), independent of the symbol table,
    /// so matrices parse without registering `\begin` as a symbol.
    Begin(Span),
    End(Span),
    /// Matrix column separator `&`.
    Ampersand(Span),
    /// Matrix row separator `\\`.
    RowSep(Span),
    /// End of input.
    Eof(Span),
}

/// Which quantifier a `\forall` / `\exists` keyword denotes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quant {
    Forall,
    Exists,
}

/// Recognized node-field keywords — the ones a node actually carries.
///
/// This list is the vocabulary of a `.qbit` body, and it is deliberately closed: a line whose
/// `Word:` is not here is not a field, and the parser reads it as a STATEMENT (crates/parse/src/
/// node.rs::split_fields has the same list, and crates/build/src/ingest.rs::is_field_keyword must
/// agree with both — a key known to one and not the others is a line that is a field to the
/// symbol scan and a statement to the parser). `Trigger`, `Kind`, `Tex`, `Prec`, `Assoc`, `Req`,
/// `Sig`, `Form`, `Thm` and `Shape` were here and are gone: they were recognized and then
/// read by nothing, so they only made a typo harder to see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// the glyph (a calibration's label)
    Symbol,
    /// definition: the defining equality
    Sema,
    /// definition: the symbolic signature
    Type,
    /// definition: the glyph's classification (`infix`, `relation`, `null`, ...)
    Role,
    /// definition: the Lean name the glyph IS, instead of one the engine defines
    Lean,
    /// theorem: the proof block
    Proof,
    /// axiom: the refutation protocol
    Falsify,
    /// prose, kept as a note on the node: `Math:` is the mathematical reading
    Math,
    /// prose, kept as a note on the node: `Phys:` is the physical reading
    Phys,
    /// calibration: q notation
    QUnits,
    /// calibration: `[T]`, `[1]`, `[L]`, ...
    Dims,
    /// calibration: the scale statement
    Rate,
}

impl FieldKind {
    fn parse(word: &str) -> Option<FieldKind> {
        Some(match word {
            "Symbol" => FieldKind::Symbol,
            "Sema" => FieldKind::Sema,
            "Type" => FieldKind::Type,
            "Role" => FieldKind::Role,
            "Lean" => FieldKind::Lean,
            "Proof" => FieldKind::Proof,
            "Falsify" => FieldKind::Falsify,
            "Math" => FieldKind::Math,
            "Phys" => FieldKind::Phys,
            "QUnits" => FieldKind::QUnits,
            "Dims" => FieldKind::Dims,
            "Rate" => FieldKind::Rate,
            _ => return None,
        })
    }
}

/// A lexical error.
#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub span: Span,
    pub message: String,
}


/// The control word for a Unicode token of the DECLARATION FORMAT.
///
/// A person writes `→` and `∀` where the format writes `\to` and `\forall`, so the
/// lexer accepts both — that is input normalisation of the format's own alphabet, and
/// it stops there. Which glyph a corpus writes for an operator, a relation, a
/// collector or a sort is the CORPUS's business: it declares the spelling it uses, and
/// the engine carries no dictionary from `∩` to `\cap`, because that dictionary is
/// the corpus's mathematics.
fn latex_name_of(c: char) -> Option<&'static str> {
    Some(match c {
        '→' => "\\to",
        '↔' => "\\leftrightarrow",
        '∀' => "\\forall",
        '∃' => "\\exists",
        _ => return None,
    })
}

/// A message for a byte the alphabet does not know.
///
/// The lexer walks bytes, so a multi-byte glyph arrives here one byte at a time;
/// reporting `'â'` for `∩` is both wrong and unhelpful. Decode the character and
/// name the control word the corpus actually uses.
fn unexpected_at(src: &str, i: usize) -> LexError {
    let ch = src[i..].chars().next().unwrap_or('\u{fffd}');
    let width = ch.len_utf8().max(1);
    let message = match latex_name_of(ch) {
        Some(name) => format!(
            "unexpected symbol '{ch}' (U+{:04X}): expressions in this corpus are written in \
             LaTeX, so write '{name}'",
            ch as u32
        ),
        None if ch.is_ascii() => format!("unexpected character '{ch}'"),
        None => format!("unexpected character '{ch}' (U+{:04X})", ch as u32),
    };
    LexError {
        span: Span::new(i, i + width),
        message,
    }
}

/// Tokenize ASCII LaTeX `src` against the context symbol table.
pub fn lex(src: &str, ctx: &Ctx) -> Result<Vec<Token>, LexError> {
    let bytes = src.as_bytes();
    let n = bytes.len();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < n {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        // Comments.
        if b == b'%' || (b == b'-' && i + 1 < n && bytes[i + 1] == b'-') {
            while i < n && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // Non-ASCII that doesn't match a registered symbol → error.
        // But first check if it matches a registered symbol (for Unicode operators like `×`).
        if !b.is_ascii() {
            let mut best_match: Option<(usize, u32)> = None;
            for row in ctx.symbols() {
                let tex = &row.tex;
                if !tex.is_empty()
                    && src[i..].starts_with(tex)
                    && best_match.is_none_or(|(len, _)| tex.len() > len)
                {
                    best_match = Some((tex.len(), row.id as u32));
                }
            }
            if let Some((len, id)) = best_match {
                tokens.push(Token::Symbol(id, Span::new(i, i + len)));
                i += len;
                continue;
            }
            return Err(unexpected_at(src, i));
        }
        // ASCII: try specific structural tokens first, then registered symbols.
        match b {
            b'(' => {
                tokens.push(Token::LParen(Span::new(i, i + 1)));
                i += 1;
            }
            b')' => {
                tokens.push(Token::RParen(Span::new(i, i + 1)));
                i += 1;
            }
            b'{' => {
                tokens.push(Token::LBrace(Span::new(i, i + 1)));
                i += 1;
            }
            b'}' => {
                tokens.push(Token::RBrace(Span::new(i, i + 1)));
                i += 1;
            }
            b'[' => {
                tokens.push(Token::LBracket(Span::new(i, i + 1)));
                i += 1;
            }
            b']' => {
                tokens.push(Token::RBracket(Span::new(i, i + 1)));
                i += 1;
            }
            b'^' => {
                // `^` followed by an operator character is ONE symbol the corpus may
                // declare (`^+`, `^*`, ...); otherwise `^` starts a superscript. The
                // rule is the format's, and it names no glyph.
                // `^` followed by an operator character is ONE symbol the corpus may
                // declare (`^+`, `^*`, `^⊙`). The character is DECODED, not read as a byte:
                // a byte read answers `None` for every non-ASCII operator, and the caller
                // then takes the superscript path for a glyph that is one symbol.
                if let Some(next) = src[i + 1..].chars().next()
                    && qubitick_model::corpus::is_operator_punct(next)
                {
                    let start = i;
                    let tex = format!("^{next}");
                    i += 1 + next.len_utf8();
                    resolve_symbol(&mut tokens, &tex, start, i, ctx)?;
                } else {
                    tokens.push(Token::Superscript(Span::new(i, i + 1)));
                    i += 1;
                }
            }
            b'_' => {
                tokens.push(Token::Subscript(Span::new(i, i + 1)));
                i += 1;
            }
            b',' => {
                tokens.push(Token::Comma(Span::new(i, i + 1)));
                i += 1;
            }
            b':' => {
                /* `:=` IS THE DEFINING RELATION, AND `:\iff` THE DEFINING IFF.
                 *
                 * Human symbolic mathematics writes a definition `a \star b := ...` and a
                 * relational definition `a \subseteq b :\iff ...`. Both spell a relation the
                 * declaration format already has, so they lex as that relation — one token
                 * each, here, so every reader of a `Sema:` body sees one shape. Left as a bare
                 * colon, the body failed to parse and the tolerant pass kept the whole
                 * definition as raw text: the node still looked like a definition and said
                 * nothing (measured on a corpus written with `:=`). */
                if i + 1 < n && bytes[i + 1] == b'=' {
                    tokens.push(Token::Eq(Span::new(i, i + 2)));
                    i += 2;
                    continue;
                }
                // `:\iff` — the colon is punctuation in front of the linking connective, so it
                // is dropped and the command lexes on its own, by the same arm as everywhere.
                for word in ["\\iff", "\\leftrightarrow"] {
                    if src[i + 1..].starts_with(word)
                        && !src[i + 1 + word.len()..]
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_ascii_alphabetic())
                    {
                        i += 1;
                        break;
                    }
                }
                if bytes[i] != b':' {
                    continue;
                }
                tokens.push(Token::Colon(Span::new(i, i + 1)));
                i += 1;
            }
            b';' => {
                tokens.push(Token::Semicolon(Span::new(i, i + 1)));
                i += 1;
            }
            b'|' => {
                tokens.push(Token::Bar(Span::new(i, i + 1)));
                i += 1;
            }
            b'=' => {
                tokens.push(Token::Eq(Span::new(i, i + 1)));
                i += 1;
            }
            b'&' => {
                tokens.push(Token::Ampersand(Span::new(i, i + 1)));
                i += 1;
            }
            b'\\' => {
                // `` \\ `` — the matrix ROW separator (two backslashes), handled
                // before generic command reading.
                if i + 1 < n && bytes[i + 1] == b'\\' {
                    tokens.push(Token::RowSep(Span::new(i, i + 2)));
                    i += 2;
                    continue;
                }
                // `\begin{...}` / `\end{...}` — matrix environment delimiters.
                let cmd_start = i + 1;
                let at_words: Vec<&[u8]> = if bytes[cmd_start..].starts_with(b"begin{") {
                    vec![b"begin{"]
                } else if bytes[cmd_start..].starts_with(b"end{") {
                    vec![b"end{"]
                } else {
                    Vec::new()
                };
                if let Some(kw) = at_words.first() {
                    let kwlen = kw.len();
                    if i + 1 + kwlen <= n {
                        // Find the closing `}` of the environment name.
                        if let Some(rel) = bytes[i + 1 + kwlen..].iter().position(|&b| b == b'}')
                        {
                            let end = i + 1 + kwlen + rel + 1;
                            tokens.push(if kw.starts_with(b"begin") {
                                Token::Begin(Span::new(i, end))
                            } else {
                                Token::End(Span::new(i, end))
                            });
                            i = end;
                            continue;
                        }
                    }
                }
                // LaTeX command.
                let start = i;
                // `\{` / `\}` — SET-LITERAL delimiters (Shape-driven, no glyph name):
                // `\{a, b, c\}` is a `Set`, per LexParseAst §5.4/§7.1. Recognised
                // structurally from the `\{`/`\}` escape pair before generic
                // command handling.
                if i + 1 < n && bytes[i + 1] == b'{' {
                    tokens.push(Token::LBrace(Span::new(i, i + 2)));
                    i += 2;
                    continue;
                }
                if i + 1 < n && bytes[i + 1] == b'}' {
                    tokens.push(Token::RBrace(Span::new(i, i + 2)));
                    i += 2;
                    continue;
                }
                i += 1;
                let mut word = String::new();
                while i < n && bytes[i].is_ascii_alphabetic() {
                    word.push(bytes[i] as char);
                    i += 1;
                }
                let mut tex = format!("\\{word}");
                // A command followed by ≥2 consecutive braced groups is a
                // braced-ARGUMENT macro: `\frac{n}{d}` = `Apply(frac, [n, d])`.
                // The bare `\command` heads an application and each {@…} group is
                // one argument, so the first brace is NOT glued into the symbol
                // tex; instead the braces become ordinary LBrace/…/RBrace tokens
                // below and the parser folds them into an application.
                //
                // Contrast font/decoration commands (`\mathbb{Z}`, `\hat{B}`),
                // which take exactly ONE braced group that is part of the symbol's
                // own tex (one symbol, not `Apply(mathbb,[Z])`). This is purely
                // STRUCTURAL (count of consecutive braced groups), so it is
                // data-driven: any new function macro with ≥2 braced args works
                // with no per-glyph code.
                // NOTE: peek only — the scanner below must NOT consume the braces,
                // so the braced-arg macro's `{…}` groups stay in the token stream
                // for the parser to fold into an application.
                let consecutive_braces =
                    qubitick_model::corpus::braced_group_count(bytes, i, n);
                let braced_args = consecutive_braces >= 2;
                if !braced_args && i < n && bytes[i] == b'{' {
                    i += 1;
                    let mut arg = String::new();
                    while i < n && bytes[i] != b'}' {
                        arg.push(bytes[i] as char);
                        i += 1;
                    }
                    if i < n {
                        i += 1; // consume '}'
                    }
                    tex.push('{');
                    tex.push_str(&arg);
                    tex.push('}');
                }
                // Absorb trailing mods (`_`/`^`) — NESTING-AWARE.
                absorb_mods(&mut tex, bytes, &mut i, n);
                // LaTeX spacing/noise commands carry no meaning on the surface.
                if is_noise_command(&tex) {
                    // `\;`, `\,`, `\!`, `\:` end with a NON-alpha after the bare `\`
                    // (so `word` above was empty and `tex` is just `\`). Consume the
                    // one punctuation char that follows so it does NOT surface as a
                    // stray `Semicolon`/etc. token that would truncate a `\land`-
                    // conjoined recursive `Sema:` at exactly the `\;\land\;` joins.
                    if word.is_empty() && i < n && !bytes[i].is_ascii_whitespace()
                        && (bytes[i] != b'_' && bytes[i] != b'^' && bytes[i] != b'{')
                    {
                        i += 1;
                    }
                    continue;
                }
                // Quantifier binders are recognized independent of the symbol table
                // (they are binder forms, not operands).
                let kind = match tex.as_str() {
                    "\\forall" => Some(Quant::Forall),
                    "\\exists" => Some(Quant::Exists),
                    _ => None,
                };
                if let Some(kind) = kind {
                    let span = Span::new(start, i);
                    tokens.push(match kind {
                        Quant::Forall => Token::Forall(span),
                        Quant::Exists => Token::Exists(span),
                    });
                    continue;
                }
                if matches!(tex.as_str(), "\\to" | "\\rightarrow" | "\\longrightarrow") {
                    // The type arrow is a right-associative infix; emit a
                    // dedicated token rather than a plain symbol operand.
                    let id = ctx
                        .by_tex(&tex)
                        .and_then(|rows| rows.first())
                        .map(|r| r.id as u32)
                        .unwrap_or(0);
                    tokens.push(Token::Arrow(id, Span::new(start, i)));
                } else {
                    resolve_symbol(&mut tokens, &tex, start, i, ctx)?;
                }
            }
            _ if b.is_ascii_alphabetic() || b.is_ascii_digit() => {
                let start = i;
                let mut word = String::new();
                while i < n && (bytes[i].is_ascii_alphanumeric()) {
                    word.push(bytes[i] as char);
                    i += 1;
                }
                // Field keyword followed by colon?
                if i < n
                    && bytes[i] == b':'
                    && let Some(kind) = FieldKind::parse(&word)
                {
                    tokens.push(Token::Field(kind, Span::new(start, i + 1)));
                    i += 1; // consume ':'
                    continue;
                }
                // Hygiene (Document 3 §2.1 / SurfaceFormat §5): a bare identifier
                // in a math expression is either a single lowercase metavariable
                // (x, a, k, …), a type/Prop sort, or — if longer — an error. No
                // English-word atoms (ih, begin, conclusion, rev, rfl, refl, …).
                // A bare numeric literal (`0`, `1`) is a symbol, not a metavariable.
                if !word.is_empty() && word.chars().all(|c| c.is_ascii_digit()) {
                    resolve_symbol(&mut tokens, &word, start, i, ctx)?;
                    continue;
                }
                // A bare identifier that names a DECLARED symbol (e.g. the canon's
                // swap/grading/Haar operators `S`, `T`, `U`) resolves to that symbol,
                // not a metavariable. Undeclared single lowercase letters stay vars.
                if let Some(rows) = ctx.by_tex(&word)
                    && let Some(r) = rows.first()
                {
                    tokens.push(Token::Symbol(r.id as u32, Span::new(start, i)));
                    continue;
                }
                let is_metavar =
                    word.len() == 1 && word.chars().next().is_some_and(|c| c.is_ascii_lowercase());
                if is_metavar || word == "Type" || word == "Prop" {
                    tokens.push(Token::Var(word, Span::new(start, i)));
                } else {
                    return Err(LexError {
                        span: Span::new(start, i),
                        message: format!(
                            "unknown symbol '{word}': bare multi-letter identifiers in a math \
                             expression are not allowed; write it as LaTeX (e.g. \\{word}) or a \
                             single-letter metavariable"
                        ),
                    });
                }
            }
            b'-' if i + 1 < n && bytes[i + 1].is_ascii_digit() => {
                // A negative numeric literal (`-1`); `--` was already taken as a
                // comment above.
                let start = i;
                i += 1;
                while i < n && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                let tex = &src[start..i];
                resolve_symbol(&mut tokens, tex, start, i, ctx)?;
            }
            _ => {
                // Match any registered LaTeX symbol not starting with `\` (e.g. `+_{\tau}`, `-_{\mathbb{Z}}`)
                let mut best_match: Option<(usize, u32)> = None;
                for row in ctx.symbols() {
                    let tex = &row.tex;
                    if !tex.is_empty()
                        && src[i..].starts_with(tex)
                        && best_match.is_none_or(|(len, _)| tex.len() > len)
                    {
                        best_match = Some((tex.len(), row.id as u32));
                    }
                }
                if let Some((len, id)) = best_match {
                    tokens.push(Token::Symbol(id, Span::new(i, i + len)));
                    i += len;
                    continue;
                }
                return Err(unexpected_at(src, i));
            }
        }
    }

    tokens.push(Token::Eof(Span::new(n, n)));
    Ok(tokens)
}

/// LaTeX commands that are pure spacing/styling noise on the declarative surface.
///
/// The corpus uses `\;`, `\land`-style plumbing and `\left`/`\right` sizing;
/// none of it carries mathematical content, so the lexer drops it rather than
/// failing the whole Sema into a raw-text fallback.
fn is_noise_command(tex: &str) -> bool {
    matches!(
        tex,
        "\\" | "\\;"
            | "\\,"
            | "\\!"
            | "\\:"
            | "\\quad"
            | "\\qquad"
            | "\\left"
            | "\\right"
            | "\\big"
            | "\\Big"
            | "\\bigg"
            | "\\Bigg"
            | "\\displaystyle"
            | "\\textstyle"
            | "\\limits"
            | "\\nolimits"
            | "\\space"
            | "\\thinspace"
            | "\\medspace"
            | "\\thickspace"
    )
}

/// Absorb trailing `_`/`^` mods into `tex`, e.g. `_{\tau}` or `_{\mathcal{R}}`.
///
/// Braces are matched with a DEPTH COUNTER rather than the first `}`: a mod may
/// contain its own braced group — `\mathcal{E}_{\mathcal{R}}`,
/// `\uparrow_{\mathbb{Z}^{-}}`, `\ge_{0}^{\mathbb{Z}}` — and a naive scan ends the
/// symbol early, leaving a dangling `}` and a tex that `ingest` never registered
/// (which silently degrades the whole statement to raw text).
///
/// `ingest::hand_mods` mirrors this byte for byte; if the two disagree the symbol
/// lookup misses.
fn absorb_mods(tex: &mut String, bytes: &[u8], i: &mut usize, n: usize) {
    while *i < n && (bytes[*i] == b'_' || bytes[*i] == b'^') {
        tex.push(bytes[*i] as char);
        *i += 1;
        if *i < n && bytes[*i] == b'{' {
            tex.push('{');
            *i += 1;
            let mut depth = 1usize;
            while *i < n {
                let c = bytes[*i];
                if c == b'{' {
                    depth += 1;
                } else if c == b'}' {
                    depth -= 1;
                    if depth == 0 {
                        tex.push('}');
                        *i += 1;
                        break;
                    }
                }
                tex.push(c as char);
                *i += 1;
            }
        } else if *i < n && bytes[*i] == b'\\' {
            // `+_\tau`, `\mathcal{U}_\odot`: the whole command belongs to the glyph.
            tex.push('\\');
            *i += 1;
            while *i < n && bytes[*i].is_ascii_alphabetic() {
                tex.push(bytes[*i] as char);
                *i += 1;
            }
            if *i < n && bytes[*i] == b'{' {
                tex.push('{');
                *i += 1;
                while *i < n && bytes[*i] != b'}' {
                    tex.push(bytes[*i] as char);
                    *i += 1;
                }
                if *i < n {
                    tex.push('}');
                    *i += 1;
                }
            }
        } else if *i < n {
            tex.push(bytes[*i] as char);
            *i += 1;
        }
    }
}

fn resolve_symbol(
    tokens: &mut Vec<Token>,
    tex: &str,
    start: usize,
    end: usize,
    ctx: &Ctx,
) -> Result<(), LexError> {
    if let Some(row) = ctx.by_tex(tex).and_then(|rows| rows.first()) {
        tokens.push(Token::Symbol(row.id as u32, Span::new(start, end)));
        return Ok(());
    }
    // Unknown command. In a real corpus this is a lex error; here we surface it.
    Err(LexError {
        span: Span::new(start, end),
        message: format!("unknown LaTeX command {tex}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_ctx::Ctx;
    use qubitick_db::queries::SymbolRow;

    fn row(id: i64, tex: &str, lean: &str) -> SymbolRow {
        SymbolRow { facets: 0,
            id,
            scope: "Era0/Strata0".into(),
            tex: tex.into(),
            display: tex.into(),
            lean_name: lean.into(),
            trigger: None,
            kind: "notation".into(),
            role: None,
            precedence: None,
            home_node: 1,
            elab: None,
            binder_kind: None,
        }
    }

    #[test]
    fn resolves_latex_command_to_symbol() {
        let ctx = Ctx::from_rows(
            "Era0/Strata0",
            vec![row(7, "\\tau", "Tau"), row(8, "\\cap", "Cap")],
        );
        let toks = lex("\\tau \\cap", &ctx).unwrap();
        assert_eq!(toks[0], Token::Symbol(7, Span::new(0, 4)));
        assert_eq!(toks[1], Token::Symbol(8, Span::new(5, 9)));
    }

    #[test]
    fn vars_for_unknown_idents() {
        let ctx = Ctx::from_rows("Era0/Strata0", vec![row(7, "\\tau", "Tau")]);
        let toks = lex("a b", &ctx).unwrap();
        assert!(matches!(toks[0], Token::Var(ref v, _) if v == "a"));
        assert!(matches!(toks[1], Token::Var(ref v, _) if v == "b"));
    }

    #[test]
    fn skips_comments_and_whitespace() {
        let ctx = Ctx::from_rows("Era0/Strata0", vec![row(7, "\\tau", "Tau")]);
        let toks = lex("% hello\n\\tau -- trailing", &ctx).unwrap();
        assert_eq!(toks[0], Token::Symbol(7, Span::new(8, 12)));
    }

    #[test]
    fn unknown_command_is_error() {
        let ctx = Ctx::from_rows("Era0/Strata0", vec![]);
        assert!(lex("\\nope", &ctx).is_err());
    }

    #[test]
    fn rejects_english_word_atom() {
        // Hygiene: a bare multi-letter English identifier (ih, begin, conclusion,
        // rev, rfl, refl, …) is NOT a valid math atom.
        let ctx = Ctx::from_rows("Era0/Strata0", vec![]);
        for word in ["ih", "begin", "conclusion", "rev", "rfl", "refl"] {
            match lex(word, &ctx) {
                Err(e) => assert!(e.message.contains("unknown symbol"), "got {:?}", e.message),
                Ok(_) => panic!("word '{word}' should be rejected"),
            }
        }
    }

    #[test]
    fn allows_single_letter_metavars_and_sorts() {
        let ctx = Ctx::from_rows("Era0/Strata0", vec![]);
        let toks = lex("x Type Prop", &ctx).unwrap();
        assert!(matches!(toks[0], Token::Var(ref v, _) if v == "x"));
        assert!(matches!(toks[1], Token::Var(ref v, _) if v == "Type"));
        assert!(matches!(toks[2], Token::Var(ref v, _) if v == "Prop"));
    }
}

