//! `qubitick-autocite` — the autocite search engine (L4).
//!
//! Given `(lhs, rhs)` against a `Ctx`, find a chain of cited rewrite lemmas that
//! bridges `lhs` to `rhs`, or the closest reachable state plus the missing gap.

use std::sync::Mutex;

use qubitick_ctx::Ctx;
use qubitick_model::Expr;

pub mod cache;
pub mod candidates;
/// The editor's two questions: what can follow this expression, and does this row follow.
pub mod rows;
pub mod distance;
pub mod index;
pub mod search;

use cache::AutociteCache;

/// Result of an autocite query.
#[derive(Debug, Clone)]
pub struct AutociteResult {
    pub lhs: Expr,
    pub rhs: Expr,
    /// The sequence of intermediate states bridging lhs → rhs.
    pub chain: Vec<Expr>,
    /// Whether a full bridge was found.
    pub reached: bool,
    /// On failure: the state closest to `rhs` reachable from `lhs`.
    pub closest: Option<Expr>,
    /// On failure: the differing leaves of `closest` vs `rhs`.
    pub gap: Option<Vec<(String, String)>>,
}

/// Find an autocite chain from `lhs` to `rhs`.
///
/// The cache memoizes by `(hash_expr(lhs), hash_expr(rhs))` so repeated queries
/// are O(1). The search itself is a bounded bidirectional BFS (see `search`).
pub fn find(ctx: &Ctx, lhs: &Expr, rhs: &Expr) -> AutociteResult {
    let mut cache = cache_guard().lock().unwrap_or_else(|p| p.into_inner());
    if let Some(hit) = cache.get(lhs, rhs) {
        return hit.clone();
    }

    let result = if qubitick_pattern::expr_norm_eq(lhs, rhs) {
        AutociteResult {
            lhs: lhs.clone(),
            rhs: rhs.clone(),
            chain: vec![lhs.clone()],
            reached: true,
            closest: None,
            gap: None,
        }
    } else {
        search::run(ctx, lhs, rhs)
    };

    cache.insert(lhs, rhs, result.clone());
    result
}

/// Find a bridge from `lhs` to `rhs` by applying **citable equations**, and say
/// which citation justified each step.
///
/// This is the command an author wants: "which rows take me from here to there".
/// The rules are the corpus's own equations (`A*`, `D.*`, `T.*`); building them is
/// the caller's job (`qubitick_build::cite_rules`), because it needs the DB.
pub fn find_with_rules(
    lhs: &Expr,
    rhs: &Expr,
    rules: &[(String, qubitick_rewrite::diff::RewriteRule)],
) -> search::CitedChain {
    if qubitick_pattern::expr_norm_eq(lhs, rhs) {
        return search::CitedChain {
            reached: true,
            steps: vec![search::CitedStep {
                expr: lhs.clone(),
                cite: None,
            }],
            closest: None,
            gap: None,
        };
    }
    search::run_ruled(lhs, rhs, rules)
}

/// An expression in the corpus's OWN notation (`\Delta b a`), for whoever shows the answer:
/// the route (`crates/api/src/handlers.rs::post_autocite`), the CLI (`qubitick autocite`) and
/// anything that prints a chain.
///
/// A chain a reader cannot read is a chain they cannot check, and `Expr`'s own `Debug` prints
/// `Apply(AtomicComposite { primary: 178, ... })` — which is what the route used to answer
/// `closest` with. The symbols' tex is what the corpus writes, and it is also what typesets, so
/// both callers show the same string for the same state. A symbol the context does not hold is
/// shown as `#<id>` rather than dropped: an unreadable leaf in a chain is still a fact about it.
pub fn show(ctx: &Ctx, e: &Expr) -> String {
    let tex: std::collections::HashMap<u32, String> = ctx
        .symbols()
        .map(|r| (r.id as u32, r.tex.clone()))
        .collect();
    qubitick_lean::render(e, &|id| {
        tex.get(&id).cloned().unwrap_or_else(|| format!("#{id}"))
    })
}

fn cache_guard() -> &'static Mutex<AutociteCache> {
    static CACHE: std::sync::OnceLock<Mutex<AutociteCache>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(AutociteCache::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Ctx {
        Ctx::from_rows("Era0/Strata0", vec![])
    }

    #[test]
    fn identical_reaches_immediately() {
        let ctx = ctx();
        let e = Expr::Sym(qubitick_model::AtomicComposite::new(1));
        let r = find(&ctx, &e, &e);
        assert!(r.reached);
    }

    /// A symbol row, as `Ctx` reads them: the id, its tex and (for `Ctx`'s own carrier
    /// detection) a lean name.
    fn row(id: i64, tex: &str, lean_name: &str) -> qubitick_db::queries::SymbolRow {
        qubitick_db::queries::SymbolRow {
            id,
            scope: "Era0/Strata0".to_string(),
            tex: tex.to_string(),
            display: tex.to_string(),
            lean_name: lean_name.to_string(),
            trigger: None,
            kind: "symbol".to_string(),
            role: None,
            precedence: None,
            home_node: 0,
            elab: None,
            binder_kind: None,
            facets: 0,
        }
    }

    #[test]
    fn an_expression_shows_in_the_corpus_s_notation_not_as_debug_output() {
        let ctx = Ctx::from_rows(
            "Era0/Strata0",
            vec![row(7, "\\Delta", "Delta"), row(8, "a", "a")],
        );
        let a = Expr::Sym(qubitick_model::AtomicComposite::new(8));

        assert_eq!(show(&ctx, &a), "a", "a leaf is the tex the corpus writes");
        /* the fallback matters: a chain may name a symbol this context does not hold, and an
           answer that panics or prints nothing loses the fact that the leaf was there */
        let unknown = Expr::Sym(qubitick_model::AtomicComposite::new(999));
        assert_eq!(show(&ctx, &unknown), "#999");
        assert!(
            !show(&ctx, &a).contains("AtomicComposite"),
            "and never the model's debug shape"
        );
    }
}
