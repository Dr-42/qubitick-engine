//! `Ctx::build(conn) → Ctx` — construct a context from the database (CodeArch §5.4).

use anyhow::Result;

use crate::context::Ctx;

/// Build a `Ctx` for `scope` by loading all symbols visible there from the corpus
/// database (own scope + global `''` + ancestor scopes).
pub fn build_ctx(conn: &qubitick_db::Db, scope: &str) -> Result<Ctx> {
    let rows = qubitick_db::queries::symbols_in_scope(conn, scope)?;
    Ok(Ctx::from_rows(scope, rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_from_empty_db() {
        let conn = qubitick_db::open_in_memory().unwrap();
        let ctx = build_ctx(&conn, "Era0/Strata0").unwrap();
        assert_eq!(ctx.scope(), "Era0/Strata0");
        assert!(ctx.by_id(1).is_none());
    }
}