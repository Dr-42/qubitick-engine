//! Project-wide build (`lake build` path) (CodeArch §5.12).

use std::path::Path;

use anyhow::Result;
use qubitick_ctx::Ctx;

/// Build the whole project rooted at `root`. Returns the build status text.
pub fn build_project(_ctx: &Ctx, root: &Path) -> Result<String> {
    crate::lean_build::lean_build(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn nonexistent_project_errs() {
        let ctx = Ctx::from_rows("Era0/Strata0", vec![]);
        assert!(build_project(&ctx, &PathBuf::from("/nonexistent")).is_err());
    }
}