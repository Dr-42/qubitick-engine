//! PDF generation orchestration: DB → LaTeX → PDF bytes (CodeArch §5.7).

use std::path::Path;

use anyhow::Result;
use qubitick_db::read::get_node_list;

use crate::latex;
use crate::tectonic;

/// Generate a PDF of the whole corpus, returning its bytes.
///
/// Reads the corpus node list from the DB, emits a LaTeX summary document, and
/// compiles it with the `tectonic` binary into `workdir`.
pub fn render_pdf(conn: &qubitick_db::Db, workdir: &Path) -> Result<Vec<u8>> {
    let nodes = get_node_list(conn)?;
    let doc = latex::document(&nodes);
    tectonic::compile(&doc, workdir)
}

/// The corpus as a document, from material the build crate assembled (see
/// `qubitick_pdf::latex::document_sections` for what the material is).
pub fn render_sections_pdf(sections: &[(String, Vec<String>)], workdir: &Path) -> Result<Vec<u8>> {
    let doc = latex::document_sections(sections);
    tectonic::compile(&doc, workdir)
}

/// Convenience wrapper using the default temporary work directory.
pub fn render_pdf_default(conn: &qubitick_db::Db) -> Result<Vec<u8>> {
    render_pdf(conn, &tectonic::default_workdir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_empty_db_to_a_document() {
        let conn = qubitick_db::open_in_memory().unwrap();
        let nodes = get_node_list(&conn).unwrap();
        let doc = latex::document(&nodes);
        assert!(doc.contains("\\documentclass"));
        // tectonic compile is not run in unit tests (needs the binary + network).
    }
}