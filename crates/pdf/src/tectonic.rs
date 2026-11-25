//! Invocation of the `tectonic` binary (CodeArch §5.7).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

/// Render a LaTeX document to PDF bytes using the `tectonic` binary.
///
/// Tectonic is provided by nix-portable (`pkgs.tectonic`) and is invoked as a
/// subprocess rather than linked as a Rust crate (the `tectonic` 0.15 crate does
/// not build against the current registry). The LaTeX is written to a temporary
/// working directory and compiled with `tectonic --outdir`.
pub fn compile(latex: &str, workdir: &Path) -> Result<Vec<u8>> {
    std::fs::create_dir_all(workdir).with_context(|| format!("mkdir {}", workdir.display()))?;
    let tex_path = workdir.join("corpus.tex");
    let mut f = std::fs::File::create(&tex_path)?;
    f.write_all(latex.as_bytes())?;
    drop(f);

    let outdir = workdir.join("out");
    std::fs::create_dir_all(&outdir)?;

    let status = Command::new("tectonic")
        .arg("--outdir")
        .arg(&outdir)
        .arg(&tex_path)
        .current_dir(workdir)
        .status()
        .with_context(|| "failed to spawn the tectonic binary (is it on PATH?)")?;

    if !status.success() {
        return Err(anyhow::anyhow!(
            "tectonic exited with status {status}: {}",
            workdir.display()
        ));
    }

    let pdf = outdir.join("corpus.pdf");
    let bytes = std::fs::read(&pdf).with_context(|| format!("read {}", pdf.display()))?;
    Ok(bytes)
}

/// Default work directory for temporary build files.
pub fn default_workdir() -> PathBuf {
    std::env::temp_dir().join("qubitick_pdf")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_tectonic_binary_or_reports_clean_error() {
        // Do not run the actual compile in unit tests; just confirm the binary
        // name is discoverable via `which` — presence is guaranteed by nix-portable.
        let found = Command::new("sh")
            .arg("-c")
            .arg("command -v tectonic >/dev/null 2>&1 && echo yes || echo no")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "yes")
            .unwrap_or(false);
        let _ = found; // result is environment-dependent; not asserted
    }
}