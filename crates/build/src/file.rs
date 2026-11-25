//! Single-file build (fast path for IDE saves) (CodeArch §5.12).

use std::path::Path;

use anyhow::Result;
use qubitick_ctx::Ctx;

/// Build a single file end-to-end (parse + write + emit), returning the emitted
/// Lean text. Inputs via `parse_content`, outputs via `emit_lean_text`.
pub fn build_file_lean(ctx: &Ctx, path: &Path) -> Result<String> {
    let src = std::fs::read_to_string(path)?;
    let nodes = qubitick_parse::parse_content(ctx, &src)?;
    let mut rules = qubitick_rewrite::RuleTable::new();
    qubitick_emit::emit_lean_text(ctx, &nodes, "Qubitick.HL", &mut rules)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_error() {
        let ctx = Ctx::from_rows("Era0/Strata0", vec![]);
        let r = build_file_lean(&ctx, Path::new("/nonexistent/nope.qbit"));
        assert!(r.is_err());
    }
}
