//! Import assembly (CodeArch §5.11).

use qubitick_lean::LeanTarget;

/// Assemble a comma-separated import list from the definitions referenced while
/// emitting a module. Starts with the shared prelude imports.
pub fn assemble_imports(target: &LeanTarget) -> String {
    let mut imports = vec!["Qubitick.HL".to_string()];
    for ext in target.external_uses() {
        // Derive a module path from a dotted external name.
        let module = ext.split('.').next().unwrap_or(ext);
        let candidate = format!("Qubitick.{module}");
        if candidate != "Qubitick.HL" && !imports.contains(&candidate) {
            imports.push(candidate);
        }
    }
    imports.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_start_with_core() {
        let t = LeanTarget::new("Qubitick.HL");
        let out = assemble_imports(&t);
        assert!(out.starts_with("Qubitick"));
    }
}