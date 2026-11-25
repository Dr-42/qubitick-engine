//! Connection open + engine configuration (SymbolDB §2).

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::Connection;

use crate::schema;

/// Open (creating if needed) the corpus database at `path`, apply migrations,
/// and configure the SQLite engine per SymbolDB §2.
pub fn open<P: AsRef<Path>>(path: P) -> Result<Connection> {
    let display = path.as_ref().display();
    let conn = Connection::open(&path)
        .with_context(|| format!("open DB at {display}"))?;
    configure(&conn)?;
    schema::migrate(&conn)?;
    Ok(conn)
}

/// Open an in-memory database with the same configuration (used in tests).
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    configure(&conn)?;
    schema::migrate(&conn)?;
    Ok(conn)
}

/// Apply the SQLite engine settings from SymbolDB §2.
fn configure(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.pragma_update(None, "cache_size", -20000)?; // 20 MB
    conn.busy_timeout(std::time::Duration::from_millis(5000))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_and_migrates() {
        let conn = open_in_memory().unwrap();
        let version: i64 = conn
            .query_row("SELECT COALESCE(MAX(version),0) FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            version,
            crate::schema::SCHEMA_VERSION,
            "opening a DB leaves it at the current schema version"
        );
        let fk: i64 = conn
            .pragma_query_value(None, "foreign_keys", |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1, "foreign_keys should be ON");
    }
}