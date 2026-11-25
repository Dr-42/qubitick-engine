//! Resolution helpers: symbol-by-id, form-of, row-of.

use qubitick_db::queries::{SymbolRow, symbol_by_tex};
use crate::context::Ctx;

/// Resolve a `SymbolId` to its canonical `SymbolRow` from the context.
pub fn symbol_by_id(ctx: &Ctx, id: i64) -> Option<&SymbolRow> {
    ctx.by_id(id)
}

/// Resolve a `SymbolRow` from the database by its lexical `tex` within a scope.
pub fn row_of(conn: &qubitick_db::Db, scope: &str, tex: &str) -> anyhow::Result<Option<SymbolRow>> {
    symbol_by_tex(conn, scope, tex)
}

/// The kind string of a symbol ('notation', 'macro', 'formation', …), used to pick
/// elaboration rules. Returns an empty string when the symbol is absent.
pub fn form_of(ctx: &Ctx, id: i64) -> String {
    ctx.by_id(id).map(|r| r.kind.clone()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Ctx;

    fn row(id: i64, tex: &str, lean: &str, kind: &str) -> SymbolRow {
        SymbolRow { facets: 0,
            id,
            scope: "".into(),
            tex: tex.into(),
            display: tex.into(),
            lean_name: lean.into(),
            trigger: None,
            kind: kind.into(),
            role: None,
            precedence: None,
            home_node: 1,
            elab: None,
            binder_kind: None,
        }
    }

    #[test]
    fn resolves_by_id() {
        let rows = vec![row(7, "\\x", "X", "notation")];
        let ctx = Ctx::from_rows("Era0/Strata0", rows);
        assert_eq!(symbol_by_id(&ctx, 7).unwrap().lean_name, "X");
        assert_eq!(form_of(&ctx, 7), "notation");
        assert!(symbol_by_id(&ctx, 99).is_none());
    }
}