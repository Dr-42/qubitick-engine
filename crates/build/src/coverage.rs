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

/// Which of the two corpus holes a [`Gap`] names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapKind {
    /// An A block writes a glyph no D node in its stratum declares.
    Unhomed,
    /// A `Type:` gives a glyph a signature and no ground name, and no `Sema:`.
    NoGroundName,
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
            Some(Node::Definition(d)) => {
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
            Some(Node::Axiom(a)) => {
                let mut declared: BTreeSet<u32> = BTreeSet::new();
                for stmt in &a.statements {
                    glyphs(stmt, &mut declared);
                }
                writes.push((sector.clone(), path.clone(), declared));
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