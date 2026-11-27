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

/// WHAT A CITATION NAMES, as a key: a statement of an axiom node, a definition, or a theorem.
///
/// A definition and a theorem are addressed by the TIER they are numbered in and the number
/// itself — `[D.BT.0.7]` and `[D.QB.0.7]` are two definitions, and a table keyed by the number
/// alone justified rows with whichever one the corpus walked first.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CiteKey {
    /// A statement of an axiom node, 1-based (`A1.1` is line 1).
    AxiomLine { node: u32, line: u32 },
    /// `[D.<tier>.<n>]`.
    Definition { tier: String, n: u32 },
    /// `[T.<tier>.<n>]`.
    Theorem { tier: String, n: u32 },
}

/// Citation → the statements it names.
#[derive(Debug, Clone, Default)]
pub struct RuleTable {
    /* ONE CITATION CAN NAME SEVERAL STATEMENTS. An axiom line and a theorem are one
       statement each; a DEFINITION whose `Sema:` is written as cases
       (`(\tau_0 \oplus b = b) \land ((^+ a) \oplus b = ^+ (a \oplus b))`) is one
       citation — `[D.n]` — over as many equations as the carrier has constructors. Keyed
       by citation, holding every statement the citation covers, so a row over the operator
       is justified by the case that matches it. */
    by_cite: HashMap<CiteKey, Vec<AxRule>>,
}

impl RuleTable {
    pub fn new() -> Self {
        RuleTable::default()
    }

    /// Register the statement at 1-based `line` of axiom `node`.
    pub fn insert(&mut self, node: u32, line: u32, global_index: usize, statement: Expr) {
        self.by_cite.entry(CiteKey::AxiomLine { node, line }).or_default().push(AxRule {
            head: format!("R.ax_{global_index}"),
            statement,
            definitional: false,
        });
    }

    /// Register a definition's unfolding under the tier and number it is cited by —
    /// the `[D.<tier>.<n>]` a proof writes.
    pub fn insert_definition(&mut self, tier: &str, n: u32, statement: Expr) {
        /* APPENDED, not replaced: a case-defined operator registers one rule per section
           under the same `[D.n]`, and the row is justified by whichever case matches. */
        self.by_cite
            .entry(CiteKey::Definition {
                tier: tier.to_string(),
                n,
            })
            .or_default()
            .push(AxRule {
                head: String::new(),
                statement,
                definitional: true,
            });
    }

    /// Register an ASSUMED theorem (cited by `[T.<tier>.<n>]` and declared by name in this
    /// module) under the tier and number it is cited by.
    pub fn insert_theorem(&mut self, tier: &str, n: u32, head: String, statement: Expr) {
        self.by_cite
            .entry(CiteKey::Theorem {
                tier: tier.to_string(),
                n,
            })
            .or_default()
            .push(AxRule {
                head,
                statement,
                definitional: false,
            });
    }

    /// The FIRST statement a citation names. A definition may name several; use
    /// [`RuleTable::all`] where every one of them has to be tried.
    pub fn get(&self, key: &CiteKey) -> Option<AxRule> {
        let mut rules = self.all(key);
        if rules.is_empty() {
            None
        } else {
            Some(rules.remove(0))
        }
    }

    /// EVERY statement a citation names, in registration order.
    ///
    /// A citation that names NO tier (`[D.5]`) matches every tier that numbers a 5 — the same
    /// wildcard a resolution by number has, and the reason a flat corpus keeps working. The
    /// matches are ordered by tier so the answer does not depend on hash order.
    pub fn all(&self, key: &CiteKey) -> Vec<AxRule> {
        if let Some(v) = self.by_cite.get(key) {
            return v.clone();
        }
        let (want_tier, n) = match key {
            CiteKey::Definition { tier, n } => (tier, *n),
            CiteKey::Theorem { tier, n } => (tier, *n),
            CiteKey::AxiomLine { .. } => return Vec::new(),
        };
        if !want_tier.is_empty() {
            return Vec::new();
        }
        let mut hits: Vec<(&String, &Vec<AxRule>)> = self
            .by_cite
            .iter()
            .filter_map(|(k, v)| match k {
                CiteKey::Definition { tier, n: other } if *other == n && key_is_definition(key) => {
                    Some((tier, v))
                }
                CiteKey::Theorem { tier, n: other } if *other == n && key_is_theorem(key) => {
                    Some((tier, v))
                }
                _ => None,
            })
            .collect();
        hits.sort_by(|a, b| a.0.cmp(b.0));
        hits.into_iter().flat_map(|(_, v)| v.iter().cloned()).collect()
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

/// Whether a key names a definition (the empty-tier wildcard only gathers same-kind entries).
fn key_is_definition(key: &CiteKey) -> bool {
    matches!(key, CiteKey::Definition { .. })
}

/// Whether a key names a theorem.
fn key_is_theorem(key: &CiteKey) -> bool {
    matches!(key, CiteKey::Theorem { .. })
}
