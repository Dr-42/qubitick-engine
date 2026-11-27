//! The axiom rewrite-rule table used to justify proof-chain rows.
//!
//! A chain row `lhs = rhs` citing `[A1.1]` is justified by instantiating that
//! axiom's statement: `R.ax_k a b c`, where `k` is the statement's **cumulative**
//! index across the axiom chain (Document 5 §5.1/§5.2, §6.1) and `a b c` are the
//! values bound when the statement's left-hand side is matched against `lhs`.

use std::collections::HashMap;

use qubitick_model::{Expr, BinderKind};

/// One citable axiom statement.
#[derive(Debug, Clone)]
pub struct AxRule {
    /// The Lean head to apply — a class field `R.ax_<k>`, or the name of a
    /// declared (assumed) theorem. Unused for definitional rules.
    pub head: String,
    /// The statement as written, including its `∀` binder prefix.
    pub statement: Expr,
    /// A DEFINITION: the step is the definition's unfolding, closed by `Eq.refl`
    /// (Lean's kernel conversion), not by applying a class field.
    pub definitional: bool,
}

/// Citation `(axiom node id, corpus line)` → the axiom statement it names.
///
/// `line` is the 1-based statement index used by the corpus (`A1.1` is line 1).
#[derive(Debug, Clone, Default)]
pub struct RuleTable {
    /* ONE CITATION CAN NAME SEVERAL STATEMENTS. An axiom line and a theorem are one
       statement each; a DEFINITION whose `Sema:` is written as cases
       (`(\tau_0 \oplus b = b) \land ((^+ a) \oplus b = ^+ (a \oplus b))`) is one
       citation — `[D.n]` — over as many equations as the carrier has constructors. Keyed
       by citation, holding every statement the citation covers, so a row over the operator
       is justified by the case that matches it. */
    by_cite: HashMap<(u32, u32), Vec<AxRule>>,
}

impl RuleTable {
    pub fn new() -> Self {
        RuleTable::default()
    }

    /// Register the statement at 1-based `line` of axiom `node`.
    pub fn insert(&mut self, node: u32, line: u32, global_index: usize, statement: Expr) {
        self.by_cite.entry((node, line)).or_default().push(AxRule {
            head: format!("R.ax_{global_index}"),
            statement,
            definitional: false,
        });
    }

    /// Register a definition's unfolding under its corpus division index — the
    /// number `n` in a `[D.BT.<n>]` citation. Keyed as node `0`.
    pub fn insert_definition(&mut self, division: u32, statement: Expr) {
        /* APPENDED, not replaced: a case-defined operator registers one rule per section
           under the same `[D.n]`, and the row is justified by whichever case matches. */
        self.by_cite.entry((0, division)).or_default().push(AxRule {
            head: String::new(),
            statement,
            definitional: true,
        });
    }

    /// Register an ASSUMED theorem (cited by `[T.BT.<n>]` and declared by name in
    /// this module) under its corpus division index. Keyed as node `u32::MAX`.
    pub fn insert_theorem(&mut self, division: u32, head: String, statement: Expr) {
        self.by_cite.entry((u32::MAX, division)).or_default().push(AxRule {
            head,
            statement,
            definitional: false,
        });
    }
    /// The FIRST statement a citation names. A definition may name several; use
    /// [`RuleTable::all`] where every one of them has to be tried.
    pub fn get(&self, node: u32, line: u32) -> Option<&AxRule> {
        self.by_cite.get(&(node, line)).and_then(|v| v.first())
    }

    /// EVERY statement a citation names, in registration order.
    pub fn all(&self, node: u32, line: u32) -> &[AxRule] {
        self.by_cite.get(&(node, line)).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn is_empty(&self) -> bool {
        self.by_cite.is_empty()
    }
}

/// Strip an axiom statement's `∀` binder prefix.
///
/// Returns the bound variable names (outermost group first, in order) and the
/// body the statement asserts.
pub fn strip_binders(e: &Expr) -> (Vec<String>, Expr) {
    let mut vars = Vec::new();
    let mut cur = e.clone();
    loop {
        let next = match cur {
            Expr::Binder(BinderKind::Forall, binders, body) => {
                for b in binders {
                    vars.extend(b.vars.iter().cloned());
                }
                *body
            }
            other => return (vars, other),
        };
        cur = next;
    }
}
