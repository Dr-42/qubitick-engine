//! THE COVERAGE INVARIANT: every glyph an A block writes has a home.
//!
//! An A block is the corpus's statement of its *grammar* — the symbols and the ways they
//! compose. It emits nothing. But a grammar whose terminals mean nothing cannot be
//! resolved: `\Delta` composing associatively is a claim about *something*, and that
//! something is a D node in the same era/stratum saying where the glyph lives (`Type:`
//! for the ground's symbol, `Sema:` for the corpus's own definition).
//!
//! ```text
//! glyphs(A_era_stratum) ⊆ glyphs(D_era_stratum)
//! ```
//!
//! Per era/stratum, not per file: D nodes may be split across files, so the right-hand
//! side is the union over every Definition node in the stratum.
//!
//! This is the check that replaces the old `axiom` fallback. A glyph the corpus never
//! homes used to be emitted as an `axiom` — an assumption nobody chose. It is now a build
//! error naming the glyph.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use anyhow::Result;
use qubitick_model::{Expr, Node};

/// The separator between a glyph's symbol id and its text in an unresolved line's carry:
/// `12<sep>\Delta`, or `-1<sep>\Delta` for a glyph with no row at all.
const GLYPH_SEP: char = '\u{1}';

/// Which of the two corpus holes a [`Gap`] names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapKind {
    /// An A block writes a glyph no D node in its stratum declares.
    Unhomed,
    /// A `Type:` gives a glyph a signature and no ground name, and no `Sema:`.
    NoGroundName,
    /// A statement that did not resolve at all: the parser kept the line as opaque text
    /// because a glyph in it has no declaration.
    Unresolved,
    /// The parser REFUSED the statement as an operator nothing declares. The refusal is
    /// the parser's own sentence (`parse.operator.undeclared`), and it already names the
    /// glyph and says what a declaration looks like. This kind carries it to the report
    /// so the build does not answer with the collapsed text or a Lean type error instead.
    UndeclaredOperator,
}

/// One glyph an A block writes that no D node in its stratum homes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gap {
    /// The file the glyph appears in, as the walk named it — the renderer opens this.
    pub file: PathBuf,
    /// The glyph as the corpus writes it (`\Delta`).
    pub glyph: String,
    /// The era/stratum both sides are read in (`Era0/Strata0`).
    pub sector: String,
    pub kind: GapKind,
    /// Where the glyph sits, when the pass could locate it: 1-based line and column, and
    /// the width of the run to underline. A statement that resolved to nothing is found by
    /// its own text, so this is exact; a resolved statement has no positions yet, and the
    /// diagnostic points at the file instead of guessing.
    pub line: Option<u32>,
    pub col: Option<u32>,
    pub len: Option<u32>,
}

impl Gap {
    /// The diagnostic, in the shape rustc and Lean print: a title that says what is
    /// wrong, a position, and a `= help:` line that says what to do about it.
    pub fn diagnostic(&self) -> qubitick_model::Diagnostic {
        use qubitick_model::{Diagnostic, Phase, Span};
        let mut span = Span::file(self.file.display().to_string());
        if let (Some(line), Some(col)) = (self.line, self.col) {
            span.line = Some(line);
            span.col = Some(col);
            span.len = self.len;
        }
        let (title, help) = match self.kind {
            GapKind::UndeclaredOperator => (
                format!(
                    "`{}` is used as an operator, but nothing declares it",
                    self.glyph
                ),
                format!(
                    "declare it with a glyph line `a {} b` and a `Type:` or `Sema:`",
                    self.glyph
                ),
            ),
            GapKind::Unresolved => (
                format!("`{}` is not declared in {}", self.glyph, self.sector),
                "add a D node whose glyph line is that glyph, with a `Type:` or `Sema:` \
                 — nothing resolves it today, so this line means nothing"
                    .to_string(),
            ),
            GapKind::NoGroundName => (
                format!("`{}` has a `Type:` signature and no home", self.glyph),
                "write `Type: <ground name>` (e.g. `Type: ISA.Tick : \\mathsf{Type}`), \
                 or give the node a `Sema:`"
                    .to_string(),
            ),
            GapKind::Unhomed => (
                format!("`{}` is not declared in {}", self.glyph, self.sector),
                format!(
                    "add a D node whose glyph line is `{}`, with a `Type:` or `Sema:`",
                    self.glyph
                ),
            ),
        };
        /* THE CODE IS THE PARSER'S when the parser is the one that refused: a client acts
           on `parse.operator.undeclared`, not on a coverage code it would have to learn. */
        let (phase, code) = match self.kind {
            GapKind::UndeclaredOperator => (Phase::Parse, "parse.operator.undeclared"),
            _ => (Phase::Scope, "coverage.unhomed"),
        };
        Diagnostic::error(phase, code, title)
            .at(span)
            .label(match self.kind {
                GapKind::UndeclaredOperator => "used as an operator".to_string(),
                GapKind::Unresolved => "used here".to_string(),
                GapKind::NoGroundName => "signature with no name".to_string(),
                GapKind::Unhomed => "written here".to_string(),
            })
            .hint(help)
    }

    /// The one-line form, for a caller that prints a sentence rather than a block.
    pub fn sentence(&self) -> String {
        let d = self.diagnostic();
        let mut out = d.message.clone();
        for hint in &d.hints {
            out.push_str(&format!("\n  = help: {hint}"));
        }
        out
    }
}

/// Every glyph the corpus's A blocks write that its D nodes do not home.
///
/// `files` is the corpus walk (the files a build reports on); each file's own era/stratum
/// is the one its scope gives it, so a corpus that spreads its strata is checked stratum by
/// stratum and never against a neighbour's vocabulary.
pub fn gaps(conn: &qubitick_db::Db, files: &[PathBuf]) -> Result<Vec<Gap>> {
    let mut scope_ctxs: HashMap<String, qubitick_ctx::Ctx> = HashMap::new();
    // (era, strata) -> the glyph ids its D nodes DECLARE.
    let mut homed: HashMap<String, BTreeSet<u32>> = HashMap::new();
    // The A blocks: their sector, their file, and the glyphs they write.
    let mut writes: Vec<(String, PathBuf, BTreeSet<u32>)> = Vec::new();
    // Definitions that give a glyph a signature and no home: `Type: <sig>` with no name
    // and no `Sema:`. A constructor is exempt — its home is the formation that declares it.
    let mut undared: Vec<(String, PathBuf, u32)> = Vec::new();
    // Statements that resolved to nothing, paired with the sector and file they are in.
    // (sector, file, the line as written, the glyphs it carries)
    let mut unresolved: Vec<(String, PathBuf, String, Vec<String>)> = Vec::new();
    // Statements the PARSER refused, already carrying the sentence that says why.
    let mut refused: Vec<Gap> = Vec::new();
    let mut ctors: BTreeSet<u32> = BTreeSet::new();
    for path in files {
        let sector = crate::pipeline::scope_of_path(path);
        if !scope_ctxs.contains_key(&sector) {
            scope_ctxs.insert(sector.clone(), qubitick_ctx::build_ctx(conn, &sector)?);
        }
        let ctx = &scope_ctxs[&sector];
        let Ok(src) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(nodes) = qubitick_parse::parse_content(ctx, &src) else {
            // A file that does not parse is not this pass's business: the build reports
            // the row it refuses, per file, and coverage has nothing to add.
            continue;
        };
        match nodes.first() {
            Some(Node::Axiom(a)) => {
                let mut declared: BTreeSet<u32> = BTreeSet::new();
                for stmt in &a.statements {
                    unresolved_stmt(ctx, &sector, path, stmt, &mut unresolved, &mut refused);
                    glyphs(stmt, &mut declared);
                }
                writes.push((sector.clone(), path.clone(), declared));
            }
            // A THEOREM'S STATEMENT is read exactly as an A block's: a glyph nothing
            // declares leaves the line as opaque text, and a green build would say
            // nothing about it.
            Some(Node::Theorem(t)) => {
                unresolved_stmt(ctx, &sector, path, &t.statement, &mut unresolved, &mut refused);
            }
            Some(Node::Definition(d)) => {
                if let Some(qubitick_model::FormBody::MacroExpr(e)) = &d.d_block.form {
                    unresolved_stmt(ctx, &sector, path, e, &mut unresolved, &mut refused);
                }
                for extra in &d.extra_symbols {
                    if let Some(qubitick_model::FormBody::MacroExpr(e)) = &extra.d_block.form {
                        unresolved_stmt(ctx, &sector, path, e, &mut unresolved, &mut refused);
                    }
                }
                let set = homed.entry(sector.clone()).or_default();
                set.insert(d.symbol.primary);
                // A node may declare MORE than its glyph line: a formation's
                // constructors are declared by the node that forms the carrier.
                for extra in &d.extra_symbols {
                    set.insert(extra.symbol.primary);
                }
                if let Some(qubitick_model::FormBody::Formation(cs)) = &d.d_block.form {
                    for c in cs {
                        set.insert(c.constructor.primary);
                        ctors.insert(c.constructor.primary);
                    }
                }
                if d.d_block.sig.is_some()
                    && d.d_block.ground_name.is_none()
                    && d.d_block.form.is_none()
                {
                    undared.push((sector.clone(), path.to_path_buf(), d.symbol.primary));
                }
            }
            _ => {}
        }
    }
    let mut out: Vec<Gap> = Vec::new();
    // A REFUSED STATEMENT IS REPORTED AS THE PARSER REFUSED IT, before the word-scan and
    // in place of it: the sentence names the glyph, and the glyph nothing declares is
    // not something a pass over the text can find — it has no row to scan for.
    for gap in refused {
        if !out.contains(&gap) {
            out.push(gap);
        }
    }
    for (sector, file, ids) in writes {
        let ctx = &scope_ctxs[&sector];
        let declared = homed.get(&sector);
        for id in ids {
            let Some(row) = ctx.by_id(id as i64) else {
                continue;
            };
            /* THE DECLARATION FORMAT'S OWN ALPHABET IS NOT THE CORPUS'S TO HOME.
               `=` relates a `Sema:`'s two sides, the arrows type a signature, `\forall`
               introduces a binder's sort: the engine owns those tokens (crates/model/src/
               format.rs), no corpus declares them, and demanding a D node for one would
               be the engine asking the corpus to declare the format it is written in. */
            if id == qubitick_model::EQ_SYM || is_format_token(&row.tex) {
                continue;
            }
            if declared.is_some_and(|d| d.contains(&id)) {
                continue;
            }
            let gap = Gap {
                file: file.clone(),
                glyph: row.tex.clone(),
                sector: sector.clone(),
                kind: GapKind::Unhomed,
                line: None,
                col: None,
                len: None,
            };
            if !out.contains(&gap) {
                out.push(gap);
            }
        }
    }
    for (sector, absolute, raw, words) in unresolved {
        let declared = homed.get(&sector);
        let file = absolute.clone();
        // The line the statement was written on, so the caret lands under the glyph
        // rather than beside a file name.
        let located = locate(&absolute, &raw);
        for word in words {
            let (id, tex) = word.split_once(GLYPH_SEP).unwrap_or(("-1", word.as_str()));
            let id: i64 = id.parse().unwrap_or(-1);
            if id >= 0 && declared.is_some_and(|d| d.contains(&(id as u32))) {
                continue;
            }
            let (line, col, len) = match &located {
                Some((line, text)) => match text.find(tex) {
                    Some(at) => (
                        Some(*line),
                        Some(text[..at].chars().count() as u32 + 1),
                        Some(tex.chars().count() as u32),
                    ),
                    None => (Some(*line), None, None),
                },
                None => (None, None, None),
            };
            let gap = Gap {
                file: file.clone(),
                glyph: tex.to_string(),
                sector: sector.clone(),
                kind: GapKind::Unresolved,
                line,
                col,
                len,
            };
            if !out.contains(&gap) {
                out.push(gap);
            }
        }
    }
    for (sector, file, id) in undared {
        if ctors.contains(&id) {
            continue;
        }
        let glyph = scope_ctxs
            .get(&sector)
            .and_then(|c| c.by_id(id as i64))
            .map(|r| r.tex.clone())
            .unwrap_or_default();
        out.push(Gap {
            file,
            glyph,
            sector,
            kind: GapKind::NoGroundName,
            line: None,
            col: None,
            len: None,
        });
    }
    Ok(out)
}

/// The tokens the declaration format owns (README §2): `=`, the iff, the arrows, the
/// quantifiers, the citation corners.
fn is_format_token(tex: &str) -> bool {
    use qubitick_model::format;
    format::is_iff(tex)
        || format::is_arrow(tex)
        || format::is_implication(tex)
        || format::is_binder(tex)
        || tex == format::CITE_OPEN
        || tex == format::CITE_CLOSE
}

/// Every symbol an expression writes, in the corpus's own vocabulary.
///
/// Binder TYPES count: `\forall a b : \mathfrak{R}, ...` writes `\mathfrak{R}` even though
/// it is not an operand — a carrier the corpus never homes is exactly the glyph this pass
/// exists to name.
/// The glyphs a statement the parser could NOT resolve writes, left to be judged with every
/// other file's — a D node may be declared in a file the walk has not reached yet, and both
/// paths through this pass must answer "declares it" the same way.
///
/// The line is read off its TEXT, because that is all there is: a statement that resolves to
/// nothing is kept as an opaque `Var(<whole line>)`. And the glyphs are carried by name as
/// well as by id, because the most undeclared glyph there is is the one with no row at all —
/// the parse gave up before ingestion ever recorded it.
/// Every symbol an expression writes, in the corpus's own vocabulary.
///
/// Binder TYPES count: `\forall a b : \mathfrak{R}, ...` writes `\mathfrak{R}` even though
/// it is not an operand — a carrier the corpus never homes is exactly the glyph this pass
/// exists to name.
/// The glyphs a statement the parser could NOT resolve writes, left to be judged with every
/// other file's — a D node may be declared in a file the walk has not reached yet, and both
/// paths through this pass must answer "declares it" the same way.
///
/// The line is read off its TEXT, because that is all there is: a statement that resolves to
/// nothing is kept as an opaque `Var(<whole line>)`. And a glyph is carried by name as well
/// as by id, because the most undeclared glyph there is is the one with no row at all — the
/// parse gave up before ingestion ever recorded it.
///
/// Every symbol an expression writes, in the corpus's own vocabulary.
///
/// Binder TYPES count: `\forall a b : \mathfrak{R}, ...` writes `\mathfrak{R}` even though
/// it is not an operand — a carrier the corpus never homes is exactly the glyph this pass
/// exists to name.
/// Every symbol an expression writes, in the corpus's own vocabulary.
///
/// Binder TYPES count: `\forall a b : \mathfrak{R}, ...` writes `\mathfrak{R}` even though
/// it is not an operand — a carrier the corpus never homes is exactly the glyph this pass
/// exists to name.
/// A statement the parser could not resolve, reported with the glyph that stopped it.
///
/// Every symbol an expression writes, in the corpus's own vocabulary.
///
/// Binder TYPES count: `\forall a b : \mathfrak{R}, ...` writes `\mathfrak{R}` even though
/// it is not an operand — a carrier the corpus never homes is exactly the glyph this pass
/// exists to name.
fn unresolved_stmt(
    ctx: &qubitick_ctx::Ctx,
    sector: &str,
    path: &Path,
    e: &Expr,
    out: &mut Vec<(String, PathBuf, String, Vec<String>)>,
    refused: &mut Vec<Gap>,
) {
    let Expr::Var(raw) = e else { return };
    // A real metavariable is one identifier. A whole line of source is not a variable.
    if !raw.contains(char::is_whitespace) && !raw.contains('\\') {
        return;
    }
    /* WHEN THE PARSER CAN SAY WHY, IT SAYS IT. A statement kept as text may be one whose
       operator gate fired (`a \Delta b` with no declaration for `\Delta`): that refusal
       names the glyph and the fix, and the word-scan below would only guess from the text.
       The refusal is the corpus's diagnostic, so it is reported as one. */
    let mut refused_glyph: Option<String> = None;
    if let Some(said) = qubitick_parse::statement_refusal(ctx, e)
        && let Some(glyph) = said.split('`').nth(1)
        && said.contains("is used as an operator")
    {
        // A statement's text is the line it was written on (see `locate`).
        let located = locate(path, raw);
        let gap = Gap {
            file: path.to_path_buf(),
            glyph: glyph.to_string(),
            sector: sector.to_string(),
            kind: GapKind::UndeclaredOperator,
            line: located.as_ref().map(|(l, _)| *l),
            col: located.as_ref().and_then(|(_, text)| {
                text.find(glyph).map(|at| text[..at].chars().count() as u32 + 1)
            }),
            len: Some(glyph.chars().count() as u32),
        };
        refused_glyph = Some(glyph.to_string());
        if !refused.contains(&gap) {
            refused.push(gap);
        }
    }
    let mut words: Vec<String> = Vec::new();
    for tex in control_words(raw) {
        if is_format_token(&tex) {
            continue;
        }
        // The operator the gate refused is already in `refused`, with a better sentence.
        if refused_glyph.as_deref() == Some(tex.as_str()) {
            continue;
        }
        let id = ctx
            .by_tex(&tex)
            .and_then(|rows| rows.first())
            .map(|r| r.id)
            .unwrap_or(-1);
        words.push(format!("{id}{GLYPH_SEP}{tex}"));
    }
    if !words.is_empty() {
        let entry = (sector.to_string(), path.to_path_buf(), raw.clone(), words);
        if !out.contains(&entry) {
            out.push(entry);
        }
    }
}

/// The line a statement was written on, and its text.
///
/// The statement arrives as the parser's opaque `Var(<whole line>)`, so its text IS the
/// source line: the first line of the file whose trimmed text equals it. That is exact
/// for the case this exists for, and it returns `None` rather than a guess otherwise.
fn locate(file: &Path, raw: &str) -> Option<(u32, String)> {
    let src = std::fs::read_to_string(file).ok()?;
    let want = raw.trim();
    src.lines()
        .enumerate()
        .find(|(_, line)| line.trim() == want)
        .map(|(i, line)| (i as u32 + 1, line.to_string()))
}

/// The LaTeX control words a line writes, with a font command's braced group glued on
/// (`\mathfrak{E}` is one glyph, not `\mathfrak` plus `{E}`).
fn control_words(raw: &str) -> Vec<String> {
    let bytes = raw.as_bytes();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
            i += 1;
        }
        if i == start + 1 {
            continue; // a lone backslash is not a control word
        }
        if i < bytes.len() && bytes[i] == b'{' {
            let mut depth = 0usize;
            while i < bytes.len() {
                match bytes[i] {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
        }
        let word = raw[start..i].to_string();
        if !out.contains(&word) {
            out.push(word);
        }
    }
    out
}

/// Every symbol an expression writes, in the corpus's own vocabulary.
fn glyphs(e: &Expr, out: &mut BTreeSet<u32>) {
    match e {
        Expr::Sym(a) => {
            out.insert(a.primary);
        }
        Expr::Var(_) => {}
        Expr::Apply(head, args) => {
            out.insert(head.primary);
            for a in args {
                glyphs(a, out);
            }
        }
        Expr::Bracket(id, elems) => {
            out.insert(*id);
            for a in elems {
                glyphs(a, out);
            }
        }
        Expr::Binder(_, binders, body) => {
            for b in binders {
                glyphs(&b.ty, out);
            }
            glyphs(body, out);
        }
        Expr::Recursor(scrutinee, arms) => {
            glyphs(scrutinee, out);
            for (pat, body) in arms {
                glyphs_pattern(pat, out);
                glyphs(body, out);
            }
        }
        Expr::Matrix(rows) => {
            for r in rows {
                for a in r {
                    glyphs(a, out);
                }
            }
        }
        Expr::Vector(xs) | Expr::Set(xs) => {
            for a in xs {
                glyphs(a, out);
            }
        }
    }
}

fn glyphs_pattern(p: &qubitick_model::Pattern, out: &mut BTreeSet<u32>) {
    out.insert(p.constructor.primary);
    for sub in &p.sub {
        glyphs_pattern(sub, out);
    }
}

/// The path the build reports: corpus-relative where it can be, whole where it cannot.
pub fn relative_to_corpus(path: &Path) -> PathBuf {
    let root = crate::pipeline::corpus_root(path);
    path.strip_prefix(&root).unwrap_or(path).to_path_buf()
}