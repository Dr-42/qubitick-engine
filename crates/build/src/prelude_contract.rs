//! The Lean contract a project's own ground must satisfy.
//!
//! Emission is term mode and stays that way: the modules we emit name a small,
//! fixed set of declarations and nothing else. That set is the *contract*, and it is
//! what makes `HL.lean` a project asset rather than app code — a project may put
//! anything it likes on top (Init, Mathlib, its own previous development), as long
//! as the base below is there.
//!
//! Two tiers, because they fail differently:
//!
//!   · `Kernel` — `Eq` and its three lemmas. The kernel recognises `Eq`/`Eq.refl` by
//!     NAME at root, so a project cannot rename or namespace these away; it may only
//!     supply them.
//!   · `Logic` — the Prop toolkit the emitter uses when a corpus needs it (`False`,
//!     `True`, `Not`, `And`, `Or`, `Exists`, `Iff`) and the decidability vocabulary
//!     (`Bool`, `Decidable`, `DecidableEq`).
//!   · `Codegen` — the two `unsafe axiom` markers Lean's own `Init` declares for
//!     erased parameters. A parameterized inductive or structure in an emitted module
//!     does not elaborate without them.
//!
//! What the shipped corpus actually uses is narrower than the contract (it references
//! `Eq`, `Eq.refl`, `Eq.symm`, `Eq.trans` and `True`); the rest is there because
//! emission is data-driven and another corpus will reach for it.

/// What a missing declaration breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// The kernel looks these up by name; they live at root.
    Kernel,
    /// The Prop toolkit the emitter may reach for.
    Logic,
    /// `lcErased` / `lcAny`, needed to elaborate erased parameters.
    Codegen,
}

impl Tier {
    pub fn label(self) -> &'static str {
        match self {
            Tier::Kernel => "kernel",
            Tier::Logic => "logic",
            Tier::Codegen => "codegen",
        }
    }
}

/// Every declaration an emitted module may name, at the tier that explains it.
pub const REQUIRED: &[(&str, Tier)] = &[
    ("Eq", Tier::Kernel),
    ("Eq.refl", Tier::Kernel),
    ("Eq.symm", Tier::Kernel),
    ("Eq.trans", Tier::Kernel),
    ("False", Tier::Logic),
    ("True", Tier::Logic),
    ("Not", Tier::Logic),
    ("And", Tier::Logic),
    ("Or", Tier::Logic),
    ("Exists", Tier::Logic),
    ("Iff", Tier::Logic),
    ("Bool", Tier::Logic),
    ("Decidable", Tier::Logic),
    ("DecidableEq", Tier::Logic),
    ("lcErased", Tier::Codegen),
    ("lcAny", Tier::Codegen),
];

/// A Lean module that references every contract name, so `lean` can say which are
/// missing. `prelude` first: the contract is about what the module can see, and the
/// probe must not smuggle in `Init` behind the project's back.
pub fn probe(import_module: &str) -> String {
    let mut out = String::from("prelude\n");
    if !import_module.is_empty() {
        out.push_str(&format!("import {import_module}\n"));
    }
    for (name, _tier) in REQUIRED {
        out.push_str(&format!("#check {name}\n"));
    }
    out
}

/// Read `lean`'s answer to [`probe`] and return the names it could not find.
///
/// Lean 4.35 reports `` Unknown identifier `X` `` (capital U, backticks) and tags it
/// `error(lean.unknownIdentifier)`; older spellings use single quotes. Rather than
/// depend on one of those, every identifier named on an "unknown" line is collected
/// and intersected with the contract — so a change of wording cannot make the check
/// silently pass.
pub fn missing_from(lean_output: &str) -> Vec<String> {
    let mut named: Vec<String> = Vec::new();
    for line in lean_output.lines() {
        let lower = line.to_ascii_lowercase();
        if !lower.contains("unknown") {
            continue;
        }
        for quote in ['`', '\''] {
            let mut rest = line;
            while let Some(start) = rest.find(quote) {
                let tail = &rest[start + 1..];
                let Some(end) = tail.find(quote) else { break };
                named.push(tail[..end].to_string());
                rest = &tail[end + 1..];
            }
        }
    }
    let mut missing: Vec<String> = Vec::new();
    for (name, _tier) in REQUIRED {
        let bare = name.rsplit('.').next().unwrap_or(name);
        if named.iter().any(|n| n == name || n == bare) && !missing.iter().any(|m| m == name) {
            missing.push((*name).to_string());
        }
    }
    missing
}

/// The tier of a contract name, for a report.
pub fn tier_of(name: &str) -> Option<Tier> {
    REQUIRED
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| *t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_probe_names_every_required_declaration() {
        let probe = probe("Qubitick.HL");
        assert!(probe.starts_with("prelude\n"));
        assert!(probe.contains("import Qubitick.HL\n"));
        for (name, _) in REQUIRED {
            assert!(probe.contains(&format!("#check {name}\n")), "{name}");
        }
    }

    #[test]
    fn a_missing_name_is_reported_from_leans_output() {
        let out = "probe.lean:12:0: error(lean.unknownIdentifier): Unknown identifier `Exists`";
        let missing = missing_from(out);
        assert_eq!(missing, vec!["Exists".to_string()]);
        assert_eq!(tier_of("Exists"), Some(Tier::Logic));
    }

    #[test]
    fn a_clean_check_reports_nothing() {
        assert!(missing_from("").is_empty());
    }

    /// The shipped prelude satisfies its own contract, asked of LEAN rather than of
    /// the source text: `Eq.symm` is a projection the source never spells out, so a
    /// grep would be wrong in both directions. Skipped where there is no `lean`.
    #[test]
    fn our_prelude_is_a_complete_ground() {
        if std::process::Command::new("lean")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }
        let dir = std::env::temp_dir().join(format!("ground-probe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        crate::ground::materialise(&dir).unwrap();
        crate::lean_check::ensure_prelude(&dir).unwrap();
        std::fs::write(dir.join("Contract.lean"), probe("Qubitick.HL")).unwrap();
        let out = std::process::Command::new("lean")
            .arg("Contract.lean")
            .current_dir(&dir)
            .env("LEAN_PATH", &dir)
            .output()
            .unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let missing = missing_from(&text);
        assert!(
            missing.is_empty() && out.status.success(),
            "the shipped ground must satisfy its own contract; missing {missing:?}\n{text}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
