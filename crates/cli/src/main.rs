//! `qubitick` — mirrored testing CLI.
//!
//! Every `/api/*` route has a matching command that runs the same engine without
//! HTTP. Commands print machine-parseable JSON so integration tests can diff.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde_json::json;

#[derive(Parser)]
#[command(name = "qubitick")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List files under a project root (mirrors GET /api/files).
    Files { root: PathBuf },
    /// Compute the symbol palette (mirrors GET /api/palette).
    Palette { root: PathBuf },
    /// Run an autocite query (mirrors POST /api/autocite).
    Autocite {
        root: PathBuf,
        lhs: String,
        rhs: String,
    },
    /// The rewrites one cited equation reaches from an expression (mirrors POST /api/steps).
    Steps {
        root: PathBuf,
        expr: String,
        /// How many suggestions to print (default 24).
        #[arg(long, default_value_t = 24)]
        limit: usize,
    },
    /// Check the rows of one file's proofs (mirrors POST /api/rows).
    Rows {
        root: PathBuf,
        /// The `.qbit` file whose proof rows are checked.
        file: PathBuf,
    },
    /// The corpus as a document (mirrors GET /api/pdf).
    Pdf {
        root: PathBuf,
        /// Print the LaTeX document instead of compiling it (compiling needs `tectonic`).
        #[arg(long)]
        latex: bool,
    },
    /// Build the project (mirrors POST /api/build).
    Build {
        root: PathBuf,
        /// Refuse to build unless the project's ground file hashes to this.
        ///
        /// The freeze: a ground is the one file that carries a corpus's assumptions, so
        /// `--frozen` pins it. `qubitick hash-ground` prints the value.
        #[arg(long)]
        frozen: Option<String>,
    },
    /// Print the ground a corpus's A blocks imply — the class chain, as a suggestion.
    ///
    /// The engine writes no `class` itself: this is what a project drops into its own
    /// `HL.lean` under the contract preset, then edits freely.
    Scaffold {
        root: PathBuf,
        /// The namespace to wrap the chain in.
        #[arg(long, default_value = "Qubitick.HL")]
        namespace: String,
        /// Write it to `<root>/Qubitick/HL.lean` instead of printing it.
        #[arg(long)]
        write: bool,
    },
    /// Print the hash of a corpus's ground file, for `build --frozen`.
    HashGround { root: PathBuf },
    /// Seed a corpus's ground from a preset: `init`, `mathlib`, `bare`, or `custom`.
    Seed {
        root: PathBuf,
        #[arg(long, default_value = "init")]
        preset: String,
    },
    /// Check that a project's own `HL.lean` satisfies the emitter's contract.
    ///
    /// Emission stays term mode and names a fixed set of declarations; a project may
    /// put Init, Mathlib or its own development on top of that base, but not below it.
    PreludeCheck {
        /// The Lean module the project's ground is imported as.
        #[arg(long, default_value = "Qubitick.HL")]
        module: String,
        /// LEAN_PATH for the module, when it is not on the default path.
        #[arg(long)]
        lean_path: Option<PathBuf>,
    },
    /// Emit the corpus as ONE Lake package and build it (incremental, parallel).
    Lake {
        root: PathBuf,
        /// Write the package but do not invoke `lake build`.
        #[arg(long)]
        no_build: bool,
        /// Shared ground + imports: one Lean environment, nothing declared twice.
        /// IN PROGRESS — it needs the pipeline's curated per-file context, so today
        /// it reports the first emit failure instead of producing a package.
        #[arg(long)]
        mesh: bool,
    },
    /// Parse a single .qbit file into nodes (mirrors POST /api/parse).
    Parse { file: PathBuf },
    /// Print what Lean says each constant of a file stands on (the axiom footprint).
    Axioms { root: PathBuf, file: PathBuf },
    /// Print the Lean module a single .qbit file emits (debugging aid).
    Emit {
        file: PathBuf,
        /// Build the corpus DB under this root first; omit for a symbol-free CTX.
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Files { root } => {
            let files = qubitick_build::walk_qbit(&root);
            println!("{}", serde_json::to_string_pretty(&json!({ "files": files }))?);
        }
        Commands::Palette { root } => {
            // The route this mirrors answers for the OPEN PROJECT, so the command has
            // to ingest the corpus first: an in-memory DB with no symbols generates an
            // empty palette, which is what this command answered for every corpus.
            let mut db = qubitick_db::open_in_memory()?;
            qubitick_build::ingest_corpus(&mut db, &root)?;
            let entries = qubitick_palette::generate_palette(&db)?;
            let vec: Vec<_> = entries
                .iter()
                .map(|e| {
                    json!({
                        "symbol": e.symbol,
                        "lean": e.lean,
                        "color": e.color,
                        "usage": e.usage,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&json!({ "symbols": vec }))?);
        }
        Commands::Autocite { root, lhs, rhs } => {
            // The query is written in the corpus's OWN notation (`a \cap b`), so it
            // has to be resolved against the corpus's symbols: an empty context
            // parses nothing, which is why this command used to answer
            // "parse failed" for every query, whatever the root.
            let mut db = qubitick_db::open_in_memory()?;
            let root_path = PathBuf::from(&root);
            qubitick_build::ingest_corpus(&mut db, &root_path)?;
            let ctx = qubitick_ctx::build_ctx(&db, &corpus_scope(&root_path))?;
            /* THE SAME RENDERING THE ROUTE USES, called rather than copied: this command's
               whole point is that it answers what `POST /api/autocite` answers, and a chain
               shown in a different notation would be a mirror that lies. */
            let show = |e: &qubitick_model::Expr| qubitick_autocite::show(&ctx, e);
            // The corpus as a rewrite system, so the answer can name citations.
            let rules: Vec<(String, qubitick_rewrite::diff::RewriteRule)> =
                qubitick_build::cite_rules(&ctx, &db)?
                    .into_iter()
                    .map(|c| (c.cite, c.rule))
                    .collect();
            let lhs_e = qubitick_parse::semantic::parse_expr(&lhs, &ctx);
            let rhs_e = qubitick_parse::semantic::parse_expr(&rhs, &ctx);
            match (lhs_e, rhs_e) {
                (Ok(l), Ok(r)) => {
                    let res = qubitick_autocite::find_with_rules(&l, &r, &rules);
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json!({
                            "reached": res.reached,
                            "rules": rules.len(),
                            /* the bridge, each step with the citation that made it. The field is
                               named the way the route names it (`POST /api/autocite`), because
                               this command IS that route with a printer on the end. */
                            "citations": res.steps.iter().map(|s| json!({
                                "expr": show(&s.expr),
                                "cite": s.cite,
                            })).collect::<Vec<_>>(),
                            "closest": res.closest.as_ref().map(show),
                            "gap": res.gap,
                        }))?
                    );
                }
                // Say WHY: the parser knows the span, the glyph and the form it
                // wanted, and the caller cannot fix anything without that.
                (lhs_r, rhs_r) => {
                    let mut problems: Vec<String> = Vec::new();
                    if let Err(e) = lhs_r {
                        problems.push(format!("lhs {e}"));
                    }
                    if let Err(e) = rhs_r {
                        problems.push(format!("rhs {e}"));
                    }
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json!({
                            "reached": false,
                            "error": "expression did not parse",
                            "problems": problems,
                            "hint": "expressions use the corpus's own LaTeX symbols (\\cap, \\Delta, \\varnothing), single-letter metavariables (a, b, c)",
                        }))?
                    );
                }
            }
        }
        Commands::Steps { root, expr, limit } => {
            /* THE ROUTE'S OWN ANSWER, from the route's own function: what can follow this
               expression, each state with the citation that reaches it. */
            let mut db = qubitick_db::open_in_memory()?;
            let root_path = PathBuf::from(&root);
            qubitick_build::ingest_corpus(&mut db, &root_path)?;
            let ctx = qubitick_ctx::build_ctx(&db, &corpus_scope(&root_path))?;
            let show = |e: &qubitick_model::Expr| qubitick_autocite::show(&ctx, e);
            let rules: Vec<(String, qubitick_rewrite::diff::RewriteRule)> =
                qubitick_build::cite_rules(&ctx, &db)?
                    .into_iter()
                    .map(|c| (c.cite, c.rule))
                    .collect();
            match qubitick_parse::semantic::parse_expr(&expr, &ctx) {
                Ok(state) => {
                    let steps = qubitick_autocite::rows::steps(&state, &rules, limit);
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json!({
                            "ok": true,
                            "state": show(&state),
                            "count": steps.len(),
                            "steps": steps.iter().map(|s| json!({
                                "cite": s.cite,
                                "expr": show(&s.expr),
                            })).collect::<Vec<_>>(),
                        }))?
                    );
                }
                Err(e) => println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "ok": false,
                        "error": "expression did not parse",
                        "problem": e.to_string(),
                        "hint": "expressions use the corpus's own LaTeX symbols (\\cap, \\Delta, \\varnothing), single-letter metavariables (a, b, c)",
                    }))?
                ),
            }
        }
        Commands::Rows { root, file } => {
            /* THE ROUTE'S OWN CHECK, on the FILE's text: the route takes a buffer because the
               editor has one; a terminal has the file. */
            let mut db = qubitick_db::open_in_memory()?;
            let root_path = PathBuf::from(&root);
            qubitick_build::ingest_corpus(&mut db, &root_path)?;
            let ctx = qubitick_ctx::build_ctx(&db, &corpus_scope(&root_path))?;
            let show = |e: &qubitick_model::Expr| qubitick_autocite::show(&ctx, e);
            let src = std::fs::read_to_string(&file)?;
            let nodes = qubitick_parse::parse_content(&ctx, &src)?;
            let rows = qubitick_build::check_rows(&db, &ctx, &nodes);
            let refused = rows.iter().filter(|r| r.verdict != "confirmed" && r.verdict != "local").count();
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    /* `ok` is "the check ran"; `unconfirmed` says what it could not confirm —
                       the same two fields the route answers with. */
                    "ok": true,
                    "file": file.display().to_string(),
                    "count": rows.len(),
                    /* the rows this check could not confirm: a check that CONFIRMS is the only
                       kind this engine can honestly offer (crates/build/src/row_check.rs) */
                    "unconfirmed": refused,
                    "rows": rows.iter().map(|r| json!({
                        "row": r.row,
                        "cite": r.cite,
                        "refs": r.refs,
                        "verdict": r.verdict,
                        "from": show(&r.from),
                        "to": show(&r.to),
                        "reaches": r.reaches.iter().map(show).collect::<Vec<_>>(),
                    })).collect::<Vec<_>>(),
                }))?
            );
        }
        Commands::Pdf { root, latex } => {
            /* THE ROUTE'S OWN MATERIAL, printed. `--latex` is here because compiling a PDF needs
               `tectonic`, and a document nobody can look at is a document nobody can check. */
            let mut db = qubitick_db::open_in_memory()?;
            let root_path = PathBuf::from(&root);
            qubitick_build::ingest_corpus(&mut db, &root_path)?;
            let ctx = qubitick_ctx::build_ctx(&db, &corpus_scope(&root_path))?;
            let headers = qubitick_db::read::get_node_list(&db)?;
            let sections = qubitick_build::pdf_sections(&ctx, &headers)?;
            let doc = qubitick_pdf::latex::document_sections(&sections);
            if latex {
                print!("{doc}");
            } else {
                let bytes = qubitick_pdf::tectonic::compile(&doc, &std::env::temp_dir().join("qubitick_pdf"))?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "ok": true,
                        "bytes": bytes.len(),
                        "sections": sections.len(),
                    }))?
                );
            }
        }
        Commands::Build { root, frozen } => {
            let root_path = PathBuf::from(&root);
            if let Some(expected) = &frozen {
                qubitick_build::presets::check_frozen(&root_path, expected)?;
            }
            // Populate the corpus DB + a real symbol context so glyphs resolve and
            // statements parse structurally (not the empty-CTX fallback path).
            let mut db = qubitick_db::open_in_memory()?;
            let report = qubitick_build::ingest_corpus(&mut db, &root_path)?;
            let files: Vec<PathBuf> = qubitick_build::walk_qbit(&root);
            // the ground the corpus is checked against: our base, with the charter's `LeanDir`
            // over it when the project ships its own `HL.lean` (crates/build/src/ground.rs)
            let ground = qubitick_build::ground::for_check(&root_path)?;
            // no context: the pipeline builds each file's own, from the scope its path implies
            let results = qubitick_build::run_pipeline(&db, &files, Some(&ground))?;
            /* THE REPORT A PERSON READS, rendered the way rustc and Lean print: a title,
               an arrow to the position, the source line with the offending run underlined,
               `= help:` underneath, then one summary line. It goes to stderr because stdout
               is the JSON a tool parses — and it is printed whether the build is green or
               red, because a red build is exactly when a person needs it. */
            eprint!(
                "{}",
                qubitick_build::report::render(&results, &qubitick_build::report::DiskSources)
            );
            let verified_files: Vec<&str> = results
                .iter()
                .filter(|r| r.verified)
                .map(|r| r.path.as_str())
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "built": results.iter().filter(|r| r.ok).count(),
                    "verified": results.iter().filter(|r| r.verified).count(),
                    "verified_files": verified_files,
                    "files": results.len(),
                    // Warnings and errors share one shape, and only errors make a
                    // file unverified. A failure lists that file's errors; a warning
                    // carries its own file and line, so it needs no grouping.
                    "failures": results
                        .iter()
                        .filter(|r| !r.verified)
                        .map(|r| json!({
                            "path": r.path,
                            "diagnostics": r
                                .diagnostics
                                .iter()
                                .filter(|d| d.is_error())
                                .map(|d| serde_json::to_value(d).unwrap_or_default())
                                .collect::<Vec<_>>(),
                        }))
                        .collect::<Vec<_>>(),
                    "warnings": warnings_json(&results),
                    "warning_count": qubitick_build::warning_count(&results),
                    // the same number the route reports: theorems stated and not proved
                    "admitted": qubitick_build::admitted_count(&results),
                    "error_count": results.iter().flat_map(|r| &r.diagnostics).filter(|d| d.is_error()).count(),
                    // A reference is only as good as its uniqueness: a number or a
                    // name claimed twice in one scope makes both citations and name
                    // lookups ambiguous, so a build says so rather than picking one.
                    "duplicates": report.duplicates.iter().map(|d| d.message()).collect::<Vec<_>>(),
                    // Definitions that define each other: one of the pair is circular.
                    "cycles": qubitick_build::definition_cycles(&db, &root_path)?
                        .iter()
                        .map(|c| c.message())
                        .collect::<Vec<_>>(),
                }))?
            );
            // A green build compiles the charter into `canon.bit`: what the corpus
            // turned out to be (census + theorem graph + acyclicity), next to the
            // hand-written `Canon.qbit`, which is only ever read. Deterministic, so
            // an unchanged corpus rewrites nothing.
            if results.iter().all(|r| r.verified)
                && let Some(canon) = qubitick_build::read_canon(&root_path)?
            {
                let mut ctxs: std::collections::HashMap<String, qubitick_ctx::Ctx> =
                    std::collections::HashMap::new();
                let mut kinds: std::collections::BTreeMap<String, usize> =
                    std::collections::BTreeMap::new();
                let mut edges: Vec<qubitick_build::Edge> = Vec::new();
                for file in &files {
                    let scope = qubitick_build::pipeline::scope_of_path(file);
                    if !ctxs.contains_key(&scope) {
                        ctxs.insert(scope.clone(), qubitick_ctx::build_ctx(&db, &scope)?);
                    }
                    let text = std::fs::read_to_string(file)?;
                    let nodes = qubitick_parse::parse_content(&ctxs[&scope], &text)?;
                    for node in &nodes {
                        let kind = node.base().kind.as_str().to_string();
                        *kinds.entry(kind).or_insert(0) += 1;
                    }
                    let from = qubitick_build::axioms::division_of_filename(
                        &file.display().to_string(),
                    );
                    if let Some(from) = from
                        && let Some(qubitick_model::Node::Theorem(_)) =
                            nodes.iter().find(|n| matches!(n, qubitick_model::Node::Theorem(_)))
                    {
                        for to in qubitick_build::axioms::cited_divisions(&nodes) {
                            edges.push(qubitick_build::Edge { from, to });
                        }
                    }
                }
                let verified = results.iter().filter(|r| r.verified).count();
                let path = qubitick_build::write_artifact(
                    &root_path, &canon, results.len(), verified, &kinds, &mut edges,
                )?;
                eprintln!("canon: {}", path.display());
            }
            // An ambiguous reference is not a warning: `[A4.2]` naming two axioms,
            // or a name lookup naming two nodes, has no right answer, so the corpus
            // does not build. The JSON above still describes what was found, so a
            // tool can read the detail off stdout while the failure is on stderr.
            let cycles = qubitick_build::definition_cycles(&db, &root_path)?;
            if !cycles.is_empty() {
                for c in &cycles {
                    let mut d = qubitick_model::Diagnostic::error(
                        qubitick_model::Phase::Scope,
                        "corpus.definition-cycle",
                        format!("definitions use each other's glyphs in {}", c.era),
                    );
                    for m in &c.members {
                        d = d.hint(format!(
                            "D.{} {} ({})",
                            m.division, m.name, m.path
                        ));
                    }
                    eprintln!(
                        "{}",
                        d.hint(
                            "one of them is circular: a package cannot import a cycle, so \
                             decide which definition is load-bearing"
                        )
                        .render(&qubitick_build::report::DiskSources)
                    );
                }
                eprintln!(
                    "error: could not build: {} definition cycle(s) found",
                    cycles.len()
                );
                std::process::exit(2);
            }
            if !report.is_clean() {
                for duplicate in &report.duplicates {
                    let d = qubitick_model::Diagnostic::error(
                        qubitick_model::Phase::Store,
                        "corpus.duplicate",
                        duplicate.message(),
                    )
                    .hint(
                        "number each node once per era, and name each one once per scope: \
                         a citation and a name lookup both have to agree on what they mean",
                    );
                    eprintln!("{}", d.render(&qubitick_build::report::DiskSources));
                }
                eprintln!(
                    "error: could not build: {} ambiguous reference(s) found",
                    report.duplicates.len()
                );
                std::process::exit(2);
            }
        }
        Commands::PreludeCheck { module, lean_path } => {
            // Run the contract as Lean sees it, not as we hope it is: a probe module
            // that references every required declaration, checked with the project's
            // own ground in scope.
            // An `import` needs a compiled `.olean`, so the ground is built first —
            // the same step a check of an emitted module performs.
            if let Some(path) = &lean_path {
                qubitick_build::lean_check::ensure_prelude(path)?;
            }
            let dir = std::env::temp_dir().join(format!("qbtick-prelude-{}", std::process::id()));
            std::fs::create_dir_all(&dir)?;
            let probe = dir.join("Contract.lean");
            std::fs::write(&probe, qubitick_build::prelude_contract::probe(&module))?;
            let mut cmd = std::process::Command::new("lean");
            cmd.arg(&probe);
            if let Some(path) = &lean_path {
                cmd.env("LEAN_PATH", path);
            }
            let out = cmd.output()?;
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            let missing = qubitick_build::prelude_contract::missing_from(&text);
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "module": module,
                    "satisfied": missing.is_empty() && out.status.success(),
                    "missing": missing
                        .iter()
                        .map(|n| json!({
                            "name": n,
                            "tier": qubitick_build::prelude_contract::tier_of(n).map(|t| t.label()),
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
            // Anything not satisfied is a failure: a probe that fails for a reason
            // this parser does not name is still a contract the project has not met.
            if !missing.is_empty() || !out.status.success() {
                for line in text.lines().take(6) {
                    eprintln!("{line}");
                }
                std::process::exit(1);
            }
        }
        Commands::Lake { root, no_build, mesh } => {
            // One process, one ingest: the per-file CLI shape re-ingests the whole
            // corpus for every file, which is why it cannot be the way to check it.
            let mut db = qubitick_db::open_in_memory()?;
            let corpus = PathBuf::from(&root);
            qubitick_build::ingest_corpus(&mut db, &corpus)?;
            let ctx = qubitick_ctx::build_ctx(&db, &corpus_scope(&corpus))?;
            let out = qubitick_build::lake::package_dir(&corpus);
            let shape = if mesh {
                qubitick_build::lake::Shape::Meshed
            } else {
                qubitick_build::lake::Shape::PerNode
            };
            let modules = qubitick_build::lake::write_package(&ctx, &db, &corpus, &out, shape)?;
            let report = if no_build {
                None
            } else {
                Some(qubitick_build::lake::build(&out)?)
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "package": out.display().to_string(),
                    "modules": modules.len(),
                    "built": report.as_ref().map(|r| r.built),
                    // Lake's own words, in the same shape as every other failure:
                    // a package build reports files and lines we did not write, so
                    // the message is the line and the location stays in it.
                    "failures": report
                        .as_ref()
                        .map(|r| {
                            r.failures
                                .iter()
                                .map(|line| serde_json::to_value(
                                    qubitick_model::Diagnostic::error(
                                        qubitick_model::Phase::Build,
                                        "build.lake-error",
                                        line.clone(),
                                    )
                                    .in_file(out.display().to_string()),
                                ).unwrap_or_default())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default(),
                    "failure_count": report.as_ref().map(|r| r.failures.len()).unwrap_or(0),
                    "warnings": Vec::<serde_json::Value>::new(),
                    "warning_count": 0,
                }))?
            );
        }
        Commands::Parse { file } => {
            let src = std::fs::read_to_string(&file)?;
            /* THE CORPUS, FOUND FROM THE FILE. This parsed every file against an EMPTY context
               labelled `Era0/Strata0` — the harness's first stratum — so a glyph written in the
               corpus was unknown to the very command that claims to show what the corpus makes of
               the file. The corpus root is the nearest ancestor carrying the charter (which the
               charter rule already says belongs at the root); with none, structure is all there is
               to resolve, and the label is the file's own scope rather than somebody else's. */
            let corpus = file
                .parent()
                .and_then(|d| {
                    let mut dir = Some(d.to_path_buf());
                    while let Some(cur) = dir {
                        if cur.join("Canon.qbit").is_file() {
                            return Some(cur);
                        }
                        dir = cur.parent().map(std::path::Path::to_path_buf);
                    }
                    None
                });
            let ctx = match &corpus {
                Some(root) => {
                    let mut db = qubitick_db::open_in_memory()?;
                    qubitick_build::ingest_corpus(&mut db, root)?;
                    qubitick_ctx::build_ctx(&db, &qubitick_build::pipeline::scope_of_path(&file))?
                }
                None => qubitick_ctx::Ctx::from_rows(
                    &qubitick_build::pipeline::scope_of_path(&file),
                    vec![],
                ),
            };
            let nodes = qubitick_parse::parse_content(&ctx, &src)?;
            let kinds: Vec<&str> = nodes.iter().map(|n| n.base().kind.as_str()).collect();
            /* The context is PRINTED because otherwise this command cannot be checked: it used to
               read every file against an empty context, and the output — file, kind, count — was
               byte-identical to the same file read against its corpus, on every file in the
               shipped project. `scope` and `symbols` are the claim ("what the corpus makes of this
               file"); `ast` is what the endpoint this mirrors answers with (`POST /api/parse`),
               and it is where a resolved glyph shows up. */
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "file": file.display().to_string(),
                    "scope": ctx.scope(),
                    "symbols": ctx.symbols().count(),
                    "nodes": nodes.len(),
                    "kinds": kinds,
                    "ast": format!("{nodes:?}"),
                }))?
            );
        }
        Commands::Axioms { root, file } => {
            // THE FOOTPRINT IS LEAN'S, shown as it reports it. The ground a corpus is
            // checked against decides it (Init names its own axioms; a bare prelude names
            // none), so the same theorem can print two different footprints and both are
            // true — which is what makes this a report and not a gate.
            let mut db = qubitick_db::open_in_memory()?;
            qubitick_build::ingest_corpus(&mut db, &root)?;
            let scope = qubitick_build::pipeline::scope_of_path(&file);
            let ctx = qubitick_ctx::build_ctx(&db, &scope)?;
            let src_text = std::fs::read_to_string(&file)?;
            let nodes = qubitick_parse::parse_content(&ctx, &src_text)?;
            let names: Vec<String> = nodes.iter().map(|n| n.base().name.clone()).collect();
            let emitted = qubitick_build::emit_file(&ctx, &db, &file)?;
            let ground = qubitick_build::ground::for_check(&root).ok();
            /* THE CORPUS AS OLEANS FIRST. The module being probed imports the modules
               that define the glyphs it names, so a footprint cannot be read until the
               corpus it belongs to has compiled. */
            let tree = qubitick_build::pipeline::compiled_tree(
                &db,
                &qubitick_build::walk_qbit(&root),
                ground.as_deref(),
            )
            .ok();
            let footprints = qubitick_build::lean_check::axioms_footprint(
                tree.as_deref().or(ground.as_deref()),
                &emitted,
                &names,
            )?;
            let rows: Vec<serde_json::Value> = footprints
                .iter()
                .map(|f| json!({ "name": f.name, "axioms": f.axioms }))
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "file": file.display().to_string(),
                    "ground": ground.map(|g| g.display().to_string()),
                    "footprints": rows,
                }))?
            );
        }
        Commands::Scaffold {
            root,
            namespace,
            write,
        } => {
            let root_path = PathBuf::from(&root);
            let mut db = qubitick_db::open_in_memory()?;
            qubitick_build::ingest_corpus(&mut db, &root_path)?;
            let text = qubitick_build::scaffold::scaffold(&db, &root_path, &namespace)?;
            if write {
                let path = qubitick_build::presets::ground_file(&root_path, "prelude");
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&path, format!("{text}\n"))?;
                eprintln!("scaffold: wrote {}", path.display());
            } else {
                println!("{text}");
            }
        }
        Commands::HashGround { root } => {
            println!("{}", qubitick_build::presets::hash(&root)?);
        }
        Commands::Seed { root, preset } => {
            let path = qubitick_build::presets::seed(&root, &preset)?;
            eprintln!("seed: wrote {} from the `{preset}` preset", path.display());
        }
        Commands::Emit { file, root } => {
            // Same DB + CTX the build path uses, so the printed module matches
            // what `build` would hand to `lean`.
            let mut db = qubitick_db::open_in_memory()?;
            qubitick_build::ingest_corpus(&mut db, &root)?;
            // the FILE's scope: emission is where a file's own symbols have to be visible
            let ctx = qubitick_ctx::build_ctx(&db, &qubitick_build::pipeline::scope_of_path(&file))?;
            let emitted = qubitick_build::emit_file(&ctx, &db, &file)?;
            print!("{emitted}");
        }
    }
    Ok(())
}

/// The scope a corpus-level command speaks in.
///
/// A command with no file argument has no file's scope to use, and `Era0/Strata0` — which is
/// where these three used to read — is the harness's first stratum, not a fact about somebody
/// else's corpus. The corpus's own first node is the closest honest answer: it is a scope the
/// corpus actually has. A single-file command (`emit`) uses the FILE's scope and not this.
fn corpus_scope(corpus: &std::path::Path) -> String {
    qubitick_build::walk_qbit(corpus)
        .first()
        .map(|f| qubitick_build::pipeline::scope_of_path(f))
        .unwrap_or_else(|| qubitick_build::pipeline::scope_of_path(corpus))
}

/// Condense a multi-line build diagnostic into ONE informative line.
///
/// Lean reports `error:` lines interleaved with `warning:` lines, and which
/// line comes first is not fixed — so a naive `take(3)` can report nothing but
/// warnings and hide the failure entirely. Errors win; warnings are the
/// fallback. Each chosen line keeps its `file:line:col: error: message` shape
/// with the scratch path stripped, since the scratch path is meaningless to the
/// reader and dominates the width.
/// The warnings across a build, as JSON, in file order.
fn warnings_json(results: &[qubitick_build::BuildFileResult]) -> Vec<serde_json::Value> {
    results
        .iter()
        .flat_map(|r| r.diagnostics.iter())
        .filter(|d| !d.is_error())
        .map(|d| serde_json::to_value(d).unwrap_or_default())
        .collect()
}



