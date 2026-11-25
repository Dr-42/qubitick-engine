//! The corpus charter, and the artifact it compiles to.
//!
//! `Canon.qbit` is written by a human: it names the project (LaTeX allowed) and is
//! the one file that says what the corpus *is*. `canon.bit` is written by the
//! engine after a green build: what the corpus *turned out to be* — the node census
//! and the theorem dependency graph, checked for cycles.
//!
//! The two never trade places. A build reads `Canon.qbit` and writes `canon.bit`;
//! it does not rewrite the file a person edits, because a generator that edits its
//! own input makes every build a merge conflict.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// What the charter declares.
#[derive(Debug, Clone)]
pub struct Canon {
    /// The displayed name — LaTeX is fine, nothing resolves against it.
    pub name: String,
    /// The identity: a slug, because scopes and module names are derived from it.
    pub id: String,
    /// `LeanDir:` — the directory holding this project's `Qubitick/*.lean` ground.
    /// Relative to the corpus root; `None` means the shipped prelude.
    ///
    /// This is what makes a project's logic its own: point it at a directory whose
    /// `HL.lean` provides the contract and adds Init, Mathlib, or a previous
    /// development on top, and emission is unchanged (term mode, same contract).
    pub lean_dir: Option<String>,
    /// `Module:` — the module inside `LeanDir` that carries the contract.
    pub lean_module: String,
    /// `Require:` (repeatable) — Lake requirements the package adds, e.g. `mathlib`.
    pub requires: Vec<String>,
    /// `Precedence:` (repeatable) — the master precedence table, ONE LINE PER LEVEL,
    /// **loosest first**. A line is a whitespace-separated list of glyphs written the
    /// way the corpus writes them (`+`, `×`, `\times`, `\le_{\mathbb{Z}}`).
    ///
    /// The charter says what binds tighter than what; a glyph it does not name binds
    /// LOOSER than every glyph it does name. Association is not here: every operator
    /// the table orders is left-associative, and `(a + b) + c = a + (b + c)` is a
    /// theorem a corpus states, not metadata it declares.
    pub precedence: Vec<Vec<String>>,
}

impl Canon {
    /// The level a glyph binds at: 1 is the loosest line, and a higher number binds
    /// tighter. `None` means the charter's table does not name this glyph.
    pub fn level_of(&self, tex: &str) -> Option<i64> {
        self.precedence
            .iter()
            .position(|level| level.iter().any(|g| g == tex))
            .map(|i| i as i64 + 1)
    }
}

/// Read `Canon.qbit` from a corpus root, if it has one.
///
/// Deliberately simple — a header line and `Key: value` fields — so the charter can
/// be written and reviewed without a parser: the first line is `P. <name>`, and
/// `Id:` gives the slug.
pub fn read(corpus: &Path) -> Result<Option<Canon>> {
    // One charter per project, at its root: two would be two claims about what the
    // corpus is, and a charter in a subdirectory would be a claim scoped to nothing.
    let mut found: Vec<PathBuf> = Vec::new();
    collect_charters(corpus, &mut found);
    if found.len() > 1 {
        let list = found
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        anyhow::bail!("a project has exactly one charter; found {}: {list}", found.len());
    }
    if let Some(only) = found.first()
        && only != &corpus.join("Canon.qbit")
    {
        anyhow::bail!(
            "the charter belongs at the corpus root, not at {}",
            only.display()
        );
    }
    let path = corpus.join("Canon.qbit");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    let mut lines = text.lines();
    let header = lines.next().unwrap_or_default().trim();
    let name = header
        .strip_prefix("P.")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(header)
        .to_string();
    let mut id = String::new();
    let mut lean_dir: Option<String> = None;
    let mut lean_module = String::from("Qubitick.HL");
    let mut requires: Vec<String> = Vec::new();
    let mut precedence: Vec<Vec<String>> = Vec::new();
    for line in lines {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Id:") {
            id = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("LeanDir:") {
            let dir = rest.trim().trim_end_matches('/');
            if !dir.is_empty() {
                lean_dir = Some(dir.to_string());
            }
        } else if let Some(rest) = line.strip_prefix("Module:") {
            let module = rest.trim();
            if !module.is_empty() {
                lean_module = module.to_string();
            }
        } else if let Some(rest) = line.strip_prefix("Precedence:") {
            // One line = one level, loosest first; the glyphs on it bind at that level.
            let level: Vec<String> = rest.split_whitespace().map(str::to_string).collect();
            if !level.is_empty() {
                precedence.push(level);
            }
        } else if let Some(rest) = line.strip_prefix("Require:") {
            let requirement = rest.trim();
            if !requirement.is_empty() {
                requires.push(requirement.to_string());
            }
        }
    }
    if id.is_empty() {
        id = name
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_lowercase();
    }
    Ok(Some(Canon {
        name,
        id,
        lean_dir,
        lean_module,
        requires,
        precedence,
    }))
}

/// One directed edge of the theorem graph: `from` cites `to` (both as `T.<n>`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Edge {
    pub from: u32,
    pub to: u32,
}

/// Where the compiled artifact lives (next to the charter, and ignored by git).
pub fn artifact_path(corpus: &Path) -> PathBuf {
    corpus.join("canon.bit")
}

/// True when the theorem graph has no cycle reachable from any node.
///
/// Acyclic is a *computation*, not a proof: this walks the edges. Lean's part is to
/// certify a found cycle (a `Path` term), not to enumerate 300 nodes by hand.
pub fn acyclic(edges: &[Edge]) -> bool {
    let mut adj: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
    for e in edges {
        adj.entry(e.from).or_default().push(e.to);
    }
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        New,
        Open,
        Done,
    }
    let mut state: std::collections::HashMap<u32, Mark> = std::collections::HashMap::new();
    // Iterative DFS: a corpus graph is small, but recursion depth is not worth it.
    for start in adj.keys().copied().collect::<Vec<_>>() {
        if state.get(&start).is_some_and(|m| *m != Mark::New) {
            continue;
        }
        let mut stack: Vec<(u32, usize)> = vec![(start, 0)];
        state.insert(start, Mark::Open);
        while let Some((node, idx)) = stack.pop() {
            let next = adj.get(&node).and_then(|v| v.get(idx)).copied();
            match next {
                Some(to) => {
                    stack.push((node, idx + 1));
                    match state.get(&to).copied().unwrap_or(Mark::New) {
                        Mark::Open => return false, // back edge = cycle
                        Mark::New => {
                            state.insert(to, Mark::Open);
                            stack.push((to, 0));
                        }
                        Mark::Done => {}
                    }
                }
                None => {
                    state.insert(node, Mark::Done);
                }
            }
        }
    }
    true
}

/// Write `canon.bit` for a green build.
///
/// Deterministic on purpose — no timestamps, no paths — so a rebuild that changed
/// nothing produces an identical file and `git status` stays quiet.
pub fn write_artifact(
    corpus: &Path,
    canon: &Canon,
    files: usize,
    verified: usize,
    kinds: &std::collections::BTreeMap<String, usize>,
    edges: &mut Vec<Edge>,
) -> Result<PathBuf> {
    edges.sort();
    edges.dedup();
    let mut dag = String::new();
    for e in edges.iter() {
        dag.push_str(&format!("  {{\"from\": \"T.{}\", \"to\": \"T.{}\"}},\n", e.from, e.to));
    }
    let mut census = String::new();
    for (kind, n) in kinds {
        census.push_str(&format!("    \"{kind}\": {n},\n"));
    }
    let text = format!(
        "{{\n  \"name\": {},\n  \"id\": {},\n  \"files\": {files},\n  \"verified\": {verified},\n  \"kinds\": {{\n{census}  }},\n  \"acyclic\": {},\n  \"edges\": [\n{dag}  ]\n}}\n",
        json_string(&canon.name),
        json_string(&canon.id),
        acyclic(edges),
    );
    let path = artifact_path(corpus);
    // Write only when it changed, for the same reason as the Lake package.
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        std::fs::write(&path, text).with_context(|| format!("write {}", path.display()))?;
    }
    Ok(path)
}

/// Every `Canon.qbit` under `dir` (the charter is one file, so this should find 0 or 1).
fn collect_charters(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().map(|n| n == ".lake").unwrap_or(false) {
                continue; // the emitted package, not the corpus
            }
            collect_charters(&path, out);
        } else if path.file_name().map(|n| n == "Canon.qbit").unwrap_or(false) {
            out.push(path);
        }
    }
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chain_is_acyclic_and_a_loop_is_not() {
        let chain = [Edge { from: 1, to: 2 }, Edge { from: 2, to: 3 }];
        assert!(acyclic(&chain));
        let loop_ = [
            Edge { from: 1, to: 2 },
            Edge { from: 2, to: 3 },
            Edge { from: 3, to: 1 },
        ];
        assert!(!acyclic(&loop_));
        let self_loop = [Edge { from: 7, to: 7 }];
        assert!(!acyclic(&self_loop));
        assert!(acyclic(&[]));
    }

    #[test]
    fn the_charter_is_a_header_and_fields() {
        let dir = std::env::temp_dir().join(format!("canon-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Canon.qbit"),
            "P. qubitick — \\text{The} Corpus\nId: qubitick\nKind: project\n",
        )
        .unwrap();
        let canon = read(&dir).unwrap().expect("charter present");
        assert_eq!(canon.id, "qubitick");
        assert!(canon.name.contains("\\text{The}"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_table_orders_the_glyphs_it_names_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!("canon-prec-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Canon.qbit"),
            "P. school\nId: school\nPrecedence: + -\nPrecedence: × \\times\n",
        )
        .unwrap();
        let canon = read(&dir).unwrap().expect("charter present");
        // Loosest first: `+` is level 1, `×` level 2, and a glyph nothing names is None
        // (which the parser reads as "looser than every level here").
        assert_eq!(canon.level_of("+"), Some(1));
        assert_eq!(canon.level_of("-"), Some(1));
        assert_eq!(canon.level_of("×"), Some(2));
        assert_eq!(canon.level_of("\\times"), Some(2));
        assert_eq!(canon.level_of("\\cap"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corpus_without_a_charter_has_none() {
        let dir = std::env::temp_dir().join(format!("canon-none-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(read(&dir).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}