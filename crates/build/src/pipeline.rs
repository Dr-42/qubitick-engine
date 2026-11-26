//! The build pipeline orchestration (CodeArch §5.12).

use std::path::{Path, PathBuf};

use anyhow::Result;
use qubitick_ctx::Ctx;
use qubitick_model::{Diagnostic, Phase, Scope};

use crate::lean_check::check_file_diagnostics;

/// A single file's build outcome with per-file diagnostics.
///
/// `diagnostics` holds warnings as well as errors: a build is one report, and a
/// caller that wants only the fatal ones filters on [`Diagnostic::is_error`] — or
/// reads `verified`, which is exactly that. Warnings never make a file unverified.
#[derive(Debug, Clone)]
pub struct BuildFileResult {
    pub path: String,
    /// Parse + emit succeeded (the Lean was produced).
    pub ok: bool,
    /// The emitted Lean actually type-checked under the `lean` binary, and nothing
    /// it said was an error.
    pub verified: bool,
    pub diagnostics: Vec<Diagnostic>,
}

/// Build one `.qbit` file: parse → write ASTs → emit Lean → check.
///
/// Two independent things have to hold: every citation must name something the
/// corpus has (see [`crate::cites`]), and the emitted Lean must type-check. The
/// first is checked whether or not the second needs it — a proof that closes by
/// conversion never consults its citations, so Lean alone cannot notice that one
/// of them names a theorem nobody wrote.
pub fn build_file(
    conn: &qubitick_db::Db,
    path: &Path,
    ground: Option<&Path>,
) -> Result<BuildFileResult> {
    // THE FILE'S OWN CONTEXT, from its own scope. A build has no business being handed a scope by
    // its caller: that is how every file of a corpus came to be emitted against `Era0/Strata0`
    // whatever stratum it was written in (crates/api/src/state.rs::SCOPE).
    let ctx = qubitick_ctx::build_ctx(conn, &scope_of_path(path))?;
    let emitted = emit_file(&ctx, conn, path)?;
    let mut diagnostics = dangling_diagnostics(conn, path)?;
    diagnostics.extend(refused_row_diagnostics(&ctx, path)?);
    diagnostics.extend(latex_warnings(&ctx, path)?);
    diagnostics.extend(check_file_diagnostics(path, &emitted, ground)?);
    let verified = !diagnostics.iter().any(Diagnostic::is_error);
    Ok(BuildFileResult {
        path: path.display().to_string(),
        ok: true,
        verified,
        diagnostics,
    })
}

/// A file's rows that are NOT notation, with the parser's own sentence.
///
/// The parser owns the judgement (its retry ladder, the operator gate, the words), and it
/// refuses a row it cannot read. This asks it once, strictly, against the file's real
/// context: the tolerant passes elsewhere in the build exist so a corpus can be OPENED and
/// its problems REPORTED, which is only honest if the refusal still reaches the report.
fn refused_row_diagnostics(ctx: &Ctx, path: &Path) -> Result<Vec<Diagnostic>> {
    let src = std::fs::read_to_string(path)?;
    let file = path.display().to_string();
    qubitick_parse::tolerate_prose(false);
    match qubitick_parse::parse_content(ctx, &src) {
        Ok(nodes) => {
            let _ = nodes;
            Ok(Vec::new())
        }
        Err(e) => {
            let said = format!("{e:#}");
            let code = if said.contains("is used as an operator") {
                "parse.operator.undeclared"
            } else {
                "parse.not-notation"
            };
            Ok(vec![
                Diagnostic::error(Phase::Parse, code, said).in_file(file)
            ])
        }
    }
}

/// How many diagnostics in a build are warnings rather than errors.
///
/// Both faces that print a report (the CLI and the HTTP API) need this number, and
/// a count that disagrees between them would be a bug nobody notices until it does.
pub fn warning_count(results: &[BuildFileResult]) -> usize {
    results
        .iter()
        .flat_map(|r| r.diagnostics.iter())
        .filter(|d| !d.is_error())
        .count()
}

/// The theorems that are stated and not yet proved, as warnings.
///
/// ADMITTED IS A REAL STATE, AND IT MUST NOT READ AS VERIFIED. The corpus says "stated, not yet
/// proved" by carrying no proof steps (`Proof:` absent, empty, or `\sorry` — all three parse to
/// an empty vector) and the emitter turns that into an `axiom` of the node's own module, so the
/// file still checks. `verified` alone would then say "proved", which is exactly the lie the
/// placeholder `Eq.refl` used to tell. One warning per admitted theorem, carrying the node's own
/// line, and the report counts them (`admitted_count`).
fn admitted_diagnostics(nodes: &[qubitick_model::Node], src: &str, file: &str) -> Vec<Diagnostic> {
    /* one `.qbit` file is one node, so the node's header is the first line with something on it */
    let line = src
        .lines()
        .position(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with('%') && !t.starts_with('#') && !t.starts_with("//")
        })
        .map(|i| i as u32 + 1)
        .unwrap_or(1);
    nodes
        .iter()
        .filter_map(|n| {
            let qubitick_model::Node::Theorem(t) = n else { return None };
            if !t.proof.is_empty() {
                return None;
            }
            Some(
                Diagnostic::warning(
                    Phase::Build,
                    "proof.admitted",
                    format!(
                        "{} is admitted: it is stated and not proved",
                        t.base.name
                    ),
                )
                .in_file(file)
                .at_line(line)
                .hint("write its `Proof:` rows (each `= … [cite]`), or leave it admitted knowingly"),
            )
        })
        .collect()
}

/// How many theorems a build found admitted (see [`admitted_diagnostics`]).
pub fn admitted_count(results: &[BuildFileResult]) -> usize {
    results
        .iter()
        .flat_map(|r| r.diagnostics.iter())
        .filter(|d| d.code == "proof.admitted")
        .count()
}

/// The corpus's own typesetting, as warnings (see [`crate::latex`]).
pub fn latex_warnings(ctx: &Ctx, path: &Path) -> Result<Vec<Diagnostic>> {
    let src = std::fs::read_to_string(path)?;
    Ok(crate::latex::LatexLint::new(ctx).lint(path, &src))
}

/// Citations that name nothing, as diagnostics at file granularity.
///
/// The resolver reports them as sentences; the node and the citation are in the
/// text, so the file is the closest position available until citations carry spans.
fn dangling_diagnostics(conn: &qubitick_db::Db, path: &Path) -> Result<Vec<Diagnostic>> {
    let file = path.display().to_string();
    Ok(dangling_in_file(conn, path)?
        .into_iter()
        .map(|m| Diagnostic::error(Phase::Cite, "cite.dangling", m).in_file(file.clone()))
        .collect())
}

/// The citations in one file that name nothing the corpus has, resolved in the
/// file's own scope (the same context the emitter builds for it).
pub fn dangling_in_file(conn: &qubitick_db::Db, path: &Path) -> Result<Vec<String>> {
    let scope = scope_of_path(path);
    let ctx = qubitick_ctx::build_ctx(conn, &scope)?;
    let headers = qubitick_db::read::get_node_list(conn)?;
    let src = std::fs::read_to_string(path)?;
    let nodes = qubitick_parse::parse_content(&ctx, &src)?;
    crate::cites::dangling(conn, &ctx, &headers, &nodes)
}

/// Emit (but do not check) the Lean module for one `.qbit` file.
///
/// Split out of [`build_file`] so tooling can inspect exactly what a file emits
/// without paying for a `lean` invocation (`qubitick emit <file>`).
/// Per-build memory of the expensive, INVARIANT context lookups: the definition
/// corpus, the axiom classes, and the source-parsed definitions. Without this,
/// every file re-parsed all ~79 definitions + 6 axioms + every definition source
/// (O(files x definitions)); the emit phase dominated the build because of it.
#[derive(Default)]
pub struct EmitCache {
    defs: std::collections::HashMap<String, std::sync::Arc<Vec<crate::axioms::LoadedDefinition>>>,
    axioms: std::collections::HashMap<String, std::sync::Arc<Vec<qubitick_model::Node>>>,
    sources: std::collections::HashMap<String, std::sync::Arc<Vec<qubitick_model::Node>>>,
    /// `(division, definitional equation)` pairs for the RuleTable. Building these
    /// used to run `format!("{:?}") + parse_expr` for EVERY definition on EVERY
    /// file — the single most expensive part of the emit phase.
    def_rules: std::collections::HashMap<String, std::sync::Arc<Vec<(u32, qubitick_model::Expr)>>>,
}

pub fn emit_file(ctx: &Ctx, conn: &qubitick_db::Db, path: &Path) -> Result<String> {
    emit_file_cached(ctx, conn, path, &mut EmitCache::default())
}

/// The context ONE file is emitted in and diagnosed with: the symbols visible where it is written,
/// built from the scope its path implies and shared per scope across a build (scopes are few).
fn scope_ctx<'a>(
    conn: &qubitick_db::Db,
    path: &Path,
    ctxs: &'a mut std::collections::HashMap<String, qubitick_ctx::Ctx>,
) -> Result<&'a qubitick_ctx::Ctx> {
    let scope = scope_of_path(path);
    if !ctxs.contains_key(&scope) {
        ctxs.insert(scope.clone(), qubitick_ctx::build_ctx(conn, &scope)?);
    }
    Ok(ctxs.get(&scope).expect("inserted above"))
}

/// Every diagnostic a file has before Lean is asked: dangling citations, then the
/// corpus's own typesetting. Both need the file's scope context, so they share the
/// cached one — and both are gathered before the check, so a file with a bad glyph
/// still reports it even when Lean rejects the module outright.
fn file_diagnostics_cached(
    conn: &qubitick_db::Db,
    path: &Path,
    ctxs: &mut std::collections::HashMap<String, qubitick_ctx::Ctx>,
    headers: &[qubitick_db::read::NodeHeader],
) -> Result<Vec<Diagnostic>> {
    let ctx = scope_ctx(conn, path, ctxs)?;
    let src = std::fs::read_to_string(path)?;
    // TOLERANT: this is the pass that REPORTS a file's problems, so it must survive them.
    // A row the parser refuses is reported below, with the parser's own sentence.
    qubitick_parse::tolerate_prose(true);
    let nodes = qubitick_parse::parse_content(ctx, &src)?;
    qubitick_parse::tolerate_prose(false);
    let file = path.display().to_string();
    let mut out: Vec<Diagnostic> = crate::cites::dangling(conn, ctx, headers, &nodes)?
        .into_iter()
        .map(|m| Diagnostic::error(Phase::Cite, "cite.dangling", m).in_file(file.clone()))
        .collect();
    out.extend(refused_row_diagnostics(ctx, path)?);
    out.extend(crate::latex::LatexLint::new(ctx).lint(path, &src));
    out.extend(admitted_diagnostics(&nodes, &src, &file));
    Ok(out)
}

/// As [`emit_file`], reusing `cache` across calls (the `build` path uses one).
pub fn emit_file_cached(
    ctx: &Ctx,
    conn: &qubitick_db::Db,
    path: &Path,
    cache: &mut EmitCache,
) -> Result<String> {
    emit_file_with(ctx, conn, path, cache, &qubitick_emit::Options::inline())
}

/// As [`emit_file_cached`], but with emission options — the meshed package asks for
/// `Ground::External` (no frame, no class chain, no assumed lemmas) while keeping
/// the context assembly here, which is the part that made a re-derived context
/// disagree with the per-file path.
pub fn emit_file_with(
    ctx: &Ctx,
    conn: &qubitick_db::Db,
    path: &Path,
    cache: &mut EmitCache,
    opts: &qubitick_emit::Options,
) -> Result<String> {
    // Each file is emitted under its OWN scope (`EraN/StrataM` from its path), so a
    // theorem/axiom in a later era/stratum sees its own symbols plus the visible
    // prior-stratum/global context through `build_ctx`'s visibility rule.
    let file_scope = scope_of_path(path);
    let owned_ctx = if file_scope != ctx.scope() {
        Some(qubitick_ctx::build_ctx(conn, &file_scope)?)
    } else {
        None
    };
    let ctx: &Ctx = owned_ctx.as_ref().map_or(ctx, |c| c);
    let src = std::fs::read_to_string(path)?;
    let nodes = qubitick_parse::parse_content(ctx, &src)?;
    // Capture the theorem's own citations before `nodes` is moved below.
    let cited = crate::axioms::cited_divisions(&nodes);
    // (AST row writes would be performed here via db::write::write_expr.)
    //
    // A theorem's module must also declare the axiom classes it cites
    // (Document 5 §6.2/§6.3) and the DEFINITIONS it unfolds, but one node per
    // file means the theorem's file has neither — so both are loaded from the DB.
    // They are passed as CONTEXT: they contribute symbol declarations, the
    // rewrite dictionary, and observed arities to the module frame, but are not
    // themselves re-emitted. Re-emitting them collided with the frame
    // (`Qubitick.HL.<Name> has already been declared`), which cascaded through
    // every theorem module.
    let mut def_index: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let mut context: Vec<qubitick_model::Node> = Vec::new();
    // A definition's own module emits its Sema directly; it does not re-import the
    // definition corpus the way theorem/axiom modules must (to resolve citations,
    // class chains, and the authored `Type:` of glyphs like `\Vert` in A4). Loading
    // the definition corpus as context for a DEFINITION file collided with its own
    // emitted body (a `def times ... := tau_0` typed over `Prop` when the frame now
    // types `\times_\tau` over `tau`). So only axioms and theorems merge the corpus.
    if nodes.iter().any(|n| {
        matches!(n, qubitick_model::Node::Theorem(_) | qubitick_model::Node::Axiom(_))
    }) {
        let defs_cache_key = ctx.scope().to_string();
        let defs: std::sync::Arc<Vec<crate::axioms::LoadedDefinition>> = match cache
            .defs
            .get(&defs_cache_key)
        {
            Some(d) => d.clone(),
            None => {
                let d = std::sync::Arc::new(crate::axioms::load_definition_nodes(
                    conn,
                    ctx,
                    ctx.scope(),
                )?);
                cache.defs.insert(defs_cache_key.clone(), d.clone());
                d
            }
        };
        if nodes
            .iter()
            .any(|n| matches!(n, qubitick_model::Node::Theorem(_)))
        {
            // Theorems also load the axiom classes they cite.
            let ax_key = ctx.scope().to_string();
            let ax = match cache.axioms.get(&ax_key) {
                Some(a) => a.clone(),
                None => {
                    let a = std::sync::Arc::new(crate::axioms::load_axiom_nodes(
                        conn,
                        ctx,
                        ctx.scope(),
                    )?);
                    cache.axioms.insert(ax_key, a.clone());
                    a
                }
            };
            for a in ax.iter() {
                context.push(a.clone());
            }
        }
        for d in defs.iter() {
            let (node, division) = (&d.node, d.division);
            if let qubitick_model::Node::Definition(def) = node {
                def_index.insert(def.symbol.primary, division);
            }
            context.push(node.clone());
        }
        // Replace each definition with its SOURCE-parsed form, the only one carrying
        // the authored `Type:` signature (see `load_definition_sources`). Match on the
        // node NAME (the source parse resolves a glyph line by its FIRST control word,
        // which can land on a different id than the DB-rebuilt definition). Only a
        // source node that CARRIES `Type:` may replace a context node — duplicates
        // parsed without a sig must not clobber a sig-bearing one.
        let src_key = format!("{}|{}", ctx.scope(), corpus_root(path).display());
        let src_nodes = cached_sources(ctx, path, cache, &src_key);
        for node in src_nodes.iter().cloned() {
            if let qubitick_model::Node::Definition(d) = &node {
                // A source node may enter context when it carries the authored
                // `Type:` (so the frame knows a glyph's declared type), when it
                // DECLARES A FORMATION (so the module emits that inductive and types
                // its carrier), or when its `Sema:` is a recursive tick-operator
                // equation (so the module can emit the operator as a recursor `def`
                // instead of a bare `axiom`).
                let recursor = d.d_block.form.as_ref().is_some_and(|f| is_recursor_sema_form(f, ctx));
                let formation = matches!(
                    d.d_block.form,
                    Some(qubitick_model::FormBody::Formation(_))
                );
                if d.d_block.sig.is_some() || recursor || formation {
                    match context.iter_mut().find(|n| {
                        matches!(n, qubitick_model::Node::Definition(e) if e.base.name == d.base.name)
                    }) {
                        Some(slot) => {
                            // Transfer the division from the old definition to the new
                            // one's symbol.primary so def_index stays correct.
                            if let qubitick_model::Node::Definition(old_def) = slot {
                                let old_primary = old_def.symbol.primary;
                                if let Some(division) = def_index.remove(&old_primary) {
                                    def_index.insert(d.symbol.primary, division);
                                }
                            }
                            *slot = node;
                        }
                        None => context.push(node),
                    }
                }
            }
        }
    }
    // Formations (declarative custom inductives) must be in EVERY module's context so
    // operators defined by recursion over a formation can be emitted as `{carrier}.rec`
    // recursor defs. They are not re-emitted; they contribute their constructor algebra.
    let src_key2 = format!("{}|{}", ctx.scope(), corpus_root(path).display());
    for node in cached_sources(ctx, path, cache, &src_key2).iter().cloned() {
        if let qubitick_model::Node::Definition(d) = &node
            && matches!(d.d_block.form, Some(qubitick_model::FormBody::Formation(_)))
        {
            match context.iter_mut().find(|n| {
                matches!(n, qubitick_model::Node::Definition(e) if e.base.name == d.base.name)
            }) {
                Some(slot) => {
                    // Transfer the division from the old definition to the new
                    // one's symbol.primary so def_index stays correct.
                    if let qubitick_model::Node::Definition(old_def) = slot {
                        let old_primary = old_def.symbol.primary;
                        if let Some(division) = def_index.remove(&old_primary) {
                            def_index.insert(d.symbol.primary, division);
                        }
                    }
                    *slot = node;
                }
                None => context.push(node),
            }
        }
    }
    // Cite previously-proven theorems: load each `[T.BT.<n>]` statement so the
    // module can declare it as an assumed lemma and use it as a rewrite.
    let mut thm_index: std::collections::HashMap<u32, (String, qubitick_model::Expr)> =
        std::collections::HashMap::new();
    for div in cited {
        if let Some(t) = crate::axioms::load_theorem(conn, ctx, ctx.scope(), div)? {
            thm_index.insert(div, (t.name.clone(), t.statement));
        }
    }
    let mut rules = qubitick_rewrite::RuleTable::new();
    for (div, (name, stmt)) in &thm_index {
        rules.insert_theorem(*div, name.clone(), stmt.clone());
    }
    // Load the definitional rules into the RuleTable (built ONCE per scope and
    // cached: deriving them runs a Debug-format + re-parse per definition).
    let rules_key = ctx.scope().to_string();
    let def_rules = match cache.def_rules.get(&rules_key) {
        Some(v) => v.clone(),
        None => {
            let mut v: Vec<(u32, qubitick_model::Expr)> = Vec::new();
            for d in crate::axioms::load_definition_nodes(conn, ctx, ctx.scope())? {
                if let qubitick_model::Node::Definition(def) = &d.node
                    && let Some(form) = &def.d_block.form
                    && let Ok(expr) =
                        qubitick_parse::semantic::parse_expr(&format!("{form:?}"), ctx)
                {
                    v.push((d.division, expr));
                }
            }
            let v = std::sync::Arc::new(v);
            cache.def_rules.insert(rules_key, v.clone());
            v
        }
    };
    for (div, expr) in def_rules.iter() {
        rules.insert_definition(*div, expr.clone());
    }
    let mut all: Vec<qubitick_model::Node> = context.clone();
    all.extend(nodes.iter().cloned());
    if opts.ground == qubitick_emit::Ground::External {
        let emitted = qubitick_emit::emit_lean_text_meshed(
            ctx,
            &all,
            &nodes,
            "Qubitick.HL",
            &def_index,
            &thm_index,
            &mut rules,
            "Qubitick.Frame",
            opts.extra_imports.clone(),
            true,
            // The per-node external shape imports a `Qubitick.Frame` that carries
            // the chain.
            true,
        )?;
        return Ok(emitted);
    }
    let emitted = qubitick_emit::emit_lean_text_split(
        ctx,
        &context,
        &nodes,
        "Qubitick.HL",
        &def_index,
        &thm_index,
        &mut rules,
    )?;
    Ok(emitted)
}

/// The source-parsed definition nodes for a corpus root, memoised in `cache`.
fn cached_sources(
    ctx: &Ctx,
    path: &Path,
    cache: &mut EmitCache,
    key: &str,
) -> std::sync::Arc<Vec<qubitick_model::Node>> {
    if let Some(v) = cache.sources.get(key) {
        return v.clone();
    }
    let v = std::sync::Arc::new(load_definition_sources(ctx, &corpus_root(path)));
    cache.sources.insert(key.to_string(), v.clone());
    v
}

/// The corpus root for a `.qbit` file: the ancestor holding `prelude/`.
fn corpus_root(path: &Path) -> PathBuf {
    // Walk up until we find the directory that CONTAINS `prelude/` (the repo root).
    // The corpus files live under `project/Era0/...`, so their parents never sit
    // next to `prelude/`; the walk must reach the root that does.
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let mut dir = abs.parent().map(Path::to_path_buf).unwrap_or_default();
    // The corpus's own root is the ancestor whose children are the era trees. It is
    // the answer when the corpus carries no `prelude/` of its own (the engine embeds
    // the base ground), and reaching for the CWD instead scanned whatever tree the
    // process happened to start in — another corpus's definitions entered the module.
    let mut corpus: Option<PathBuf> = None;
    loop {
        if dir.join("prelude").join("Qubitick").join("HL.lean").is_file() {
            return dir;
        }
        if corpus.is_none() && (dir.join("Era0").is_dir() || dir.join("Era1").is_dir()) {
            corpus = Some(dir.clone());
        }
        match dir.parent() {
            Some(p) => dir = p.to_path_buf(),
            None => return corpus.unwrap_or_else(|| PathBuf::from(".")),
        }
    }
}

// Parse every definition SOURCE under `root`.

/// True when a definition's `MacroExpr` form is a recursive tick-operator equation
/// `land (Eq (f …τ₀) rhs₀) (∀ t:τ, Eq (f …⁺t) rhs₁)` — i.e. an operator the frame can
/// emit as a recursor `def` rather than a bare `axiom`. Cheap structural probe: the
/// top is a binary application whose second conjunct is a `\forall`.
fn is_recursor_sema_form(f: &qubitick_model::FormBody, ctx: &Ctx) -> bool {
    use qubitick_model::{BinderKind, Expr};
    match f {
        qubitick_model::FormBody::MacroExpr(e) => {
            // Flatten the (possibly left-nested) `land`: a recursor Sema is a
            // base equation plus one or more `\forall` successor equations —
            // 2-case τ forms AND 3-case ℤ forms (`\ge_{0}^{\mathbb{Z}}`
            // D_BT_78), whose nested `land (land eq0 (∀k, eq1)) (∀k, eq2)`
            // defeats a plain `args.len() == 2` + Forall check.
            let mut conjuncts: Vec<&Expr> = Vec::new();
            fn fl<'a>(x: &'a Expr, ctx2: &Ctx, out: &mut Vec<&'a Expr>) {
                match x {
                    Expr::Apply(h, args)
                        if qubitick_emit::lean_name_of(ctx2, h.primary) == "land" =>
                    {
                        for a in args {
                            fl(a, ctx2, out);
                        }
                    }
                    o => out.push(o),
                }
            }
            fl(e, ctx, &mut conjuncts);
            let has_forall = conjuncts
                .iter()
                .any(|c| matches!(c, Expr::Binder(BinderKind::Forall, _, _)));
            let has_eq = conjuncts
                .iter()
                .any(|c| matches!(c, Expr::Apply(h, _) if h.primary == qubitick_model::EQ_SYM));
            has_forall && has_eq
        }
        _ => false,
    }
}
///
/// `load_definition_nodes` rebuilds a definition from its stored Sema and does NOT
/// carry `d_block.sig` — the authored `Type:` line. That signature is the corpus's own
/// statement of a glyph's type (`\mathcal{R}(t)` + `Type: \tau \to \mathfrak{E}` is a
/// function `tau → Prop`), and without it the frame falls back to an inferred arity and
/// declares `mathcal_R : Prop → Prop`, which then refuses a `tau`-typed argument.
fn load_definition_sources(ctx: &Ctx, root: &Path) -> Vec<qubitick_model::Node> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "qbit") {
                files.push(p);
            }
        }
    }
    let mut seen_files = 0usize;
    let mut parsed = 0usize;
    let mut with_sig = 0usize;
    for path in files {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if !name.starts_with('D') {
            continue;
        }
        // Scope-proof the source scan: only consider definition files whose own
        // scope is visible to THIS module's scope. Otherwise an Era-1 definition
        // whose glyph happens to name a Part-0 symbol (e.g. `\mathcal{U}`) would
        // still parse under a Part-0 context and leak its `Type:` sig into a
        // Part-0 module, corrupting the axiom-class fields (`CoeFun` on the cited
        // axiom application).
        if !Scope(ctx.scope().to_string()).visible_from(&Scope(scope_of_path(&path))) {
            continue;
        }
        let Ok(src) = std::fs::read_to_string(&path) else { continue };
        let Ok(nodes) = qubitick_parse::parse_content(ctx, &src) else { continue };
        seen_files += 1;
        let defs: Vec<qubitick_model::Node> = nodes
            .into_iter()
            .filter(|n| matches!(n, qubitick_model::Node::Definition(_)))
            .collect();
        parsed += defs.len();
        with_sig += defs
            .iter()
            .filter(|n| matches!(n, qubitick_model::Node::Definition(d) if d.d_block.sig.is_some()))
            .count();
        out.extend(defs);
    }
    if std::env::var_os("QAUTH").is_some() {
        eprintln!(
            "QAUTH_SRC files={seen_files} defs={parsed} with_sig={with_sig} root={}",
            root.display()
        );
    }
    out
}

/// The scope (`EraN/StrataM`) a `.qbit` file belongs to, read from its path
/// components. Defaults to `Era0/Strata0` when the path carries no `Era`/`Strata`.
pub fn scope_of_path(path: &Path) -> String {
    let mut era = "Era0".to_string();
    let mut strata = 0i64;
    for comp in path.components() {
        let s = comp.as_os_str().to_string_lossy();
        if let Some(n) = s.strip_prefix("Era").and_then(|r| r.parse::<i64>().ok()) {
            era = format!("Era{n}");
        } else if let Some(m) = s.strip_prefix("Strata").and_then(|r| r.parse::<i64>().ok()) {
            strata = m;
        }
    }
    format!("{era}/Strata{strata}")
}

/// Run the full pipeline for a set of files.
///
/// Two phases: EMIT is serial (each file's module needs the shared DB/context),
/// then LEAN-CHECK runs in PARALLEL — `check_file` is self-contained (it writes a
/// uniquely named scratch module and shells out to `lean`), and Lean dominates
/// the build time. Emitting first and checking after turns a minutes-long
/// sequential wait into a few seconds of parallel work.
///
/// It takes NO context: each file is emitted and diagnosed in the scope its own path implies
/// (`scope_of_path`), because that is the only context a file's symbols are visible in. A caller
/// that hands a scope to a whole build is a caller that decides for every file at once, which is
/// what `Era0/Strata0` was doing to every corpus that did not live there.
///
/// `ground` is the directory `crate::ground::for_check` assembled for this corpus: the engine's
/// base with the project's `LeanDir` over it. It is threaded through rather than resolved per
/// file, because resolving it per file is how the charter's ground came to be ignored — the check
/// looked for a `prelude/` above the file and never asked the corpus. `None` is for a caller with
/// no corpus (tests, a single file), and then the old resolution applies.
pub fn run_pipeline(
    conn: &qubitick_db::Db,
    files: &[PathBuf],
    ground: Option<&Path>,
) -> Result<Vec<BuildFileResult>> {
    let t0 = std::time::Instant::now();
    // Phase 1 (serial): emit every module, each in the context of the scope it is written in.
    enum Prepared {
        Emitted(String),
        Failed(String),
    }
    let mut cache = EmitCache::default();
    let mut scope_ctxs: std::collections::HashMap<String, qubitick_ctx::Ctx> =
        std::collections::HashMap::new();
    let prepared: Vec<Prepared> = files
        .iter()
        .map(|f| match scope_ctx(conn, f, &mut scope_ctxs) {
            Ok(ctx) => match emit_file_cached(ctx, conn, f, &mut cache) {
                Ok(emitted) => Prepared::Emitted(emitted),
                Err(e) => Prepared::Failed(e.to_string()),
            },
            Err(e) => Prepared::Failed(e.to_string()),
        })
        .collect();

    let t_emit = t0.elapsed();
    // Phase 2 (parallel): Lean-check the emitted modules.
    // Parallelism: `QUBITICK_JOBS` overrides; else the machine's parallelism.
    // (Inside a nix-shell `available_parallelism` can report 1, which would
    // silently serialise the whole check phase.)
    let threads = std::env::var("QUBITICK_JOBS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
        })
        .clamp(1, 16);
    let checks: Vec<std::sync::Mutex<Option<Result<Vec<Diagnostic>>>>> =
        (0..files.len()).map(|_| std::sync::Mutex::new(None)).collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..threads.min(files.len().max(1)) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if i >= files.len() {
                        break;
                    }
                    if let Prepared::Emitted(emitted) = &prepared[i] {
                        let outcome = check_file_diagnostics(&files[i], emitted, ground);
                        *checks[i].lock().expect("check slot lock") = Some(outcome);
                    }
                }
            });
        }
    });

    let t_check = t0.elapsed() - t_emit;
    if std::env::var("QUBITICK_TIMING").is_ok() {
        eprintln!(
            "TIMING emit={:?} check={:?} files={}",
            t_emit,
            t_check,
            files.len()
        );
    }
    // Phase 3: assemble results in the original file order (deterministic output).
    // Citation resolution needs each file's own scope context; scopes are few, so
    // one context (and one node listing) is built per scope rather than per file.
    let headers = qubitick_db::read::get_node_list(conn)?;
    let mut results = Vec::with_capacity(files.len());
    for (i, f) in files.iter().enumerate() {
        let path = f.display().to_string();
        match &prepared[i] {
            Prepared::Failed(e) => {
                // A file that could not be EMITTED still gets its own diagnostics: the
                // reason the emit failed is often a row or a citation the rest of the
                // report can name precisely, and a build that says only "emit failed"
                // hides the sentence the corpus needs.
                let mut diagnostics = file_diagnostics_cached(conn, f, &mut scope_ctxs, &headers)?;
                diagnostics.insert(
                    0,
                    Diagnostic::error(Phase::Emit, "emit.failed", e.clone()).in_file(path.clone()),
                );
                results.push(BuildFileResult {
                    path: path.clone(),
                    ok: false,
                    verified: false,
                    diagnostics,
                })
            }
            Prepared::Emitted(_) => {
                let outcome = checks[i]
                    .lock()
                    .expect("check slot lock")
                    .take()
                    .unwrap_or_else(|| {
                        Err(anyhow::anyhow!("file was never checked"))
                    });
                let mut diagnostics =
                    file_diagnostics_cached(conn, f, &mut scope_ctxs, &headers)?;
                match outcome {
                    Ok(ds) => diagnostics.extend(ds),
                    Err(e) => diagnostics.push(
                        Diagnostic::error(Phase::Build, "build.lean-failed", e.to_string())
                            .in_file(path.clone()),
                    ),
                }
                let verified = !diagnostics.iter().any(Diagnostic::is_error);
                results.push(BuildFileResult {
                    path,
                    ok: true,
                    verified,
                    diagnostics,
                });
            }
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AN ADMITTED THEOREM IS REPORTED, NOT HIDDEN.
    ///
    /// The corpus says "stated, not yet proved" by carrying no steps, and the emitter turns that
    /// into an `axiom` — so the FILE still verifies and `verified` alone would read as "proved".
    /// One warning per admitted theorem, carrying its file and line, plus the report's count.
    #[test]
    fn an_admitted_theorem_is_a_warning_and_a_count() {
        let src = "T.X.01. Open\n\\forall a : \\mathfrak{E}, a \\Delta \\varnothing = a\nProof:\n\\sorry\n";
        let thm = |proof: Vec<qubitick_model::ProofStep>| {
            qubitick_model::Node::Theorem(qubitick_model::Theorem {
                base: qubitick_model::NodeBase {
                    id: 1,
                    kind: qubitick_model::NodeKind::Theorem,
                    era: qubitick_model::Era::Era0,
                    strata: qubitick_model::Strata(0),
                    name: "Open".into(),
                    req: vec![],
                    derived: vec![],
                    note: None,
                },
                statement: qubitick_model::Expr::Sym(qubitick_model::AtomicComposite::new(1)),
                proof,
                tag: qubitick_model::PhysOrMath::Math,
            })
        };

        let warned = admitted_diagnostics(&[thm(vec![])], src, "T_01_Open.qbit");
        assert_eq!(warned.len(), 1, "one warning per admitted theorem");
        assert_eq!(warned[0].code, "proof.admitted");
        assert!(!warned[0].is_error(), "an admission is not an error: the node is just not proved");
        assert_eq!(warned[0].span.line, Some(1), "the warning points at the node's own line");
        assert_eq!(warned[0].span.file.as_deref(), Some("T_01_Open.qbit"));
        assert!(warned[0].message.contains("Open"), "{}", warned[0].message);

        /* a theorem WITH a proof says nothing, and the count follows the warnings */
        let proven = admitted_diagnostics(
            &[thm(vec![qubitick_model::ProofStep::Chain { rows: vec![] }])],
            src,
            "T_01_Open.qbit",
        );
        assert!(proven.is_empty(), "a proof-carrying theorem is not admitted");
        let result = |diagnostics| BuildFileResult {
            path: "T_01_Open.qbit".into(),
            ok: true,
            verified: true,
            diagnostics,
        };
        assert_eq!(admitted_count(&[result(warned)]), 1);
        assert_eq!(admitted_count(&[result(proven)]), 0);
    }

    /// END-TO-END "bring your own axioms" regression: a completely independent
    /// corpus (its own axioms, its own `=`-macro definition, and a theorem that
    /// CITES that definition) must build and Lean-verify with no engine edits.
    /// Requires the `lean` binary (as the other check_file tests do).
    /// Mark a fixture row as DECLARED with facet bits — what a corpus's `Role:` writes
    /// into the DB. A row with no claim on it classifies nothing.
    fn declaring(
        mut row: qubitick_db::queries::SymbolRow,
        facets: &[qubitick_model::corpus::Facet],
    ) -> qubitick_db::queries::SymbolRow {
        for f in facets {
            row.facets |= f.bits();
        }
        row
    }

    /// The declarations a fixture corpus OWES.
    ///
    /// The engine classifies no mathematical glyph of its own, so a corpus that writes
    /// `\Delta`, `\cap` or `\varnothing` declares them: one node per glyph, and the
    /// division numbers are distinct because two nodes claiming one number is an
    /// ingest error.
    fn declare_ops(root: &std::path::Path) {
        let defs = root.join("Era0/Strata0/Definitions");
        std::fs::create_dir_all(&defs).unwrap();
        std::fs::write(
            defs.join("D_00_Delta.qbit"),
            "D.X.00. Delta\na \\Delta b\nRole: nullity\n",
        )
        .unwrap();
        std::fs::write(defs.join("D_02_Cap.qbit"), "D.X.02. Cap\na \\cap b\n").unwrap();
        std::fs::write(
            defs.join("D_03_Empty.qbit"),
            "D.X.03. Empty\n\\varnothing\nRole: null\n",
        )
        .unwrap();
    }

    #[test]
    fn independent_corpus_verifies() {
        if std::process::Command::new("lean")
            .arg("--version")
            .output()
            .is_err()
        {
            return; // no lean in this environment
        }
        let root = std::env::temp_dir().join(format!("qbtick_own_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Era0/Strata0/Axioms")).unwrap();
        std::fs::create_dir_all(root.join("Era0/Strata0/Definitions")).unwrap();
        std::fs::create_dir_all(root.join("Era0/Strata0/Theorems")).unwrap();
        declare_ops(&root);
        std::fs::write(
            root.join("Era0/Strata0/Axioms/A1_Base.qbit"),
            "A1. Base\n\\forall a b c : \\mathfrak{E}, (a \\Delta b) \\Delta c = a \\Delta (b \\Delta c)\na \\Delta \\varnothing = a\na \\Delta a = \\varnothing\na \\cap \\mathbb{1} = a\n",
        )
        .unwrap();
        std::fs::write(
            root.join("Era0/Strata0/Definitions/D_01_Wedge.qbit"),
            "D.X.01. Wedge\na \\wedge b\nSema:\na \\wedge b = \\mathbb{1} \\Delta (a \\Delta b)\n",
        )
        .unwrap();
        std::fs::write(
            root.join("Era0/Strata0/Theorems/T_01_Self.qbit"),
            "T.X.01. WedgeSelf\n\\forall a : \\mathfrak{E}, a \\wedge a = \\mathbb{1}\nProof:\na \\wedge a\n  = \\mathbb{1} \\Delta (a \\Delta a)    [D.X.01]\n  = \\mathbb{1} \\Delta \\varnothing     [A1.3]\n  = \\mathbb{1}                     [A1.2]\n",
        )
        .unwrap();
        let mut conn = qubitick_db::open_in_memory().unwrap();
        crate::ingest_corpus(&mut conn, &root).unwrap();
        let files: Vec<std::path::PathBuf> = crate::walk_qbit(&root);
        let ground = crate::ground::for_check(&root).unwrap();
        let results = run_pipeline(&conn, &files, Some(&ground)).unwrap();
        assert_eq!(
            results.len(),
            6,
            "axiom + definition + theorem, plus the three declarations they use"
        );
        for r in &results {
            assert!(
                r.verified,
                "{} must verify: {:?}",
                r.path,
                r.diagnostics.first()
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// THE PROJECT'S OWN GROUND, end to end — the decoupling's one promise.
    ///
    /// A corpus whose charter names `LeanDir:` must be checked against THAT ground: the engine's
    /// base with the project's `HL.lean` over it. The second half is the telling, and it is a
    /// NEGATIVE control: the base is left untouched and only the project's file is broken, so a
    /// build that still verifies is a build checking against a file the project did not write.
    /// (Before this, it did: `check_file` looked for a `prelude/` above the file and never asked
    /// the corpus, so a project could ship a broken `HL.lean` and every file still "verified".)
    #[test]
    fn a_corpus_is_checked_against_the_ground_its_charter_names() {
        if std::process::Command::new("lean")
            .arg("--version")
            .output()
            .is_err()
        {
            return; // no lean in this environment
        }
        /* the same small corpus the independent-corpus test uses: it verifies, so anything this
           test says is about the GROUND and not about the corpus */
        let root = std::env::temp_dir().join(format!("qbtick_ground_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for d in ["Axioms", "Definitions", "Theorems"] {
            std::fs::create_dir_all(root.join("Era0/Strata0").join(d)).unwrap();
        }
        declare_ops(&root);
        std::fs::write(
            root.join("Era0/Strata0/Axioms/A1_Base.qbit"),
            "A1. Base\n\\forall a b c : \\mathfrak{E}, (a \\Delta b) \\Delta c = a \\Delta (b \\Delta c)\na \\Delta \\varnothing = a\na \\Delta a = \\varnothing\na \\cap \\mathbb{1} = a\n",
        )
        .unwrap();
        std::fs::write(
            root.join("Era0/Strata0/Definitions/D_01_Wedge.qbit"),
            "D.X.01. Wedge\na \\wedge b\nSema:\na \\wedge b = \\mathbb{1} \\Delta (a \\Delta b)\n",
        )
        .unwrap();
        std::fs::write(
            root.join("Era0/Strata0/Theorems/T_01_Self.qbit"),
            "T.X.01. WedgeSelf\n\\forall a : \\mathfrak{E}, a \\wedge a = \\mathbb{1}\nProof:\na \\wedge a\n  = \\mathbb{1} \\Delta (a \\Delta a)    [D.X.01]\n  = \\mathbb{1} \\Delta \\varnothing     [A1.3]\n  = \\mathbb{1}                     [A1.2]\n",
        )
        .unwrap();
        /* the project brings its own ground — the shipped one to start with, so a green build
           here is about WHERE it was read from, not about what it says */
        std::fs::create_dir_all(root.join("ground/Qubitick")).unwrap();
        std::fs::write(root.join("ground/Qubitick/HL.lean"), crate::ground::HL_LEAN).unwrap();
        std::fs::write(
            root.join("Canon.qbit"),
            "P. Own Ground\nId: own-ground\nKind: project\nModule: Qubitick.HL\nLeanDir: ground\n",
        )
        .unwrap();
        let mut conn = qubitick_db::open_in_memory().unwrap();
        crate::ingest_corpus(&mut conn, &root).unwrap();
        let ground = crate::ground::for_check(&root).expect("the project's ground assembles");
        let results =
            run_pipeline(&conn, &crate::walk_qbit(&root), Some(&ground)).unwrap();
        assert_eq!(results.len(), 6, "three nodes plus the three declarations they use");
        for r in &results {
            assert!(
                r.verified,
                "{} must verify against the project's own ground: {:?}",
                r.path,
                r.diagnostics.first()
            );
        }

        /* break the PROJECT's ground only: the base under it is still the shipped one */
        let hl = root.join("ground/Qubitick/HL.lean");
        let mut broken = std::fs::read_to_string(&hl).unwrap();
        broken.push_str("\n-- names nothing, so the file cannot typecheck; nothing else changed\ndef ground_marker : Type := NoSuchGroundType\n");
        std::fs::write(&hl, broken).unwrap();
        let err = crate::ground::for_check(&root)
            .expect_err("a project's broken ground must fail its build, not fall back to ours");
        // `{:#}` prints the whole chain: the context names the ground, the cause says what is wrong
        let msg = format!("{err:#}");
        assert!(
            msg.contains("LeanDir: ground"),
            "the refusal must name the project's own ground, not a temp path: {msg}"
        );
        assert!(
            msg.contains("failed to compile") || msg.contains("does not meet"),
            "and say what is wrong with it: {msg}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Build a throwaway corpus whose ONLY axiom is `axiom_id` (e.g. `A4`), with
    /// the definitions and the proof that cite it. `cite` is the binding used in
    /// the proof, so a caller can cite the numbered axiom — or a number that no
    /// node claims.
    fn byo_root(tag: &str, axiom_id: &str, cite: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("qbtick_own_{}_{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&root);
        for d in ["Axioms", "Definitions", "Theorems"] {
            std::fs::create_dir_all(root.join("Era0/Strata0").join(d)).unwrap();
        }
        declare_ops(&root);
        std::fs::write(
            root.join(format!("Era0/Strata0/Axioms/{axiom_id}_Base.qbit")),
            format!(
                "{axiom_id}. Base\n\
                 \\forall a b c : \\mathfrak{{E}}, (a \\Delta b) \\Delta c = a \\Delta (b \\Delta c)\n\
                 a \\Delta \\varnothing = a\n\
                 a \\Delta a = \\varnothing\n"
            ),
        )
        .unwrap();
        std::fs::write(
            root.join("Era0/Strata0/Definitions/D_01_Wedge.qbit"),
            "D.X.01. Wedge\na \\wedge b\nSema:\na \\wedge b = \\mathbb{1} \\Delta (a \\Delta b)\n",
        )
        .unwrap();
        std::fs::write(
            root.join("Era0/Strata0/Theorems/T_01_Self.qbit"),
            format!(
                "T.X.01. WedgeSelf\n\
                 \\forall a : \\mathfrak{{E}}, a \\wedge a = \\mathbb{{1}}\n\
                 Proof:\n\
                 a \\wedge a\n\
                 \x20 = \\mathbb{{1}} \\Delta (a \\Delta a)    [D.X.01]\n\
                 \x20 = \\mathbb{{1}} \\Delta \\varnothing     [{axiom_id}.3]\n\
                 \x20 = \\mathbb{{1}}                     [{cite}]\n"
            ),
        )
        .unwrap();
        root
    }

    /// A citation names the number the AUTHOR wrote, not the position the file
    /// happens to occupy in the walk. A corpus whose axioms are numbered with a
    /// gap (`A4` is the only axiom) must still resolve `[A4.2]`, the two sides
    /// must both verify, and — the half that used to fail silently — a citation
    /// to a number NO node claims must NOT resolve to whichever axiom happens to
    /// sit at that position. Before this, `[A3.2]` bound to the third axiom in
    /// load order regardless of what it was called.
    #[test]
    fn citation_binds_to_the_authored_number_not_the_walk_order() {
        if std::process::Command::new("lean")
            .arg("--version")
            .output()
            .is_err()
        {
            return; // no lean in this environment
        }

        // (a) the number the author wrote resolves, gap and all.
        let root = byo_root("gap", "A4", "A4.2");
        let mut conn = qubitick_db::open_in_memory().unwrap();
        crate::ingest_corpus(&mut conn, &root).unwrap();
        let ground = crate::ground::for_check(&root).unwrap();
        let results = run_pipeline(&conn, &crate::walk_qbit(&root), Some(&ground)).unwrap();
        assert_eq!(results.len(), 6, "three nodes plus the three declarations they use");
        for r in &results {
            assert!(
                r.verified,
                "{} cites the authored number and must verify: {:?}",
                r.path,
                r.diagnostics.first()
            );
        }
        let _ = std::fs::remove_dir_all(&root);

        // (b) a number nobody claims is an error, not a near-miss.
        let root = byo_root("miss", "A4", "A3.2");
        let mut conn = qubitick_db::open_in_memory().unwrap();
        crate::ingest_corpus(&mut conn, &root).unwrap();
        let ground = crate::ground::for_check(&root).unwrap();
        let results = run_pipeline(&conn, &crate::walk_qbit(&root), Some(&ground)).unwrap();
        let theorem = results
            .iter()
            .find(|r| r.path.contains("T_01_Self.qbit"))
            .expect("the theorem must be built");
        assert!(
            !theorem.verified,
            "citing [A3.2] must not bind to the axiom at position 3 — the corpus has no A3"
        );
        let diagnostic = format!("{:?}", theorem.diagnostics.first());
        assert!(
            diagnostic.contains("A3"),
            "the diagnostic must name the citation that failed: {diagnostic}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Two nodes of one kind in one scope cannot share a number or a name: both
    /// make a reference ambiguous, so the corpus reports them instead of letting
    /// one silently answer for the other.
    #[test]
    fn duplicate_numbers_and_names_are_reported() {
        let root = std::env::temp_dir().join(format!("qbtick_dup_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for d in ["Axioms", "Theorems"] {
            std::fs::create_dir_all(root.join("Era0/Strata0").join(d)).unwrap();
        }
        // two axioms sharing a NUMBER but not a name
        for (file, name) in [("A1_Base.qbit", "Base"), ("A1_Again.qbit", "Other")] {
            std::fs::write(
                root.join("Era0/Strata0/Axioms").join(file),
                format!("A1. {name}\n\\forall a : \\mathfrak{{E}}, a \\Delta a = \\varnothing\n"),
            )
            .unwrap();
        }
        // two theorems, distinct numbers, same name
        std::fs::write(
            root.join("Era0/Strata0/Theorems/T_01_X.qbit"),
            "T.X.01. Twice\n\\forall a : \\mathfrak{E}, a = a\nProof:\na\n  = a    [\\equiv]\n",
        )
        .unwrap();
        std::fs::write(
            root.join("Era0/Strata0/Theorems/T_02_X.qbit"),
            "T.X.02. Twice\n\\forall a : \\mathfrak{E}, a = a\nProof:\na\n  = a    [\\equiv]\n",
        )
        .unwrap();

        let mut conn = qubitick_db::open_in_memory().unwrap();
        let report = crate::ingest_corpus(&mut conn, &root).unwrap();
        let claim = |c: crate::Claimed| -> Vec<String> {
            report
                .duplicates
                .iter()
                .filter(|d| d.claimed == c)
                .map(|d| d.message())
                .collect()
        };
        let numbers = claim(crate::Claimed::Number);
        assert_eq!(numbers.len(), 1, "one duplicate number: {numbers:?}");
        assert!(numbers[0].contains("Axiom"), "{}", numbers[0]);
        assert!(numbers[0].contains("A1_Base.qbit") && numbers[0].contains("A1_Again.qbit"), "{}", numbers[0]);

        let names = claim(crate::Claimed::Name);
        assert_eq!(names.len(), 1, "one duplicate name: {names:?}");
        assert!(names[0].contains("Theorem") && names[0].contains("Twice"), "{}", names[0]);

        assert!(!report.is_clean());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Two visible definitions can claim one division number (a nearer stratum
    /// restating `D.01`); the nearer one answers, and only one does.
    #[test]
    fn a_shadowed_definition_resolves_to_the_nearer_stratum() {
        let root = std::env::temp_dir().join(format!("qbtick_shadow_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for d in ["Era0/Strata0/Definitions", "Era0/Strata1/Definitions"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        std::fs::write(
            root.join("Era0/Strata0/Definitions/D_BT_01_Far.qbit"),
            "D.BT.01. Far\na \\wedge b\nSema:\na \\wedge b = a\n",
        )
        .unwrap();
        std::fs::write(
            root.join("Era0/Strata1/Definitions/D_BT_01_Near.qbit"),
            "D.BT.01. Near\na \\vee b\nSema:\na \\vee b = a\n",
        )
        .unwrap();

        let mut conn = qubitick_db::open_in_memory().unwrap();
        crate::ingest_corpus(&mut conn, &root).unwrap();
        let ctx = qubitick_ctx::build_ctx(&conn, "Era0/Strata1").unwrap();
        let defs = crate::axioms::load_definition_nodes(&conn, &ctx, "Era0/Strata1").unwrap();
        let d1: Vec<_> = defs.iter().filter(|d| d.division == 1).collect();
        assert_eq!(d1.len(), 1, "exactly one definition answers for D.01");
        assert_eq!(
            d1[0].node.base().name,
            "Near",
            "the nearer stratum's definition wins, not whichever loaded last"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn empty_pipeline_is_empty() {
        let conn = qubitick_db::open_in_memory().unwrap();
        let results = run_pipeline(&conn, &[], None).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn emits_bare_prelude_module_with_type_frame_and_no_latex_leak() {
        // Locks in the emission-correctness work: the module is a bare-`prelude`
        // module, declares a `Prop` type frame for referenced glyphs, renders
        // equality as `Eq`, and never leaks raw LaTeX into the emitted Lean.
        use qubitick_db::queries::SymbolRow;
        let row = |id: i64, tex: &str| SymbolRow { facets: 0,
            id,
            scope: "Era0/Strata0".into(),
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
        let ctx = Ctx::from_rows(
            "Era0/Strata0",
            vec![
                row(1, "\\mathfrak{E}"),
                declaring(row(2, "\\Delta"), &[qubitick_model::corpus::Facet::Nullity]),
                declaring(row(3, "\\cap"), &[qubitick_model::corpus::Facet::BinOp]),
                declaring(row(4, "\\varnothing"), &[qubitick_model::corpus::Facet::Null]),
            ],
        );
        let src = "A1. Realm\n\\forall a b : \\mathfrak{E}, a \\Delta b = b \\Delta a";
        let nodes = qubitick_parse::parse_content(&ctx, src).unwrap();
        let out = qubitick_emit::emit_lean_text(&ctx, &nodes, "Qubitick.HL", &mut qubitick_rewrite::rules::RuleTable::new()).unwrap();

        assert!(
            out.starts_with("prelude\nimport Qubitick.HL"),
            "got:\n{out}"
        );
        assert!(
            out.contains("axiom Delta : Prop → Prop → Prop"),
            "got:\n{out}"
        );
        assert!(out.contains("def mathfrak_E : Type := Prop"), "got:\n{out}");
        assert!(out.contains("Eq (Delta a b) (Delta b a)"), "got:\n{out}");
        // The original bug: raw ASCII-LaTeX leaking into Lean.
        assert!(!out.contains("\\Delta"), "raw LaTeX leaked:\n{out}");
        assert!(!out.contains("\\forall"), "raw LaTeX leaked:\n{out}");
    }
}
