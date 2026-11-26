//! `qubitick scaffold` — the ground a corpus's own vocabulary implies.
//!
//! An A block is grammar, and its terminals have to exist somewhere: a `Type:` names a
//! ground symbol, and the law an A block states becomes a field of the class the proofs
//! live in. The engine writes none of that — it is the user's file — so this produces a
//! SUGGESTION: one Lean text carrying the corpus's frame (its definitions and its sorts),
//! the root class, and one class per A block with that block's statements as fields.
//!
//! The output is a starting point, not a contract. A project drops it into its own
//! `HL.lean` under the contract preset, then edits freely: the build never regenerates
//! it and never checks it against the corpus's blocks.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use qubitick_model::Node;

/// The corpus's ground, as Lean: the root class carrying the symbols it only names, and
/// one class per A block carrying that block's statements as fields.
pub fn scaffold(conn: &qubitick_db::Db, corpus: &Path, namespace: &str) -> Result<String> {
    let (ctx, nodes) = corpus_ledger(conn, corpus)?;
    let text = qubitick_emit::emit_ground_chain(&ctx, &nodes, namespace)?;
    Ok(text.trim_end().to_string())
}

/// Every node of a corpus, and the context it is written in.
///
/// Parsed from SOURCE: a definition's authored `Type:` lives only there, never in the
/// DB-rebuilt form, and the frame types a glyph from exactly that.
pub fn corpus_ledger(
    conn: &qubitick_db::Db,
    corpus: &Path,
) -> Result<(qubitick_ctx::Ctx, Vec<Node>)> {
    let files = super::walk_qbit(corpus);
    let scope = files
        .first()
        .map(|f| super::pipeline::scope_of_path(f))
        .unwrap_or_else(|| "Era0/Strata0".into());
    let ctx = qubitick_ctx::build_ctx(conn, &scope)?;
    let mut nodes: Vec<Node> = Vec::new();
    let mut ctxs: HashMap<String, qubitick_ctx::Ctx> = HashMap::new();
    for file in &files {
        let file_scope = super::pipeline::scope_of_path(file);
        if !ctxs.contains_key(&file_scope) {
            ctxs.insert(file_scope.clone(), qubitick_ctx::build_ctx(conn, &file_scope)?);
        }
        let Ok(src) = std::fs::read_to_string(file) else {
            continue;
        };
        let Ok(parsed) = qubitick_parse::parse_content(&ctxs[&file_scope], &src) else {
            continue;
        };
        nodes.extend(parsed);
    }
    Ok((ctx, nodes))
}
