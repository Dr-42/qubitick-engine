//! The base ground: the Lean a corpus needs before it has any of its own.
//!
//! The engine's emission names a fixed set of declarations (see
//! [`crate::prelude_contract`]) and nothing else. Those declarations have to come
//! from somewhere, and where they come from changed when a corpus became its own
//! repository: a corpus is now cloned *anywhere*, so the engine cannot find its
//! ground by walking up from the corpus — the engine has to carry it.
//!
//! So there are three sources, in order:
//!
//!   1. `QUBITICK_PRELUDE=<dir>` — an explicit ground, for a project or a packager.
//!   2. the checkout's `prelude/` — the development case, where the engine repo has
//!      one next to its sources.
//!   3. **embedded** — the same two files compiled into the binary, so a corpus in
//!      its own repository, on a machine with only the engine installed, still
//!      builds. This is the case that matters after decoupling.
//!
//! A project's own ground (`Canon.qbit`'s `LeanDir:`) is laid OVER this base: the
//! base provides the contract, the project provides everything above it.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// `Qubitick/HL.lean` as shipped — THE ground, compiled into the engine. One file carries the whole
/// contract (crates/build/src/prelude_contract.rs::REQUIRED); a project's own ground replaces exactly
/// this file when `assemble` lays it over the base, which is why there is one and not two.
pub const HL_LEAN: &str = include_str!("../../../prelude/Qubitick/HL.lean");

/// Assemble the package's ground into `src`: the engine's BASE, then the project's own OVER it.
///
/// This is the line the decoupling rests on. The engine ships `core` and the transpiler; a
/// project ships its corpus and, when its logic is its own, its `HL.lean` — and the charter is
/// where it says so (`LeanDir:` names the directory holding `Qubitick/*.lean`). Both are needed:
/// the base provides the declarations emission names, the project provides everything above them,
/// and the project's file WINS on a name collision, which is what makes it the project's ground
/// rather than an addition to ours.
///
/// A `LeanDir` that does not hold `Qubitick/HL.lean` is refused by name here rather than
/// silently falling back to the base: a project that asked for its own ground and got ours would
/// be checked against a contract its author did not write, and every error after that would be
/// about the wrong file.
pub fn assemble(corpus: &Path, canon: Option<&crate::canon::Canon>, src: &Path) -> Result<Source> {
    let named = canon
        .and_then(|c| c.lean_dir.as_ref())
        .map(|dir| corpus.join(dir));
    if let Some(dir) = &named
        && !dir.join("Qubitick").join("HL.lean").is_file()
    {
        anyhow::bail!(
            "the charter names LeanDir '{}', which has no Qubitick/HL.lean",
            dir.display()
        );
    }
    let base = materialise(src)?;
    if let Some(dir) = &named {
        copy_tree(dir, src)
            .with_context(|| format!("lay the project's ground from {}", dir.display()))?;
    }
    Ok(base)
}

/// A ground a corpus can be CHECKED against, materialised in a directory of its own.
///
/// The per-file check needs a `LEAN_PATH` root holding `Qubitick/*.lean` and their oleans, and
/// it has to be the ground the package would be written with — the engine's base with the
/// project's `LeanDir` laid over it — or a corpus is checked against one ground and packaged
/// against another, and the errors a project sees are about a file it does not own.
///
/// The contract is proved here, once per ground, before a single file is checked: a project whose
/// `HL.lean` does not typecheck gets one sentence naming its ground instead of three hundred
/// errors in scratch modules it never wrote.
///
/// STAMPED, because this is called where a build already is: the check stream asks for the ground
/// before every file, and wiping and recompiling the prelude per file would add a compile to a
/// check that is meant to be one `lean` on one module. The stamp covers everything the assembled
/// ground is made of — the two base files, an explicit `QUBITICK_PRELUDE`, the module name, and
/// the bytes of every `.lean` under the charter's `LeanDir` — so an edit to a project's `HL.lean`
/// is a different ground, and nothing else is.
pub fn for_check(corpus: &Path) -> Result<PathBuf> {
    let canon = crate::canon::read(corpus)?;
    // One directory per CORPUS (not per process): two corpora built in one process — a swap, a
    // test — must not share a ground, and the same corpus built twice must not inherit the files
    // of a `LeanDir` it has since changed.
    let dir = std::env::temp_dir().join(format!("qubitick-ground-{:016x}", key_of(corpus)));
    let stamp = format!("{:016x}", stamp_of(corpus, canon.as_ref()));
    let stamp_file = dir.join("STAMP");
    let current = std::fs::read_to_string(&stamp_file).ok();
    if current.as_deref() == Some(stamp.as_str())
        && dir.join("Qubitick").join("HL.olean").is_file()
    {
        return Ok(dir);
    }
    let _ = std::fs::remove_dir_all(&dir);
    let _ = assemble(corpus, canon.as_ref(), &dir)?;
    /* Name whose ground it was, and where the charter said it lives: the directory this compiled
       in is a temp copy, so an error that only said `/tmp/...` would send a reader to a file that
       is not theirs (crates/build/src/lean_check.rs::ensure_prelude names the module it compiled). */
    let named = canon.as_ref().and_then(|c| c.lean_dir.as_ref());
    check(&dir, canon.as_ref()).map_err(|e| match named {
        Some(lean_dir) => e.context(format!(
            "the ground this project brings (LeanDir: {lean_dir}, in {})",
            corpus.display()
        )),
        None => e.context(format!("the shipped ground, for {}", corpus.display())),
    })?;
    // AFTER the compile: a ground that half-built must not claim to be current.
    std::fs::write(&stamp_file, &stamp)?;
    Ok(dir)
}

/// A stable name for a corpus path, so its ground directory is the same across builds.
fn key_of(path: &Path) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf()).hash(&mut h);
    h.finish()
}

/// Everything the assembled ground is made of, as one number.
fn stamp_of(corpus: &Path, canon: Option<&crate::canon::Canon>) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    /* THE COMPILER IS PART OF THE GROUND. A `.olean` is only readable by the Lean that wrote it
       — `incompatible header` is what a different one says — and this stamp is what decides
       whether a compiled ground is reused. Without the version, the same corpus built first by
       one Lean and then by another kept the FIRST one's oleans and every file failed with
       `failed to read file '.../HL.olean', incompatible header`, which names a temp path and
       nothing a reader can act on. */
    lean_version().hash(&mut h);
    HL_LEAN.hash(&mut h);
    std::env::var("QUBITICK_PRELUDE").ok().hash(&mut h);
    canon.map(|c| c.lean_module.clone()).hash(&mut h);
    if let Some(dir) = canon.and_then(|c| c.lean_dir.as_ref()) {
        let mut files: Vec<PathBuf> = Vec::new();
        collect_lean(&corpus.join(dir).join("Qubitick"), &mut files);
        files.sort();
        for f in files {
            f.hash(&mut h);
            if let Ok(bytes) = std::fs::read(&f) {
                bytes.hash(&mut h);
            }
        }
    }
    h.finish()
}

/// The version of the `lean` this process will run, or `""` when it cannot be asked.
fn lean_version() -> String {
    std::process::Command::new("lean")
        .arg("--version")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn collect_lean(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_lean(&path, out);
        } else if path.extension().map(|e| e == "lean").unwrap_or(false) {
            out.push(path);
        }
    }
}

/// Compile the ground in `src` and probe it for the emitter's contract.
///
/// Everything ABOVE the base belongs to the project (Init, Mathlib, a previous development);
/// the base itself is ours, and a missing declaration is reported by name and tier rather than
/// surfacing as an unexplained error deep in a module.
pub fn check(src: &Path, canon: Option<&crate::canon::Canon>) -> Result<()> {
    let module = canon
        .map(|c| c.lean_module.clone())
        .unwrap_or_else(|| "Qubitick.HL".to_string());
    // Absolute: this directory is both the working directory and `LEAN_PATH`, and a
    // relative value would be resolved against itself.
    let src = &src.canonicalize().unwrap_or_else(|_| src.to_path_buf());
    crate::lean_check::ensure_prelude(src)?;
    let probe = src.join("Contract.lean");
    std::fs::write(&probe, crate::prelude_contract::probe(&module))?;
    let out = std::process::Command::new("lean")
        .arg("Contract.lean")
        .current_dir(src)
        .env("LEAN_PATH", src)
        .output()
        .context("spawn `lean` for the contract probe")?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let missing = crate::prelude_contract::missing_from(&text);
    if !missing.is_empty() {
        let listed: Vec<String> = missing
            .iter()
            .map(|name| {
                let tier = crate::prelude_contract::tier_of(name)
                    .map(|t| t.label())
                    .unwrap_or("?");
                format!("{name} ({tier})")
            })
            .collect();
        anyhow::bail!(
            "the project's ground (`{module}`) does not meet the emitter's contract; missing: {}",
            listed.join(", ")
        );
    }
    if !out.status.success() {
        anyhow::bail!("the project's ground failed to check:\\n{text}");
    }
    Ok(())
}

/// Where the base ground came from, for a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// `QUBITICK_PRELUDE` named a directory.
    Env(PathBuf),
    /// A `prelude/` found near the engine's sources.
    Checkout(PathBuf),
    /// The copy inside the binary.
    Embedded,
}

/// Copy the base ground's `Qubitick/*.lean` into `src`, returning where it came from.
pub fn materialise(src: &Path) -> Result<Source> {
    std::fs::create_dir_all(src.join("Qubitick"))?;
    if let Some(dir) = explicit() {
        copy_tree(&dir, src)?;
        return Ok(Source::Env(dir));
    }
    if let Some(dir) = checkout() {
        copy_tree(&dir, src)?;
        return Ok(Source::Checkout(dir));
    }
    std::fs::write(src.join("Qubitick").join("HL.lean"), HL_LEAN)?;
    Ok(Source::Embedded)
}

/// `QUBITICK_PRELUDE` — an explicit base ground.
pub fn explicit() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var("QUBITICK_PRELUDE").ok()?);
    let ok = dir.join("Qubitick").join("HL.lean").is_file();
    ok.then_some(dir)
}

/// A `prelude/` beside the engine's own sources (the development checkout).
fn checkout() -> Option<PathBuf> {
    let from_manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .parent()?
        .join("prelude");
    if from_manifest.join("Qubitick").join("HL.lean").is_file() {
        return Some(from_manifest);
    }
    let from_cwd = std::env::current_dir().ok()?.join("prelude");
    if from_cwd.join("Qubitick").join("HL.lean").is_file() {
        return Some(from_cwd);
    }
    None
}

/// Copy every `.lean` under `<dir>/Qubitick` into `<dst>/Qubitick`, overwriting.
///
/// Overwriting is the point: when a project's `LeanDir` is laid over the base, the
/// project's `HL.lean` is the one that ends up in the package.
///
/// RECURSIVE, because a ground is a module TREE and not a list of files: `Qubitick/HL.lean`
/// may import `Qubitick/Frame/Shared.lean`, and a flat copy would leave the package with the
/// import and not the module — an error about a file the project never wrote. The base has
/// two files today, so the recursion changes nothing for it; it exists for the project's half.
pub fn copy_tree(dir: &Path, dst: &Path) -> Result<()> {
    copy_lean(&dir.join("Qubitick"), &dst.join("Qubitick"))
}

fn copy_lean(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    let entries = std::fs::read_dir(from).with_context(|| format!("read ground {}", from.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            copy_lean(&path, &to.join(entry.file_name()))?;
        } else if path.extension().map(|e| e == "lean").unwrap_or(false) {
            std::fs::copy(&path, to.join(entry.file_name()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_ground_is_the_shipped_one() {
        // the constants are `include_str!` of the shipped prelude, so this asserts
        // the copy in the binary has not drifted from the file
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("prelude/Qubitick");
        assert_eq!(
            HL_LEAN,
            std::fs::read_to_string(repo.join("HL.lean")).unwrap()
        );
    }

    #[test]
    fn materialise_always_produces_a_ground() {
        let dir = std::env::temp_dir().join(format!("ground-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let source = materialise(&dir).unwrap();
        assert!(dir.join("Qubitick").join("HL.lean").is_file(), "{source:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /* ---- the project's own ground: the decoupling's one real hinge ---- */

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ground-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn put(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// A charter that names `<corpus>/<dir>` as the project's ground.
    fn charter(lean_dir: &str) -> crate::canon::Canon {
        crate::canon::Canon {
            name: "Probe".into(),
            id: "probe".into(),
            lean_dir: Some(lean_dir.into()),
            lean_module: "Qubitick.HL".into(),
            requires: Vec::new(),
            precedence: Vec::new(),
        }
    }

    #[test]
    fn a_projects_ground_is_laid_over_the_base() {
        let dir = temp("over");
        let corpus = dir.join("corpus");
        /* a project ships a WHOLE ground, not one file: its own `HL.lean` and a module under it.
           The nested one is the case a flat copy dropped silently — the package would carry
           `import Qubitick.Frame.Shape` and not the module, and the build would complain about a
           file the project never wrote. */
        put(&corpus.join("lean/Qubitick/HL.lean"), "-- the project's own HL\n");
        put(&corpus.join("lean/Qubitick/Frame/Shape.lean"), "-- a nested module of the ground\n");
        let src = dir.join("src");
        assemble(&corpus, Some(&charter("lean")), &src).unwrap();
        assert_eq!(
            std::fs::read_to_string(src.join("Qubitick/HL.lean")).unwrap(),
            "-- the project's own HL\n",
            "the project's HL.lean is the one in the package, not the base's"
        );
        assert!(
            src.join("Qubitick/Frame/Shape.lean").is_file(),
            "a nested module of the ground came along"
        );
        /* THE GROUND IS ONE FILE, so laying a project's ground over the base REPLACES it outright:
           nothing of the base survives beside the project's own `HL.lean`. With two files a project
           that shipped only `HL.lean` silently kept the engine's other half — the case this asserts
           cannot happen. */
        assert!(
            !src.join("Qubitick/Core.lean").exists(),
            "one ground file: the project's HL.lean is the ground, not one half of it"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_ground_without_an_hl_is_refused_by_name_and_leaves_no_package() {
        let dir = temp("refused");
        let corpus = dir.join("corpus");
        put(&corpus.join("lean/Qubitick/Something.lean"), "-- not an HL\n");
        let src = dir.join("src");
        let err = assemble(&corpus, Some(&charter("lean")), &src)
            .unwrap_err()
            .to_string();
        assert!(err.contains("LeanDir"), "the refusal names the field: {err}");
        assert!(err.contains("corpus/lean"), "and the directory it looked in: {err}");
        assert!(err.contains("Qubitick/HL.lean"), "{err}");
        assert!(
            !src.join("Qubitick/HL.lean").is_file(),
            "the check runs BEFORE the base is materialised: a refused ground leaves no package"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The stamp exists because `for_check` is called where a build already is: the check stream
    /// asks for the ground before every file. Reusing it must be reuse, and an edit to the
    /// project's `HL.lean` must be a rebuild — a stale `.olean` would be a check against a file
    /// the project no longer has.
    #[test]
    fn the_ground_is_reused_until_something_in_it_changes() {
        if std::process::Command::new("lean")
            .arg("--version")
            .output()
            .is_err()
        {
            return; // no lean in this environment
        }
        let dir = temp("stamp");
        let corpus = dir.join("corpus");
        let hl = corpus.join("lean/Qubitick/HL.lean");
        put(&hl, HL_LEAN);
        put(&corpus.join("Canon.qbit"), "P. Stamped\nId: stamped\nLeanDir: lean\n");

        let first = for_check(&corpus).unwrap();
        assert!(first.join("Qubitick/HL.olean").is_file(), "the ground was compiled");
        /* something a rebuild would remove: a second call must leave it alone */
        put(&first.join("MARKER"), "still here");
        let second = for_check(&corpus).unwrap();
        assert_eq!(first, second, "one corpus, one ground directory");
        assert!(
            second.join("MARKER").is_file(),
            "an unchanged ground is reused, not recompiled"
        );

        /* an edit to the project's own file is a DIFFERENT ground */
        put(&hl, &format!("{HL_LEAN}\n-- an edit\n"));
        let third = for_check(&corpus).unwrap();
        assert_eq!(first, third);
        assert!(
            !third.join("MARKER").is_file(),
            "an edit to the project's HL.lean rebuilds the ground"
        );
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&third);
    }

    #[test]
    fn without_a_lean_dir_the_shipped_ground_is_the_ground() {
        let dir = temp("base");
        let corpus = dir.join("corpus");
        std::fs::create_dir_all(&corpus).unwrap();
        let src = dir.join("src");
        let source = assemble(&corpus, None, &src).unwrap();
        match source {
            // an explicit `QUBITICK_PRELUDE` is the packager's ground, not ours to compare
            Source::Env(_) => {}
            _ => assert_eq!(
                std::fs::read_to_string(src.join("Qubitick/HL.lean")).unwrap(),
                HL_LEAN,
                "the base is the shipped ground when the project names none"
            ),
        }
        /* one file, and it is the shipped ground (asserted above) */
        assert!(!src.join("Qubitick/Core.lean").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}