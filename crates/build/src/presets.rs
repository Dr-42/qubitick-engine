//! Ground presets — the Lean a project starts from, and the hash that freezes it.
//!
//! A project's ground is ONE file (`prelude/Qubitick/HL.lean`) and it is the user's: the
//! engine reads it, imports it, and never writes it after seeding. What ships here are
//! starting points, not a policy:
//!
//! * `init` — `import Init` and nothing else: Lean's own kernel, no Mathlib.
//! * `mathlib` — `import Mathlib` and nothing else.
//! * `bare` — the engine's own prelude: `prelude` mode, the 16-name contract written by
//!   hand, no dependency at all.
//! * `custom` — an empty file, for a project that will write its own.
//!
//! The build does not know which one a project chose — it sees a file, and a hash.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The preset names a project may seed from.
pub const PRESETS: &[&str] = &["init", "mathlib", "bare", "custom"];

/// The Lean text of one preset.
pub fn text(preset: &str) -> Result<&'static str> {
    match preset {
        "init" => Ok("import Init\n"),
        "mathlib" => Ok("import Mathlib\n"),
        "bare" => Ok(crate::ground::HL_LEAN),
        "custom" => Ok(""),
        other => anyhow::bail!(
            "`{other}` is not a preset; expected one of {}",
            PRESETS.join(", ")
        ),
    }
}

/// Where a project's own ground lives, when the charter names a `LeanDir:`.
pub fn ground_file(corpus: &Path, lean_dir: &str) -> PathBuf {
    corpus.join(lean_dir).join("Qubitick").join("HL.lean")
}

/// The bytes the freeze covers: the project's own ground, or the shipped base when the
/// charter names none (which is a corpus nobody seeded, and hashing the base is the
/// honest answer — it says what the build actually used).
pub fn bytes(corpus: &Path) -> Result<Vec<u8>> {
    let canon = crate::canon::read(corpus)?;
    match canon.as_ref().and_then(|c| c.lean_dir.as_ref()) {
        Some(dir) => {
            let path = ground_file(corpus, dir);
            std::fs::read(&path)
                .with_context(|| format!("reading the ground {}", path.display()))
        }
        None => Ok(crate::ground::HL_LEAN.as_bytes().to_vec()),
    }
}

/// The ground's hash, as `qubitick hash-ground` prints it and `--frozen` compares.
pub fn hash(corpus: &Path) -> Result<String> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes(corpus)?.hash(&mut h);
    Ok(format!("{:016x}", h.finish()))
}

/// Write a preset into the project's ground, and name it in the charter.
///
/// Overwriting is the point: seeding is how a project gets a ground at all, and a corpus
/// that already has one is a corpus whose author already answered this question.
pub fn seed(corpus: &Path, preset: &str) -> Result<PathBuf> {
    let body = text(preset)?;
    let path = ground_file(corpus, "prelude");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, body)
        .with_context(|| format!("writing the ground {}", path.display()))?;
    // The charter must point at it, or the build reads the shipped base instead and the
    // user's file is never compiled — silently.
    let charter = corpus.join("Canon.qbit");
    let existing = std::fs::read_to_string(&charter).unwrap_or_default();
    if !existing.lines().any(|l| l.trim_start().starts_with("LeanDir:")) {
        let mut text = existing.clone();
        if !text.ends_with('\n') && !text.is_empty() {
            text.push('\n');
        }
        text.push_str("LeanDir: prelude\n");
        std::fs::write(&charter, text)
            .with_context(|| format!("naming the ground in {}", charter.display()))?;
    }
    Ok(path)
}

/// Refuse a build whose ground is not the one that was frozen.
pub fn check_frozen(corpus: &Path, expected: &str) -> Result<()> {
    match freeze_diagnostic(corpus, expected)? {
        None => Ok(()),
        Some(d) => anyhow::bail!("{}", d.message),
    }
}

/// The refusal as a diagnostic, or `None` when the hashes agree.
///
/// The freeze exists so a proof cannot be finished by editing the ground, and the two
/// hashes are the whole content of the refusal — so they go in the shape a reader can act
/// on, not in a sentence: what changed, and both numbers.
pub fn freeze_diagnostic(
    corpus: &Path,
    expected: &str,
) -> Result<Option<qubitick_model::Diagnostic>> {
    let actual = hash(corpus)?;
    if actual == expected {
        return Ok(None);
    }
    let canon = crate::canon::read(corpus)?;
    let named = canon
        .as_ref()
        .and_then(|c| c.lean_dir.as_ref())
        .map(|d| ground_file(corpus, d).display().to_string())
        .unwrap_or_else(|| "the shipped ground".to_string());
    Ok(Some(
        qubitick_model::Diagnostic::error(
            qubitick_model::Phase::Store,
            "ground.frozen",
            "the ground is not the file that was frozen",
        )
        .label(format!("hash {actual}"))
        .hint(format!("{named} has hash {actual}, and the frozen hash is {expected}"))
        .hint(
            "update the frozen hash if the change is intentional, or revert the ground — the \
             ground is where a corpus's assumptions live, so a proof cannot be finished by \
             editing it",
        ),
    ))
}