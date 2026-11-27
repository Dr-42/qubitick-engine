//! Emit the corpus as ONE Lake package.
//!
//! Checking the corpus currently costs 329 `lean` invocations, each re-reading its
//! own context — and the CLI shape makes it worse, because every `qubitick emit`
//! re-ingests the whole corpus first. A Lake package inverts that: modules are
//! written once, `lake build` checks them with the parallelism and the olean cache
//! Lean already has, and a second run only re-checks what changed.
//!
//! What it does NOT do yet: replace the in-module "assumed lemma" declarations with
//! `import`s of the cited nodes' modules. That is the change that would make Lean
//! itself reject a dependency cycle (and check a theorem against the real proof of
//! what it cites); it changes every emitted module, so it is deliberately the next
//! step, not this one. Until then each module is exactly what the per-file path
//! emits, which is what makes the two paths comparable.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use qubitick_ctx::Ctx;

use crate::pipeline::{EmitCache, emit_file_cached};

/// What the package build produced.
#[derive(Debug, Clone)]
pub struct LakeReport {
    /// Modules written into the package.
    pub modules: usize,
    /// The package directory (`.lake/` under the corpus root).
    pub package: PathBuf,
    /// True when `lake build` exited zero.
    pub built: bool,
    /// Lake's own error lines, when it did not.
    pub failures: Vec<String>,
}

/// Where the package lives for a corpus: a cache next to the DB, so a rebuild is
/// incremental and `git status` stays clean (`project/.lake/` is ignored).
pub fn package_dir(corpus: &Path) -> PathBuf {
    corpus.join(".lake")
}

/// A node file → its Lean module name.
///
/// `project/Era0/Strata0/Theorems/T_BT_01_DeltaComm.qbit` becomes
/// `Qubitick.Corpus.Era0.Strata0.Theorems.T_BT_01_DeltaComm`. The stem is
/// sanitised because corpus filenames are not identifiers (`T_QB_106_…Cancel.`
/// ends in a dot), and a leading digit is prefixed for the same reason.
pub fn module_name(corpus: &Path, file: &Path) -> String {
    let rel = file.strip_prefix(corpus).unwrap_or(file);
    let mut parts: Vec<String> = Vec::new();
    for part in rel.components() {
        let text = part.as_os_str().to_string_lossy().to_string();
        if text.is_empty() {
            continue;
        }
        let stem = text.strip_suffix(".qbit").unwrap_or(&text);
        parts.push(sanitise(stem));
    }
    format!("Qubitick.Corpus.{}", parts.join("."))
}

/// A node file → its Lean module name, from the path ALONE.
///
/// `module_name` needs the corpus root to strip; this does not, because an ingest
/// records the path a file was walked with (absolute on one caller, corpus-relative on
/// another) and the corpus prefix is not what makes a module importable. What does is
/// the era tree: everything from the first `Era<N>` component is the module's place.
pub fn module_name_of(filename: &str) -> Option<String> {
    let parts: Vec<&str> = filename
        .split(['/', '\\'])
        .filter(|p| !p.is_empty())
        .collect();
    let start = parts.iter().position(|p| {
        p.strip_prefix("Era").is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    })?;
    let mut names: Vec<String> = Vec::new();
    for part in &parts[start..] {
        let stem = part.strip_suffix(".qbit").unwrap_or(part);
        names.push(sanitise(stem));
    }
    Some(format!("Qubitick.Corpus.{}", names.join(".")))
}

fn sanitise(text: &str) -> String {
    let mut out: String = text
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

/// Write the package: the prelude, one module per node, and the library root.
///
/// Returns the modules written, paired with the corpus file each came from, so a
/// build failure can be reported against the `.qbit` a human wrote.
/// How the corpus is laid out inside the package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// One module per node: each is checked in its own environment (today's
    /// per-file semantics), which is why the shared frame never collides.
    PerNode,
    /// One module for the whole corpus. Kept as a target shape: it needs the same
    /// curated context `Meshed` needs, and no CLI flag selects it yet.
    Single,
    /// A shared `Qubitick.Frame` holding the ground, and one module per node that
    /// imports it plus the modules of the theorems it cites. One environment,
    /// nothing declared twice, and Lake's import graph is the dependency order.
    Meshed,
}

pub fn write_package(
    ctx: &Ctx,
    conn: &qubitick_db::Db,
    corpus: &Path,
    out: &Path,
    shape: Shape,
) -> Result<Vec<(String, String)>> {
    let files = crate::walk_qbit(corpus);
    let src = out.join("src");
    let canon = crate::canon::read(corpus)?;
    // The base ground comes from the engine (env, checkout, or the copy inside the binary) —
    // a corpus in its own repository has no `prelude/` above it to find. The project's own
    // ground, when the charter names one, is laid OVER that base: the base provides the
    // contract, the project provides everything above it. Both rules live in
    // `crate::ground::assemble`, which is where they are tested.
    // Source only (Lake builds the oleans itself, so a stale `.olean` from `lean_check` must
    // not ride along).
    let _base = crate::ground::assemble(corpus, canon.as_ref(), &src)?;

    // The ground must satisfy the emitter's contract before anything is built on it:
    // that is the whole restriction on a project's own `HL.lean`. Both package shapes
    // share the ground, so the check is here rather than in either of them.
    crate::ground::check(&src, canon.as_ref())?;

    let mut cache = EmitCache::default();
    let mut modules: Vec<(String, String)> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    if shape == Shape::Meshed {
        return write_meshed(ctx, conn, corpus, out, &files, &src);
    }

    for file in &files {
        let mut module = module_name(corpus, file);
        // Sanitising can collide (two stems differing only in punctuation); a
        // suffix keeps both buildable and the collision visible in the name.
        let mut n = 2;
        while !seen.insert(module.clone()) {
            module = format!("{}_{}", module_name(corpus, file), n);
            n += 1;
        }
        let lean = emit_file_cached(ctx, conn, file, &mut cache)
            .with_context(|| format!("emit {}", file.display()))?;
        let rel = module.strip_prefix("Qubitick.").unwrap_or(&module).replace('.', "/");
        let target = src.join("Qubitick").join(format!("{rel}.lean"));
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        write_if_changed(&target, &lean)
            .with_context(|| format!("write {}", target.display()))?;
        modules.push((module, file.display().to_string()));
    }

    // The library root imports the PRELUDE only. Each node module is its own
    // target: `globs` makes `lake build` check them all, in parallel, with an olean
    // cache — which is today's per-file semantics (one module, one environment)
    // rather than one shared environment.
    //
    // One shared environment is what a package "should" be, and it does not work
    // yet: every module re-declares the shared glyph frame in `Qubitick.HL`, so
    // importing two of them fails with "environment already contains
    // 'Qubitick.HL.mathbb_R'". Hoisting that frame into one generated module is the
    // prerequisite for imports-as-dependency-graph, and it is the next step; until
    // then this package is a faster way to check the same thing, not a different
    // way to check it.
    std::fs::write(src.join("Qubitick.lean"), "prelude\nimport Qubitick.HL\n")?;
    // The glob `Qubitick.Corpus.*` names a prefix, and Lake insists the prefix
    // module itself exist; an empty one is enough.
    std::fs::write(src.join("Qubitick").join("Corpus.lean"), "prelude\n")?;

    std::fs::write(
        out.join("lakefile.toml"),
        "name = \"corpus\"\nversion = \"0.1.0\"\nsrcDir = \"src\"\n\n\
         [[lean_lib]]\nname = \"Qubitick\"\nglobs = [\"Qubitick\", \"Qubitick.Corpus.*\"]\n",
    )?;
    // A lean-toolchain file would send Lake looking for elan; the `toolchain`
    // wrapper already puts the corpus's own 4.35.0 `lake` on PATH, so Lake uses
    // the compiler it was built with.
    Ok(modules)
}

/// Run `lake build` in the package directory and report what happened.
pub fn build(out: &Path) -> Result<LakeReport> {
    let modules = out.join("src").join("Qubitick");
    let _ = modules;
    let run = std::process::Command::new("lake")
        .arg("build")
        .arg("Qubitick")
        .current_dir(out)
        .output()
        .context("failed to spawn `lake build` — is `lake` on PATH (run through ./toolchain)?")?;
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();
    let mut failures: Vec<String> = Vec::new();
    for line in stdout.lines().chain(stderr.lines()) {
        let line = line.trim();
        if line.starts_with("error") || line.starts_with('✖') {
            failures.push(line.to_string());
        }
    }
    Ok(LakeReport {
        modules: count_lean(&out.join("src")),
        package: out.to_path_buf(),
        built: run.status.success(),
        failures,
    })
}

/// Write only when the bytes differ.
///
/// Lake decides what to rebuild from timestamps, so rewriting an unchanged module
/// would make every run a full rebuild — the opposite of the point.
fn write_if_changed(path: &Path, content: &str) -> Result<()> {
    if let Ok(existing) = std::fs::read(path)
        && existing == content.as_bytes() {
            return Ok(());
        }
    std::fs::write(path, content)?;
    Ok(())
}

/// A corpus file, parsed: what it is, which scope it belongs to, and the number it is cited by.
///
/// It HAS a path (the caller still holds it) and this struct does not carry it: nothing here reads
/// one — the two failures below name a division (`T.37`) or a cycle, not a file — and a field the
/// compiler reports as "never read" is a field that is only pretending to be data.
struct Unit {
    scope: String,
    /// The division a citation names (`D.37` → 37). 0 when the stem has no number.
    division: u32,
    nodes: Vec<qubitick_model::Node>,
}

/// The scope (`EraN/StrataM`) a corpus file belongs to, from its path.
fn scope_of(file: &Path, corpus: &Path) -> String {
    let rel = file.strip_prefix(corpus).unwrap_or(file);
    let mut parts = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string());
    match (parts.next(), parts.next()) {
        (Some(era), Some(strata)) => format!("{era}/{strata}"),
        _ => "Era0/Strata0".to_string(),
    }
}

/// The meshed package: one module per ERA.
///
/// The era is the composable unit because it is the vocabulary unit — an era's glyphs
/// and its axiom class chain belong to it, so two eras share nothing that could
/// collide. One module per NODE needed every module to agree with a shared frame about
/// how a glyph splits into its bare and `_fn` forms, and those splits are derived per
/// context, which is fine when a module is checked alone and wrong when all must agree.
///
/// Inside a module, nodes are emitted in citation order: Lean requires a declaration
/// before its use, and a theorem that cites another must come after it. That order is
/// the dependency DAG, computed here and enforced by the compiler — which also means a
/// theorem citing itself, directly or through others, is a build error rather than an
/// assumption.
fn write_meshed(
    ctx: &Ctx,
    conn: &qubitick_db::Db,
    corpus: &Path,
    out: &Path,
    files: &[PathBuf],
    src: &Path,
) -> Result<Vec<(String, String)>> {
    let cycles = crate::cycles::definition_cycles(conn, corpus)?;
    if !cycles.is_empty() {
        let mut message = String::from("the corpus has definitions that define each other:");
        for cycle in &cycles {
            message.push_str("\n  ");
            message.push_str(&cycle.message());
        }
        anyhow::bail!(message);
    }

    // ── parse every file once, in its own scope ──────────────────────────────
    let mut units: Vec<Unit> = Vec::new();
    for file in files {
        let scope = scope_of(file, corpus);
        let owned = if scope == ctx.scope() {
            None
        } else {
            Some(qubitick_ctx::build_ctx(conn, &scope)?)
        };
        let fctx = owned.as_ref().unwrap_or(ctx);
        let text = std::fs::read_to_string(file)?;
        let nodes = qubitick_parse::parse_content(fctx, &text)?;
        let division =
            crate::axioms::division_of_filename(&file.display().to_string()).unwrap_or(0);
        units.push(Unit {
            scope,
            division,
            nodes,
        });
    }

    // ── one module per era, nodes in citation order ──────────────────────────
    // A context is per SCOPE, so a module must be: `Era0/Strata0` and `Era0/Strata1`
    // are different vocabularies even inside one era. Scopes are totally ordered
    // (era, then strata) and citations only ever point backwards, which is what makes
    // scope modules importable and the import graph acyclic by construction.
    let mut scopes: std::collections::BTreeMap<String, Vec<usize>> = std::collections::BTreeMap::new();
    for (i, unit) in units.iter().enumerate() {
        scopes.entry(unit.scope.clone()).or_default().push(i);
    }
    // Where each division and each glyph is defined, for the imports below.
    //
    // A blind last-wins map is not enough, and getting this wrong is not subtle: a
    // later scope can resolve the SAME symbol id as an earlier one (its context
    // sees the earlier scope's rows, so its own parse reuses them), and the map
    // then claims the later scope owns the glyph — which pointed an earlier
    // module's import at a later one and closed an import CYCLE. Ownership is
    // therefore asked per citing scope, and only ever answers with a scope the
    // citing scope can actually see.
    let mut defined_divisions: std::collections::HashMap<u32, Vec<usize>> =
        std::collections::HashMap::new();
    let mut defined_glyphs: std::collections::HashMap<u32, Vec<usize>> =
        std::collections::HashMap::new();
    for (i, unit) in units.iter().enumerate() {
        for node in &unit.nodes {
            if let qubitick_model::Node::Theorem(_) = node {
                defined_divisions.entry(unit.division).or_default().push(i);
            }
            if let qubitick_model::Node::Definition(def) = node {
                defined_glyphs.entry(def.symbol.primary).or_default().push(i);
            }
        }
    }

    // ── the ground: one module per ERA, declared once and imported ───────────
    //
    // The symbol frame (every glyph the corpus uses, at the arity it is observed
    // with) and the axiom class chain are declared ONCE. They cannot be repeated
    // per module: two modules in one Lean environment cannot both declare
    // `Qubitick.HL.cap`, and a scope module imports the scopes it cites.
    //
    // It is per ERA because an era's vocabulary is its own: Era1 writes `\le` for a
    // symbol that is not Era0's, and a single frame would have to choose.
    let mut era_units: std::collections::BTreeMap<String, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (i, unit) in units.iter().enumerate() {
        era_units.entry(era_of(&unit.scope)).or_default().push(i);
    }
    let mut written: Vec<(String, String)> = Vec::new();
    for (era, members) in &era_units {
        let mut nodes: Vec<qubitick_model::Node> = Vec::new();
        for i in members {
            nodes.extend(units[*i].nodes.iter().cloned());
        }
        // The era's LAST scope sees every symbol of the era, which is what the
        // frame must be able to name.
        let last = members
            .iter()
            .copied()
            .max_by_key(|i| scope_rank(&units[*i].scope))
            .unwrap_or(members[0]);
        let owned = if units[last].scope == ctx.scope() {
            None
        } else {
            Some(qubitick_ctx::build_ctx(conn, &units[last].scope)?)
        };
        let ground_ctx = owned.as_ref().unwrap_or(ctx);
        let module = era_ground_module(era);
        let text = qubitick_emit::emit_ground(ground_ctx, &nodes, "Qubitick.HL")
            .with_context(|| format!("emit ground for {era}"))?;
        let rel = module.strip_prefix("Qubitick.").unwrap_or(&module).replace('.', "/");
        let target = src.join("Qubitick").join(format!("{rel}.lean"));
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        write_if_changed(&target, &text)?;
        written.push((module, format!("{era}: ground, {} files", members.len())));
    }

    // ── node id -> Lean module name, in the MESHED shape ──────────────────────
    //
    // The emitter needs this map to resolve a glyph a module names but does not
    // declare (`Frame::needed`). It CANNOT be rebuilt from the parsed nodes this
    // path holds: a symbol's `home_node` is a DATABASE node id (`times` is 4 on
    // the algebra corpus) while `parse_content` numbers nodes per file (0, 1, 2),
    // so a parsed index would miss every home. The map is therefore assembled once
    // from the corpus's own node list (`get_node_list`: id -> filename) keyed by
    // the DB id the DB assigned at ingest.
    //
    // A definition's home in the mesh is the ERA GROUND module (one per era), not
    // its per-file module; a theorem's home is its scope module; anything else gets
    // no entry (the emitter then finds no home and reports the glyph).
    let mut home_modules: std::collections::HashMap<i64, String> =
        std::collections::HashMap::new();
    for header in qubitick_db::read::get_node_list(conn)? {
        let scope = scope_of(Path::new(&header.filename), corpus);
        let module = match header.node_type {
            qubitick_model::NodeKind::Definition => era_ground_module(&era_of(&scope)),
            qubitick_model::NodeKind::Theorem => module_of_scope(&scope),
            _ => continue,
        };
        home_modules.insert(header.id, module);
    }

    for (scope_id, members) in &scopes {
        // The nearest unit visible from `scope_id` among `cands` — the module that
        // must be imported for a citation or a glyph to resolve.
        let nearest_visible = |cands: &[usize]| -> Option<usize> {
            cands
                .iter()
                .copied()
                .filter(|i| {
                    qubitick_ctx::scope::scope_visible(&units[*i].scope, scope_id)
                })
                .max_by_key(|i| scope_rank(&units[*i].scope))
        };
        let owner_of_division = |d: u32| -> Option<usize> {
            nearest_visible(defined_divisions.get(&d).map(|v| v.as_slice()).unwrap_or(&[]))
        };
        let owner_of_glyph = |sym: u32| -> Option<usize> {
            nearest_visible(defined_glyphs.get(&sym).map(|v| v.as_slice()).unwrap_or(&[]))
        };
        let owned = if *scope_id == ctx.scope() {
            None
        } else {
            Some(qubitick_ctx::build_ctx(conn, scope_id)?)
        };
        let era_ctx = owned.as_ref().unwrap_or(ctx);
        // which earlier scopes this one needs
        let mut imports_of_scope: Vec<String> = Vec::new();
        for &i in members {
            for d in crate::axioms::cited_divisions(&units[i].nodes) {
                if let Some(owner) = owner_of_division(d)
                    && units[owner].scope != *scope_id
                {
                    imports_of_scope.push(module_of_scope(&units[owner].scope));
                }
            }
            let mut used: std::collections::HashSet<u32> = std::collections::HashSet::new();
            for node in &units[i].nodes {
                crate::cycles::symbols_of(node, &mut used);
            }
            for sym in &used {
                if let Some(owner) = owner_of_glyph(*sym)
                    && units[owner].scope != *scope_id
                {
                    imports_of_scope.push(module_of_scope(&units[owner].scope));
                }
            }
        }
        imports_of_scope.sort();
        imports_of_scope.dedup();
        imports_of_scope.retain(|m| m != &module_of_scope(scope_id));

        let order = citation_order(&units, members)?;

        // The indices the emitter needs.
        //
        // DEFINITIONS are declared in the ground, so this module declares none of
        // them — but a row that cites `[D.n]`, or that unfolds a glyph by kernel
        // conversion, still needs the rule registered under the division it names.
        // The nearest visible scope answers first, which is what makes a division
        // number mean one thing here: an era's own definition wins over the one it
        // inherits.
        let mut def_index: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        let mut thm_index: std::collections::HashMap<u32, (String, qubitick_model::Expr)> =
            std::collections::HashMap::new();
        let mut rules = qubitick_rewrite::RuleTable::new();
        let mut owners: Vec<usize> = (0..units.len())
            .filter(|j| qubitick_ctx::scope::scope_visible(&units[*j].scope, scope_id))
            .collect();
        owners.sort_by_key(|j| std::cmp::Reverse(scope_rank(&units[*j].scope)));
        for j in owners {
            let unit = &units[j];
            let own = unit.scope == *scope_id;
            for node in &unit.nodes {
                match node {
                    qubitick_model::Node::Definition(def) => {
                        if def_index.contains_key(&def.symbol.primary) {
                            continue;
                        }
                        // The INDEX is unconditional: it is what tells the emitter
                        // this node's glyph is a definition with a citable
                        // unfolding, and a glyph that has one must not also be
                        // declared as a frame axiom.
                        def_index.insert(def.symbol.primary, unit.division);
                        if let Some(qubitick_model::FormBody::MacroExpr(sema)) = &def.d_block.form
                            && let Ok(expr) =
                                qubitick_parse::semantic::parse_expr(&format!("{sema:?}"), era_ctx)
                        {
                            rules.insert_definition(unit.division, expr);
                        }
                    }
                    // Only this scope's own theorems are declared here; the rest are
                    // imported, and the loop below registers them as citations.
                    qubitick_model::Node::Theorem(theorem) if own => {
                        thm_index.insert(
                            unit.division,
                            (
                                qubitick_emit::lemma_name(&theorem.base.name),
                                theorem.statement.clone(),
                            ),
                        );
                    }
                    _ => {}
                }
            }
        }
        // A member may cite a theorem that lives in an EARLIER scope. That theorem
        // is not declared here — the module that owns it emits it as a real `def`
        // and this one imports it — but the rule that APPLIES the citation must
        // still be registered, or every row justified by it fails as `not in this
        // module` even though the import is present and correct.
        for &i in members {
            for div in crate::axioms::cited_divisions(&units[i].nodes) {
                if thm_index.contains_key(&div) {
                    continue;
                }
                if let Some(t) = crate::axioms::load_theorem(conn, era_ctx, scope_id, div)? {
                    thm_index.insert(div, (qubitick_emit::lemma_name(&t.name), t.statement));
                }
            }
        }
        // What this module DECLARES: its own nodes, except the definitions — the
        // ground declares those once for the whole era.
        let mut era_nodes: Vec<qubitick_model::Node> = Vec::new();
        for i in &order {
            era_nodes.extend(units[*i].nodes.iter().cloned());
        }
        let owned_nodes: Vec<qubitick_model::Node> = era_nodes
            .iter()
            .filter(|n| !matches!(n, qubitick_model::Node::Definition(_)))
            .cloned()
            .collect();
        // Everything an EARLIER scope declares, plus this scope's own definitions.
        // It is never emitted here — the module that owns each node emits it, and
        // the definitions live in the ground — but the emitter still has to SEE all
        // of it: the class chain comes from the era's axioms, a cited axiom's rule
        // is registered from its statement, a theorem's `section variable {R : …}`
        // is decided by which class its requirement belongs to, and a relational
        // statement unfolds against the `Sema:` of the definition that introduces
        // the relation. Without that last one a Strata0 row failed with
        // `h (hypothesis; does not match)` — the hypothesis was still the raw
        // `\le_{\tau} a b` the statement writes, never the intersection identity
        // the proof reasons in.
        let mut context: Vec<qubitick_model::Node> = Vec::new();
        for unit in units.iter().filter(|u| {
            u.scope != *scope_id && qubitick_ctx::scope::scope_visible(&u.scope, scope_id)
        }) {
            context.extend(unit.nodes.iter().cloned());
        }
        context.extend(
            era_nodes
                .iter()
                .filter(|n| matches!(n, qubitick_model::Node::Definition(_)))
                .cloned(),
        );
        let ground = era_ground_module(&era_of(scope_id));
        let era_has_axioms = units.iter().any(|u| {
            era_of(&u.scope) == era_of(scope_id)
                && u.nodes
                    .iter()
                    .any(|n| matches!(n, qubitick_model::Node::Axiom(_)))
        });
        let mut extra = imports_of_scope.clone();
        extra.push(ground.clone());
        // One import per module: the ground can also arrive from the scope's own
        // cited/used sets.
        extra.sort();
        extra.dedup();
        let text = qubitick_emit::emit_lean_text_meshed(
            era_ctx,
            &context,
            &owned_nodes,
            "Qubitick.HL",
            &def_index,
            &thm_index,
            &mut rules,
            &ground,
            extra,
            // A cited theorem of an earlier scope is IMPORTED here, never assumed:
            // the module that proves it declares it, and an `axiom` of the same
            // name could not coexist with that `def`.
            false,
            // …and the ground declares a class chain only if the era HAS axioms
            // (Era1's live in Era0, which it cannot see, so its scopes hypothesize
            // nothing and cite no axiom field).
            era_has_axioms,
            &home_modules,
        )
        .with_context(|| format!("emit scope {scope_id}"))?;
        let module = module_of_scope(scope_id);
        let rel = module.strip_prefix("Qubitick.").unwrap_or(&module).replace('.', "/");
        let target = src.join("Qubitick").join(format!("{rel}.lean"));
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        write_if_changed(&target, &text)?;
        written.push((module.clone(), format!("{scope_id}: {} files", members.len())));
    }

    // ── the library: the ground, then each era module as its own target ──────
    write_if_changed(&src.join("Qubitick.lean"), "prelude\nimport Qubitick.HL\n")?;
    // the prefix the era modules live under
    write_if_changed(&src.join("Qubitick").join("Corpus.lean"), "prelude\n")?;
    let mut lakefile = String::from("name = \"corpus\"\nversion = \"0.1.0\"\nsrcDir = \"src\"\n");
    if let Some(canon) = crate::canon::read(corpus)? {
        for requirement in &canon.requires {
            lakefile.push_str(&format!("\n[[require]]\nname = \"{requirement}\"\n"));
        }
    }
    lakefile.push_str(
        "\n[[lean_lib]]\nname = \"Qubitick\"\nglobs = [\"Qubitick\", \"Qubitick.Corpus.*\"]\n",
    );
    std::fs::write(out.join("lakefile.toml"), lakefile)?;
    Ok(written)
}

/// The ERA part of a scope: `Era0/Strata1` → `Era0`.
fn era_of(scope: &str) -> String {
    scope.split('/').next().unwrap_or(scope).to_string()
}

/// The module that carries an era's ground: `Qubitick.Corpus.Era0.Ground`.
fn era_ground_module(era: &str) -> String {
    format!("Qubitick.Corpus.{era}.Ground")
}

/// The module a scope becomes: `Era0/Strata0` -> `Qubitick.Corpus.Era0.Strata0`.
/// Where a scope sits in the corpus's total order: era first, then strata.
///
/// Scopes are compared only to pick the NEAREST visible owner of a glyph or a
/// division, and visibility itself is what keeps that owner behind the citing
/// scope — so this rank never has to decide an import, only which of several
/// visible owners answers first.
fn scope_rank(scope: &str) -> (u32, u32) {
    let mut it = scope.split('/');
    let era = it
        .next()
        .and_then(|e| e.trim_start_matches("Era").parse::<u32>().ok())
        .unwrap_or(0);
    let strata = it
        .next()
        .and_then(|s| s.trim_start_matches("Strata").parse::<u32>().ok())
        .unwrap_or(0);
    (era, strata)
}

fn module_of_scope(scope: &str) -> String {
    let parts: Vec<String> = scope
        .split('/')
        .map(|p| p.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect())
        .collect();
    format!("Qubitick.Corpus.{}", parts.join("."))
}

/// Order one era's units so a citation always comes after what it cites.
///
/// Non-theorems first (definitions and axioms declare the vocabulary the theorems use),
/// then theorems by their citation graph. A cycle among theorems is reported: a proof
/// that needs a proof that needs it is not an assumption to make, it is a defect.
fn citation_order(units: &[Unit], members: &[usize]) -> Result<Vec<usize>> {
    let mut by_division: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for &i in members {
        if units[i]
            .nodes
            .iter()
            .any(|n| matches!(n, qubitick_model::Node::Theorem(_)))
        {
            by_division.insert(units[i].division, i);
        }
    }
    // edges: cited -> citing
    let mut indegree: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    let mut next: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    for &i in members {
        indegree.entry(i).or_insert(0);
    }
    for &i in members {
        let Some(&citing) = by_division.get(&units[i].division) else {
            continue;
        };
        for cited in crate::axioms::cited_divisions(&units[i].nodes) {
            if let Some(&from) = by_division.get(&cited)
                && from != citing
            {
                next.entry(from).or_default().push(citing);
                *indegree.entry(citing).or_insert(0) += 1;
            }
        }
    }
    let mut order: Vec<usize> = Vec::new();
    // vocabulary first, in file order
    for &i in members {
        if !by_division.values().any(|&j| j == i) {
            order.push(i);
        }
    }
    let mut ready: Vec<usize> = by_division
        .values()
        .copied()
        .filter(|i| indegree.get(i).copied().unwrap_or(0) == 0)
        .collect();
    ready.sort_unstable();
    let mut done = 0usize;
    while let Some(i) = ready.pop() {
        order.push(i);
        done += 1;
        if let Some(followers) = next.get(&i).cloned() {
            for f in followers {
                let entry = indegree.entry(f).or_insert(0);
                *entry = entry.saturating_sub(1);
                if *entry == 0 {
                    ready.push(f);
                }
            }
        }
    }
    if done != by_division.len() {
        let stuck: Vec<String> = by_division
            .iter()
            .filter(|(_, i)| indegree.get(i).copied().unwrap_or(0) > 0)
            .map(|(d, _)| format!("T.{d}"))
            .collect();
        anyhow::bail!(
            "these theorems cite each other in a loop, so no declaration order exists: {}",
            stuck.join(", ")
        );
    }
    Ok(order)
}


fn count_lean(dir: &Path) -> usize {
    fn walk(dir: &Path, n: &mut usize) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, n);
            } else if path.to_string_lossy().ends_with(".lean") {
                *n += 1;
            }
        }
    }
    let mut n = 0;
    walk(dir, &mut n);
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_node_file_becomes_a_module_name() {
        let corpus = Path::new("/c/project");
        let file = Path::new("/c/project/Era0/Strata0/Theorems/T_BT_01_DeltaComm.qbit");
        assert_eq!(
            module_name(corpus, file),
            "Qubitick.Corpus.Era0.Strata0.Theorems.T_BT_01_DeltaComm"
        );
    }

    #[test]
    fn a_filename_that_is_not_an_identifier_is_sanitised() {
        let corpus = Path::new("/c/project");
        // the corpus really does contain a stem ending in a dot, and one beginning with a digit
        let odd = Path::new("/c/project/Era0/Strata1/Theorems/T_QB_106_QComplexCancel..qbit");
        assert_eq!(
            module_name(corpus, odd),
            "Qubitick.Corpus.Era0.Strata1.Theorems.T_QB_106_QComplexCancel_"
        );
        // a stem that STARTS with a digit cannot begin an identifier
        let digit = Path::new("/c/project/Era0/Strata1/Definitions/9_odd.qbit");
        assert!(module_name(corpus, digit).ends_with("._9_odd"));
    }
}