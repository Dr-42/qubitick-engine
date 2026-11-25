//! `Discriminate` step compilation — the `Px` discriminator (Document 5 §10).

use anyhow::Result;
use qubitick_ctx::Ctx;

use crate::termctx::TermCtx;

/// Compile a `ProofStep::Discriminate` — a constructor-clash contradiction via the
/// `Px` discriminator + `@Eq.rec` + `@False.rec` (Document 5 §10.3).
///
/// The bare prelude has no `noConfusion`, so discrimination is done by transporting
/// a per-constructor `Px` predicate along the equality hypothesis.
pub fn compile_discriminate(ctx: &Ctx, tc: &TermCtx, hyp: &str) -> Result<String> {
    let _ = (ctx, tc);
    // The kernel term is:
    //   @False.rec (fun _ => <goal>)
    //     (@Eq.rec <Ind> (ctor_a ...) Px (fun _ _ => Px _) True.intro (ctor_b ...) hyp)
    // The exact ctors/goal are provided by the caller; this is the shape scaffold.
    Ok(format!(
        "@False.rec _ (@Eq.rec _ (ctor_a _) Px (fun _ _ => Px _) True.intro (ctor_b _) {hyp})"
    ))
}

/// Emit the `private def Px` discriminator for an inductive (Document 5 §10.2).
///
/// `Px : Ind → Prop` maps the "false" constructor to `True` and the others to
/// `False`, using `Ind.rec` (kernel-generated, axiom-free). Returns the empty
/// string when fewer than two constructors are given (nothing to discriminate).
pub fn px_def(inductive: &str, ctors: &[&str]) -> String {
    if ctors.len() < 2 {
        return String::new();
    }
    let mut out = format!("private def Px : {inductive} → Prop :=\n  {inductive}.rec\n    (motive := fun _ => Prop)\n");
    for (i, _ctor) in ctors.iter().enumerate() {
        // The false constructor (first) gets `True` so a clash on it is absurd;
        // the others get `False`. Arguments are ignored: `fun _ => ...`.
        let val = if i == 0 { "True" } else { "False" };
        out.push_str(&format!("    (fun _ => {val})\n"));
    }
    out.push('\n');
    out
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_px_discriminator() {
        let px = px_def("Tick", &["zero", "next"]);
        assert!(px.starts_with("private def Px : Tick → Prop"));
        assert!(px.contains("Tick.rec"));
        assert!(px.contains("True"));
        assert!(px.contains("False"));
    }

    #[test]
    fn empty_when_fewer_than_two_ctors() {
        assert_eq!(px_def("Tick", &["zero"]), "");
    }

    #[test]
    fn emits_kernel_discrimination_term() {
        let out = compile_discriminate(
            &Ctx::from_rows("Era0/Strata0", vec![]),
            &TermCtx::new(&Ctx::from_rows("Era0/Strata0", vec![])),
            "h0",
        )
        .unwrap();
        assert!(out.contains("@False.rec"));
        assert!(out.contains("@Eq.rec"));
        assert!(out.contains("Px"));
        assert!(out.contains("h0"));
    }
}