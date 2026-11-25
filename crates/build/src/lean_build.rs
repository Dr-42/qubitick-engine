//! Invoke `lake build` on the whole project (CodeArch §5.12).

use std::path::Path;

use anyhow::{Context, Result};

/// Run `lake build` in `root`. Returns the annotation-timestamp/status line on
/// success or the captured error output on failure. Lean/Lake are provided via
/// nix-portable.
pub fn lean_build(root: &Path) -> Result<String> {
    let out = std::process::Command::new("lake")
        .arg("build")
        .current_dir(root)
        .output()
        .context("failed to spawn `lake build`")?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    if out.status.success() {
        Ok(text)
    } else {
        anyhow::bail!("lake build failed:\n{}", text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn nonexistent_root_errs() {
        let r = lean_build(&PathBuf::from("/nonexistent"));
        assert!(r.is_err());
    }
}