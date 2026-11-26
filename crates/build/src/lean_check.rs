//! Invoke the `lean` binary to check a single emitted `.lean` file (CodeArch §5.12).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Context, Result};
use qubitick_model::{Diagnostic, Phase, Severity, Span};

static SCRATCH_SEQ: AtomicUsize = AtomicUsize::new(0);
/// One module tree per compile, even when two corpora are built in one process.
static TREE_SEQ: AtomicUsize = AtomicUsize::new(0);

/// A fresh directory for one module tree. Two corpora built in one process (a swap, a
/// test) must not share a tree: the second would inherit the first's modules, and a
/// name they both declare would look like a collision.
pub fn tree_dir() -> std::path::PathBuf {
    let n = TREE_SEQ.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("qubitick-modules-{}-{n}", std::process::id()))
}

/// Check an emitted Lean file by invoking the `lean` binary on a scratch copy.
///
/// The emitted module is written to a temporary `.lean` file and type-checked
/// with `LEAN_PATH` pointed at the corpus `prelude/` (so `import Qubitick.HL`
/// resolves `Eq`/`rfl`/`@Eq.trans` and the Prop toolkit). Returns `Ok(())` if
/// the file type-checks, or an `Err` carrying Lean's diagnostics on failure.
/// Lean is provisioned by nix-portable.
pub fn check_file(path: &Path, emitted: &str, ground: Option<&Path>) -> Result<()> {
    match check_file_diagnostics(path, emitted, ground)?.first() {
        None => Ok(()),
        Some(d) => anyhow::bail!(
            "lean rejected {}:\n{}\n{}",
            path.display(),
            d.message,
            d.hints.join("\n")
        ),
    }
}

/// Check an emitted module and return what Lean said, structured.
///
/// Empty means it type-checked. A rejection is one [`Diagnostic`] per Lean error:
/// severity/phase/code, the corpus file it came from, and the line **in the
/// emitted module** — which is a different document from the `.qbit`, so the
/// number is carried as `lean_line` rather than pretended to be a source line.
pub fn check_file_diagnostics(
    path: &Path,
    emitted: &str,
    ground: Option<&Path>,
) -> Result<Vec<Diagnostic>> {
    let n = SCRATCH_SEQ.fetch_add(1, Ordering::Relaxed);
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "scratch".to_string());
    let dir = std::env::temp_dir().join(format!("qubitick-check-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let scratch = dir.join(format!("{name}-{n}.lean"));
    std::fs::write(&scratch, emitted)
        .with_context(|| format!("writing scratch lean {}", scratch.display()))?;

    // THE GROUND THE CALLER ASSEMBLED WINS: `crate::ground::for_check` builds the engine's base
    // with the project's `LeanDir` laid over it, which is what a corpus build must check against —
    // a project's own `HL.lean` is the point of the decoupling. Passing `None` is only for a
    // caller that has a FILE and no corpus, and it falls back the way the package does: a
    // `prelude/` above the file, else an explicit `QUBITICK_PRELUDE`, else the copy in this binary.
    let prelude = resolve_prelude(ground, path);
    if let Some(p) = &prelude {
        ensure_prelude(p)?;
    }
    let mut cmd = std::process::Command::new("lean");
    cmd.arg(&scratch);
    if let Some(p) = &prelude {
        cmd.env("LEAN_PATH", p);
    }
    let out = cmd
        .output()
        .with_context(|| format!("failed to spawn `lean` for {}", scratch.display()))?;
    if out.status.success() {
        return Ok(Vec::new());
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let mut diagnostics = parse_lean_errors(path, &stdout);
    diagnostics.extend(parse_lean_errors(path, &stderr));
    if diagnostics.is_empty() {
        // Lean said no without saying where: keep the words, lose no information.
        diagnostics.push(
            Diagnostic::error(
                Phase::Build,
                "build.lean-failed",
                format!("lean rejected the module:\n{}", stdout.trim()),
            )
            .in_file(path.display().to_string()),
        );
    }
    Ok(diagnostics)
}

/// One module of a corpus build: the Lean emitted for a file, and where it belongs.
pub struct ModuleUnit {
    /// The module name the emission gave it (`Qubitick.Corpus.Era0...D_A_02_Mul`).
    pub module: String,
    /// The `.qbit` it came from, which is the file a diagnostic names.
    pub file: PathBuf,
    /// The emitted Lean.
    pub source: String,
}

/// Compile a corpus's modules IN DEPENDENCY ORDER, into one shared tree.
///
/// An emitted module IMPORTS the modules that define the glyphs it names, and a Lean
/// import needs the imported module's `.olean`. So the corpus compiles the way a package
/// does: every module written once into `tree`, then each module compiled after the ones
/// it imports. This is the answer to "a definition seen from a module that cannot unfold
/// it" — the old answer was an `axiom` pair, and the honest one is to build the module
/// that defines it first.
///
/// Returns one diagnostic list per unit, in the caller's order. A module whose import
/// never compiled reports Lean's own words about the missing object file, which is the
/// truth about that module.
pub fn check_modules(
    ground: Option<&Path>,
    units: &[ModuleUnit],
    tree: &Path,
) -> Result<Vec<Vec<Diagnostic>>> {
    let src = tree.join("src");
    let mut paths: Vec<PathBuf> = Vec::with_capacity(units.len());
    for unit in units {
        let rel = unit.module.replace('.', "/");
        let path = src.join(format!("{rel}.lean"));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &unit.source)
            .with_context(|| format!("writing module {}", path.display()))?;
        paths.push(path);
    }
    let prelude = resolve_prelude(ground, tree);
    if let Some(p) = &prelude {
        ensure_prelude(p)?;
    }
    /* ONE ROOT, holding the ground AND the corpus's modules.
     *
     * Lean resolves an import against the first search-path root whose leading
     * directory matches, so two roots — the ground (holding `Qubitick/`) and a module
     * tree (holding `Qubitick/Corpus/`) — make every corpus import look for its olean
     * under whichever root it happened to match first. A package has one `src/` with
     * both; so does this. */
    if let Some(p) = &prelude {
        copy_ground_into(p, &src)?;
    }
    let lean_path = src.display().to_string();

    // Who imports whom, within the corpus (the ground is already compiled).
    let known: std::collections::HashMap<&str, usize> = units
        .iter()
        .enumerate()
        .map(|(i, u)| (u.module.as_str(), i))
        .collect();
    let deps: Vec<Vec<usize>> = units
        .iter()
        .map(|u| {
            let mut out: Vec<usize> = Vec::new();
            for line in u.source.lines() {
                let Some(rest) = line.trim().strip_prefix("import ") else {
                    continue;
                };
                if let Some(i) = known.get(rest.trim())
                    && !out.contains(i)
                {
                    out.push(*i);
                }
            }
            out
        })
        .collect();

    let mut results: Vec<Option<Vec<Diagnostic>>> = (0..units.len()).map(|_| None).collect();
    let mut done: std::collections::HashSet<usize> = std::collections::HashSet::new();
    while done.len() < units.len() {
        let wave: Vec<usize> = (0..units.len())
            .filter(|i| !done.contains(i) && deps[*i].iter().all(|d| done.contains(d)))
            .collect();
        if wave.is_empty() {
            // A cycle: every module left waits on another. Name them and stop; the
            // corpus has a shape the emitter cannot order, which is its own error.
            for i in 0..units.len() {
                if done.contains(&i) {
                    continue;
                }
                results[i] = Some(vec![Diagnostic::error(
                    Phase::Build,
                    "build.import-cycle",
                    "this module is part of an import cycle: no order compiles it".to_string(),
                )
                .in_file(units[i].file.display().to_string())]);
            }
            break;
        }
        let slots: Vec<std::sync::Mutex<Option<Vec<Diagnostic>>>> =
            wave.iter().map(|_| std::sync::Mutex::new(None)).collect();
        std::thread::scope(|scope| {
            for (slot, &i) in slots.iter().zip(wave.iter()) {
                let (path, lean_path) = (paths[i].clone(), lean_path.clone());
                let file = units[i].file.clone();
                let src_dir = src.clone();
                let rel = format!("{}.lean", units[i].module.replace('.', "/"));
                let olean = path.with_extension("olean");
                scope.spawn(move || {
                    /* `-o` and a RELATIVE input, run from the module root: Lean writes
                       an `.olean` only where it is told, and it wants the input to live
                       under the root it resolves imports from. Without the olean an
                       `import` of this module fails for every dependent — which is the
                       whole reason a corpus compiles in dependency order. */
                    let mut cmd = std::process::Command::new("lean");
                    cmd.current_dir(&src_dir)
                        .arg("-o")
                        .arg(&olean)
                        .arg(&rel)
                        .env("LEAN_PATH", &lean_path);
                    let out = cmd.output();
                    let diagnostics = match out {
                        Err(e) => vec![Diagnostic::error(
                            Phase::Build,
                            "build.lean-failed",
                            format!("failed to spawn `lean` for {}: {e}", path.display()),
                        )
                        .in_file(file.display().to_string())],
                        Ok(out) if out.status.success() => Vec::new(),
                        Ok(out) => {
                            let stdout = String::from_utf8_lossy(&out.stdout);
                            let stderr = String::from_utf8_lossy(&out.stderr);
                            let mut ds = parse_lean_errors(&file, &stdout);
                            ds.extend(parse_lean_errors(&file, &stderr));
                            if ds.is_empty() {
                                ds.push(
                                    Diagnostic::error(
                                        Phase::Build,
                                        "build.lean-failed",
                                        format!("lean rejected the module:\n{}", stdout.trim()),
                                    )
                                    .in_file(file.display().to_string()),
                                );
                            }
                            ds
                        }
                    };
                    *slot.lock().expect("wave slot lock") = Some(diagnostics);
                });
            }
        });
        for (slot, &i) in slots.into_iter().zip(wave.iter()) {
            results[i] = slot.into_inner().expect("wave slot lock");
            done.insert(i);
        }
    }
    Ok(results
        .into_iter()
        .map(|r| r.unwrap_or_default())
        .collect())
}

/// Lay a compiled ground's `Qubitick/` into a module tree, so one root holds both.
fn copy_ground_into(ground: &Path, src: &Path) -> Result<()> {
    fn walk(from: &Path, to: &Path) -> Result<()> {
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            let path = entry.path();
            let target = to.join(entry.file_name());
            if path.is_dir() {
                std::fs::create_dir_all(&target)?;
                walk(&path, &target)?;
            } else if path.extension().is_some_and(|e| e == "lean" || e == "olean") {
                std::fs::create_dir_all(to)?;
                std::fs::copy(&path, &target)?;
            }
        }
        Ok(())
    }
    let from = ground.join("Qubitick");
    if from.is_dir() {
        // `Qubitick/` exists in the tree already (the corpus's own modules); this
        // merges the ground's files in beside them rather than replacing the tree.
        walk(&from, &src.join("Qubitick"))?;
    }
    Ok(())
}

/// The ground a Lean invocation runs against: the caller's assembled ground, else a
/// `prelude/` above the file, else the copy in this binary.
fn resolve_prelude(ground: Option<&Path>, path: &Path) -> Option<PathBuf> {
    ground
        .map(|g| g.to_path_buf())
        .or_else(|| find_prelude_dir(path))
        .or_else(ground_cache)
        .map(|p| p.canonicalize().unwrap_or(p))
}

/// What one theorem STANDS ON, as Lean itself reports it.
///
/// Reported, never policed: an empty footprint is a fact about the ground (a bare prelude
/// carries no axioms), and the names Lean's Init carries appear as they are. Nothing here
/// can fail a build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Footprint {
    /// The emitted constant's name.
    pub name: String,
    /// The axioms Lean reports for it — `[]` for a proof that stands on none.
    pub axioms: Vec<String>,
}

/// Ask Lean what each named constant of an emitted module stands on.
///
/// The module text is handed to `lean` with one `#print axioms <name>` appended per name,
/// against the same ground the check uses, and Lean's own lines are read back:
///
/// ```text
/// 't4' depends on axioms: [propext, Classical.choice, Quot.sound]
/// 't5' does not depend on any axioms
/// ```
///
/// A name Lean does not know is an error here: the caller asked about a constant the module
/// does not declare, and saying so beats reporting an empty footprint for it.
pub fn axioms_footprint(
    ground: Option<&Path>,
    emitted: &str,
    names: &[String],
) -> Result<Vec<Footprint>> {
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let mut body = String::from(emitted);
    if !body.ends_with('\n') {
        body.push('\n');
    }
    let mut prints = String::new();
    for n in names {
        prints.push_str("#print axioms ");
        prints.push_str(n);
        prints.push('\n');
    }
    // THE PRINTS BELONG INSIDE THE MODULE'S NAMESPACE: an emitted module closes it with
    // `end <Namespace>`, and a `#print axioms MulTwo` appended after that names a
    // top-level constant that does not exist.
    let text = match body.rfind("\nend ") {
        Some(i) => {
            let cut = i + 1;
            format!("{}{}{}", &body[..cut], prints, &body[cut..])
        }
        None => format!("{body}{prints}"),
    };
    let n = SCRATCH_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("qubitick-axioms-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let scratch = dir.join(format!("footprint-{n}.lean"));
    std::fs::write(&scratch, &text)
        .with_context(|| format!("writing scratch lean {}", scratch.display()))?;
    let prelude = resolve_prelude(ground, &scratch);
    if let Some(p) = &prelude {
        ensure_prelude(p)?;
    }
    let mut cmd = std::process::Command::new("lean");
    cmd.arg(&scratch);
    if let Some(p) = &prelude {
        cmd.env("LEAN_PATH", p);
    }
    let out = cmd
        .output()
        .with_context(|| format!("failed to spawn `lean` for {}", scratch.display()))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let footprints = footprint_lines(&stdout);
    if footprints.len() != names.len() {
        anyhow::bail!(
            "lean reported {} footprints for {} names:\n{}{}",
            footprints.len(),
            names.len(),
            stdout.trim(),
            if stderr.trim().is_empty() { "" } else { stderr.trim() }
        );
    }
    Ok(footprints)
}

/// Read Lean's `#print axioms` lines: a list of names, or an empty one.
///
/// Public because a caller may have Lean's output in hand already (a test, a log) and
/// should read it the same way the engine does.
pub fn footprint_lines(output: &str) -> Vec<Footprint> {
    let mut out = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix('\'') else {
            continue;
        };
        let Some((name, said)) = rest.split_once('\'') else {
            continue;
        };
        if let Some(list) = said
            .trim()
            .strip_prefix("depends on axioms: [")
            .and_then(|r| r.strip_suffix(']'))
        {
            out.push(Footprint {
                name: name.to_string(),
                axioms: list
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect(),
            });
        } else if said.trim() == "does not depend on any axioms" {
            out.push(Footprint {
                name: name.to_string(),
                axioms: Vec::new(),
            });
        }
    }
    out
}

/// Turn Lean's `file:line:col: error: …` lines into diagnostics.
///
/// Lean prints the position, the message, then indented context — including caret
/// rulers and quoted source lines. The carets and quotes are useful in a terminal
/// and noise in JSON, so they are dropped; the message and up to two real
/// continuation lines are kept as hints.
fn parse_lean_errors(path: &Path, output: &str) -> Vec<Diagnostic> {
    let file = path.display().to_string();
    let mut out: Vec<Diagnostic> = Vec::new();
    let mut current: Option<Diagnostic> = None;
    let flush = |current: &mut Option<Diagnostic>, out: &mut Vec<Diagnostic>| {
        if let Some(d) = current.take() {
            out.push(d);
        }
    };
    for raw in output.lines() {
        let text = raw.trim_end();
        if text.trim().is_empty() {
            continue;
        }
        match lean_position(text) {
            Some((line, col, message)) => {
                flush(&mut current, &mut out);
                let severity = if message.starts_with("warning") {
                    Severity::Warning
                } else {
                    Severity::Error
                };
                let body = strip_lean_severity(&message);
                current = Some(
                    Diagnostic::new(severity, Phase::Build, "build.lean-error", body).at(Span {
                        file: Some(file.clone()),
                        line: None,
                        col: None,
                        lean_line: Some(line),
                        lean_col: Some(col),
                    }),
                );
            }
            None => {
                // continuation of the error above it
                let t = text.trim();
                let noise = t.is_empty()
                    || t.chars().all(|c| matches!(c, '^' | '~' | '-' | ' '))
                    || t.starts_with('…')
                    || t == "Note: This error originates from";
                if noise {
                    continue;
                }
                if let Some(d) = current.as_mut()
                    && d.hints.len() < 2
                {
                    d.hints.push(t.to_string());
                }
            }
        }
    }
    flush(&mut current, &mut out);
    out
}

/// `path:12:3: error: message` → `(12, 3, "error: message")`.
///
/// The path is discovered from the right, because a corpus path may itself contain
/// colons on some platforms and the two numbers next to the message are the only
/// unambiguous part.
fn lean_position(line: &str) -> Option<(u32, u32, String)> {
    let parts: Vec<&str> = line.split(':').collect();
    if parts.len() < 4 {
        return None;
    }
    // The message may itself contain colons, so the two numbers adjacent to it are
    // the anchor and everything after them is the message, joined back up.
    for i in 2..parts.len() - 1 {
        let rest = parts[i + 1..].join(":");
        let rest = rest.trim();
        if !(rest.starts_with("error") || rest.starts_with("warning")) {
            continue;
        }
        let (Ok(line_no), Ok(col_no)) = (
            parts[i - 1].trim().parse::<u32>(),
            parts[i].trim().parse::<u32>(),
        ) else {
            continue;
        };
        return Some((line_no, col_no, rest.to_string()));
    }
    None
}

/// Drop Lean's `error:` / `error(lean.unknownIdentifier):` prefix.
fn strip_lean_severity(message: &str) -> String {
    let rest = message
        .strip_prefix("error")
        .or_else(|| message.strip_prefix("warning"))
        .unwrap_or(message);
    let rest = match rest.strip_prefix('(') {
        Some(r) => r.split_once(')').map_or(r, |(_, tail)| tail),
        None => rest,
    };
    rest.trim_start_matches(':').trim().to_string()
}

/// The base ground, materialised once per process into a stable directory.
///
/// Stable on purpose: `ensure_prelude` compiles the oleans next to the sources, and
/// rebuilding them on every file check would cost more than the check.
fn ground_cache() -> Option<PathBuf> {
    static CACHE: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| {
            let dir = std::env::temp_dir().join("qubitick-ground");
            crate::ground::materialise(&dir).ok()?;
            Some(dir)
        })
        .clone()
}

/// Compile `prelude/Qubitick/HL.olean` if it is missing, so an emitted module's
/// `import Qubitick.HL` resolves without a manual build step.
pub fn ensure_prelude(prelude: &std::path::Path) -> Result<()> {
    // Absolute, because the compiler is run IN this directory: a relative ground
    // would have its own path applied twice (`<rel>/<rel>/Qubitick/HL.olean`) and
    // the write would fail for a directory that does not exist.
    let prelude = prelude.canonicalize().unwrap_or_else(|_| prelude.to_path_buf());
    let prelude = prelude.as_path();
    let q = prelude.join("Qubitick");
    if q.join("Core.olean").is_file() && q.join("HL.olean").is_file() {
        return Ok(());
    }
    // Run IN the ground directory and name the file relatively: Lean requires an
    // input file to live under its root, and a project's ground may live anywhere.
    let run = |src: &str, olean: &str| -> Result<()> {
        let out = std::process::Command::new("lean")
            .current_dir(prelude)
            .arg("-o")
            .arg(q.join(olean))
            .arg(format!("Qubitick/{src}"))
            .env("LEAN_PATH", prelude)
            .output()
            .with_context(|| format!("spawn `lean -o {olean}` for prelude"))?;
        if out.status.success() {
            Ok(())
        } else {
            anyhow::bail!(
                "the ground's {} failed to compile:\n{}\n{}",
                src,
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        }
    };
    if !q.join("HL.olean").is_file() {
        run("HL.lean", "HL.olean")?;
    }
    Ok(())
}

/// Walk up from a `.qbit` file to locate the corpus `prelude/` directory whose
/// `Qubitick/HL.lean` grounds the emitted imports. Returns `Some(prelude_dir)`
/// when found, else `None` (the caller then checks without a custom `LEAN_PATH`).
///
/// The FALLBACK, for a check with no corpus behind it. A corpus build hands
/// [`check_file_diagnostics`] a ground built by `crate::ground::for_check` instead, so that a
/// project's `LeanDir` is what its files are checked against.
pub fn find_prelude_dir(path: &Path) -> Option<PathBuf> {
    let mut dir = path.parent()?;
    loop {
        let candidate = dir.join("prelude");
        if candidate.join("Qubitick").join("HL.lean").is_file() {
            return Some(candidate);
        }
        dir = dir.parent()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(out: &str) -> Vec<Diagnostic> {
        parse_lean_errors(Path::new("project/T_BT_01.qbit"), out)
    }

    #[test]
    fn a_lean_error_keeps_its_position_in_the_emitted_module() {
        let ds = parse("/tmp/qubitick-check-1/T_BT_01-0.lean:12:5: error: unknown identifier 'foo'\n");
        assert_eq!(ds.len(), 1);
        assert_eq!(ds[0].severity, Severity::Error);
        assert_eq!(ds[0].span.lean_line, Some(12));
        assert_eq!(ds[0].span.lean_col, Some(5));
        // the corpus file is what a reader can open; the lane line is not it
        assert_eq!(ds[0].span.file.as_deref(), Some("project/T_BT_01.qbit"));
        assert_eq!(ds[0].span.line, None);
        assert_eq!(ds[0].message, "unknown identifier 'foo'");
    }

    #[test]
    fn carets_and_quoted_source_are_not_hints() {
        let out = "/tmp/q.lean:3:9: error: application type mismatch\n  Delta a b\n        ^^^^^^^\n  expected Prop\n";
        let ds = parse(out);
        assert_eq!(ds.len(), 1);
        assert_eq!(ds[0].message, "application type mismatch");
        assert_eq!(ds[0].hints, vec!["Delta a b", "expected Prop"]);
    }

    #[test]
    fn every_error_is_kept_not_just_the_first() {
        let out = "/tmp/q.lean:1:1: error: one\n/tmp/q.lean:2:2: error: two\n";
        let ds = parse(out);
        assert_eq!(ds.len(), 2);
        assert_eq!(ds[1].span.lean_line, Some(2));
        assert_eq!(ds[1].message, "two");
    }

    #[test]
    fn a_categorised_lean_error_drops_the_category() {
        let ds = parse("/tmp/q.lean:7:3: error(lean.unknownIdentifier): Unknown constant `x`\n");
        assert_eq!(ds.len(), 1);
        assert_eq!(ds[0].message, "Unknown constant `x`");
    }

    #[test]
    fn a_lean_warning_is_a_warning() {
        let ds = parse("/tmp/q.lean:4:1: warning: declaration uses 'sorry'\n");
        assert_eq!(ds[0].severity, Severity::Warning);
        assert!(!ds[0].is_error());
    }

    #[test]
    fn output_without_positions_yields_nothing_to_parse() {
        assert!(parse("error: something went wrong\n").is_empty());
    }

    #[test]
    fn check_is_callable_even_when_lean_absent_or_present() {
        // We don't assert on lean presence; just that the function is callable.
        let _ = check_file(Path::new("scratch.lean"), "theorem t : True := by trivial", None);
    }
}