//! TermCtx — symbol resolution for Lean rendering.

use qubitick_ctx::Ctx;

/// Resolves a `SymbolId` to its Lean name for `qubitick_lean::render`.
pub struct TermCtx<'a> {
    pub ctx: &'a Ctx,
    fn_names: &'a std::collections::HashMap<u32, String>,
}

static EMPTY_FN: std::sync::LazyLock<std::collections::HashMap<u32, String>> =
    std::sync::LazyLock::new(std::collections::HashMap::new);

impl<'a> TermCtx<'a> {
    pub fn new(ctx: &'a Ctx) -> Self {
        TermCtx {
            ctx,
            fn_names: &EMPTY_FN,
        }
    }

    pub fn with_fn(
        ctx: &'a Ctx,
        fn_names: &'a std::collections::HashMap<u32, String>,
    ) -> Self {
        TermCtx { ctx, fn_names }
    }
    /// Resolve a symbol id to its Lean name; falls back to a placeholder.
    /// Resolve a symbol id to its Lean name, preferring the polysymbol
    /// function-variant map (`X` → `X_fn` for applied uses).
    pub fn resolve(&self, id: u32) -> String {
        self.fn_names
            .get(&id)
            .cloned()
            .unwrap_or_else(|| {
                self.ctx
                    .by_id(id as i64)
                    .map(|r| r.lean_name.clone())
                    .unwrap_or_else(|| format!("s{id}"))
            })
    }
}