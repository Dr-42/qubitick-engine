//! Compiling the corpus document, with the LaTeX engine the machine actually has.
//!
//! `tectonic` is one way to turn these sources into a PDF and it is a good one — it fetches
//! what the document needs and needs no TeX installation. It is not the only way, and it is
//! not installed everywhere: a `/api/pdf` that answers 500 with `failed to spawn the tectonic
//! binary` tells a reader nothing they can act on.
//!
//! So the engines are tried in order — `tectonic`, `pdflatex`, `xelatex` — and the one that
//! runs is the one that answers. When none of them exists, the error names all three and
//! points at `--latex`, which needs no engine at all: the document is the deliverable, and the
//! PDF is one rendering of it.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

/// Which LaTeX engine is installed, in the order they are preferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Tectonic,
    PdfLatex,
    XeLatex,
}

impl Engine {
    pub fn as_str(self) -> &'static str {
        match self {
            Engine::Tectonic => "tectonic",
            Engine::PdfLatex => "pdflatex",
            Engine::XeLatex => "xelatex",
        }
    }
}

/// Every engine, in the order they are preferred.
pub const ENGINES: &[Engine] = &[Engine::Tectonic, Engine::PdfLatex, Engine::XeLatex];

/// Whether a program is on `PATH` and runs.
pub fn installed(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// The first engine this machine has, or a message naming all three.
pub fn find() -> Result<Engine> {
    for engine in ENGINES {
        if installed(engine.as_str()) {
            return Ok(*engine);
        }
    }
    Err(anyhow::anyhow!(
        "no LaTeX engine is installed: tried {}\n  \
         · install one of them (tectonic, texlive-latex-base for pdflatex, texlive-xetex \
         for xelatex)\n  \
         · or ask for the LaTeX source instead — `qubitick pdf <corpus> --latex` writes the \
         document, and any TeX installation renders it",
        ENGINES
            .iter()
            .map(|e| e.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// Render a LaTeX document to PDF bytes.
///
/// `engine` is what [`find`] answered (or what a caller forced); `workdir` is where the
/// source and the by-products go. The document is written once, as `corpus.tex`, and the
/// engine runs in that directory so `\input`, the `.aux` file and the output all agree.
pub fn compile_with(engine: Engine, latex: &str, workdir: &Path) -> Result<Vec<u8>> {
    std::fs::create_dir_all(workdir).with_context(|| format!("mkdir {}", workdir.display()))?;
    let tex_path = workdir.join("corpus.tex");
    std::fs::File::create(&tex_path)?.write_all(latex.as_bytes())?;
    let pdf = match engine {
        Engine::Tectonic => {
            let outdir = workdir.join("out");
            std::fs::create_dir_all(&outdir)?;
            run(
                engine,
                &["--outdir", &outdir.display().to_string(), &tex_path.display().to_string()],
                workdir,
            )?;
            outdir.join("corpus.pdf")
        }
        // `-interaction=nonstopmode` so a warning cannot block on stdin, and the pdf lands
        // beside the source.
        Engine::PdfLatex | Engine::XeLatex => {
            run(
                engine,
                &["-interaction=nonstopmode", "-halt-on-error", "corpus.tex"],
                workdir,
            )?;
            workdir.join("corpus.pdf")
        }
    };
    std::fs::read(&pdf).with_context(|| format!("read {}", pdf.display()))
}

/// Run one engine, and on failure keep its log — the log is the whole diagnosis.
fn run(engine: Engine, args: &[&str], workdir: &Path) -> Result<()> {
    let out = Command::new(engine.as_str())
        .args(args)
        .current_dir(workdir)
        .output()
        .with_context(|| format!("failed to spawn {}", engine.as_str()))?;
    if out.status.success() {
        return Ok(());
    }
    // pdflatex and xelatex write their reasons to stdout; tectonic to stderr as well.
    let mut said = String::from_utf8_lossy(&out.stdout).into_owned();
    said.push_str(&String::from_utf8_lossy(&out.stderr));
    let tail: Vec<&str> = said
        .lines()
        .filter(|l| {
            // the two engines announce errors with a leading `!`
            l.starts_with('!') || l.contains("error") || l.contains("Error")
        })
        .rev()
        .take(6)
        .collect();
    let detail = if tail.is_empty() {
        said.lines().rev().take(6).collect::<Vec<_>>().join("\n")
    } else {
        tail.into_iter().rev().collect::<Vec<_>>().join("\n")
    };
    Err(anyhow::anyhow!(
        "{} exited with {}: {}\n  (source and log are in {})",
        engine.as_str(),
        out.status,
        detail.trim(),
        workdir.display()
    ))
}

/// Compile with whichever engine is installed (see [`find`]).
pub fn compile(latex: &str, workdir: &Path) -> Result<Vec<u8>> {
    compile_with(find()?, latex, workdir)
}

/// Default work directory for temporary build files.
pub fn default_workdir() -> PathBuf {
    std::env::temp_dir().join("qubitick_pdf")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_engines_are_tried_in_a_fixed_order() {
        // The order is the promise: a machine with both tectonic and pdflatex gets tectonic,
        // because it needs no TeX installation and no server-side package set.
        assert_eq!(
            ENGINES.iter().map(|e| e.as_str()).collect::<Vec<_>>(),
            vec!["tectonic", "pdflatex", "xelatex"]
        );
    }

    #[test]
    fn a_missing_engine_is_named_with_the_source_fallback() {
        // `find` on a machine with none of the three says WHICH three, and what to do
        // instead — the `--latex` source needs no engine at all.
        if ENGINES.iter().any(|e| installed(e.as_str())) {
            eprintln!("skipping: this machine has a LaTeX engine");
            return;
        }
        let e = find().unwrap_err().to_string();
        for name in ["tectonic", "pdflatex", "xelatex", "--latex"] {
            assert!(e.contains(name), "{name} is named: {e}");
        }
    }

    #[test]
    fn a_failing_engine_keeps_its_log() {
        // A compile that fails must say WHY, not just "exited with status 1". The message
        // carries the source directory, because the `.log` file beside it is the rest.
        if !installed("pdflatex") {
            eprintln!("skipping: pdflatex not installed");
            return;
        }
        let dir = std::env::temp_dir().join(format!("qb-pdf-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let e = compile_with(Engine::PdfLatex, "\\documentclass{article}\\begin{document}\\undefined", &dir)
            .unwrap_err()
            .to_string();
        assert!(e.contains("pdflatex"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
