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
}

/// One glyph an A block writes that no D node in its stratum homes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gap {
    /// The A file the glyph appears in, corpus-relative.
    pub file: PathBuf,
    /// The glyph as the corpus writes it (`\Delta`).
    pub glyph: String,
    /// The era/stratum both sides are read in (`Era0/Strata0`).
    pub sector: String,
    pub kind: GapKind,
}

impl Gap {
    /// The sentence the build reports.
    pub fn sentence(&self) -> String {
        if self.kind == GapKind::Unresolved {
            return format!(
                "`{}` is used in {}\n  but this corpus never declares it, so the line it is \
                 in resolves to nothing and nothing can be emitted for it.\n  \
                 Declare it with a D node whose glyph line is that glyph, and a `Type:` or \
                 `Sema:`.",
                self.glyph,
                self.file.display()
            );
        }
        if self.kind == GapKind::NoGroundName {
            return format!(
                "`Type:` requires a ground name (e.g. `Type: ISA.Tick : \\mathsf{{Type}}`)\n  \
                 or a `Sema:` body. A glyph with no home cannot be emitted.\n  \
                 (`{}` in {} is declared with a signature and no name.)",
                self.glyph,
                self.file.display()
            );
        }
        format!(
            "`{}` appears in {}\n  but no D node in {} declares it.\n\
             Add a D node whose glyph line is `{}` with `Type:` or `Sema:`.",
            self.glyph,
            self.file.display(),
            self.sector,
            self.glyph
        )
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
    let mut unresolved: Vec<(String, PathBuf, Vec<String>)> = Vec::new();
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
                    unresolved_stmt(ctx, &sector, path, stmt, &mut unresolved);
                    glyphs(stmt, &mut declared);
                }
                writes.push((sector.clone(), path.clone(), declared));
            }
            // A THEOREM'S STATEMENT is read exactly as an A block's: a glyph nothing
            // declares leaves the line as opaque text, and a green build would say
            // nothing about it.
            Some(Node::Theorem(t)) => {
                unresolved_stmt(ctx, &sector, path, &t.statement, &mut unresolved);
            }
            Some(Node::Definition(d)) => {
                if let Some(qubitick_model::FormBody::MacroExpr(e)) = &d.d_block.form {
                    unresolved_stmt(ctx, &sector, path, e, &mut unresolved);
                }
                for extra in &d.extra_symbols {
                    if let Some(qubitick_model::FormBody::MacroExpr(e)) = &extra.d_block.form {
                        unresolved_stmt(ctx, &sector, path, e, &mut unresolved);
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
                    undared.push((sector.clone(), relative_to_corpus(path), d.symbol.primary));
                }
            }
            _ => {}
        }
    }
    let mut out: Vec<Gap> = Vec::new();
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
                file: relative_to_corpus(&file),
                glyph: row.tex.clone(),
                sector: sector.clone(),
                kind: GapKind::Unhomed,
            };
            if !out.contains(&gap) {
                out.push(gap);
            }
        }
    }
    for (sector, file, words) in unresolved {
        let declared = homed.get(&sector);
        for word in words {
            let (id, tex) = word.split_once(GLYPH_SEP).unwrap_or(("-1", word.as_str()));
            let id: i64 = id.parse().unwrap_or(-1);
            if id >= 0 && declared.is_some_and(|d| d.contains(&(id as u32))) {
                continue;
            }
            let gap = Gap {
                file: file.clone(),
                glyph: tex.to_string(),
                sector: sector.clone(),
                kind: GapKind::Unresolved,
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
    out: &mut Vec<(String, PathBuf, Vec<String>)>,
) {
    let Expr::Var(raw) = e else { return };
    // A real metavariable is one identifier. A whole line of source is not a variable.
    if !raw.contains(char::is_whitespace) && !raw.contains('\\') {
        return;
    }
    let mut words: Vec<String> = Vec::new();
    for tex in control_words(raw) {
        if is_format_token(&tex) {
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
        let entry = (sector.to_string(), relative_to_corpus(path), words);
        if !out.contains(&entry) {
            out.push(entry);
        }
    }
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