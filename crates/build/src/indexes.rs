//! Build derived edges, autocite index, and usage counts (CodeArch §5.12).

use anyhow::Result;
use qubitick_db::Db;

/// Rebuild derived indexes for a node. This is a stub for the full index pass;
/// it returns the count of rows the full pass would compute (symbol usage).
pub fn build_indexes(conn: &Db) -> Result<i64> {
    let usage = qubitick_db::queries::symbol_usage_counts(conn)?;
    Ok(usage.len() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_db_has_zero_index() {
        let conn = qubitick_db::open_in_memory().unwrap();
        assert_eq!(build_indexes(&conn).unwrap(), 0);
    }
}