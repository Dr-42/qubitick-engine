//! `qubitick-build` — pipeline orchestration (L6).
//!
//! parse → write ASTs → hash → index → emit Lean → lean check/build.
//! Depends on `model`, `db`, `parse`, `emit`.

pub mod axioms;
pub mod canon;
pub mod cycles;
pub mod cite_rules;
pub mod cite_target;
pub mod coverage;
pub mod row_check;
pub mod cites;
pub mod file;
pub mod ground;
pub mod hashes;
pub mod ingest;
pub mod lake;
pub mod latex;
pub mod lean_check;
pub mod lean_build;
pub mod pdf_doc;
pub mod pipeline;
pub mod prelude_contract;
pub mod project;

pub use axioms::load_theorem;
pub use axioms::{cited_divisions, load_axiom_nodes};
pub use canon::{Canon, Edge, read as read_canon, write_artifact};
pub use cycles::{CycleMember, DefinitionCycle, definition_cycles};
pub use cite_rules::{CiteRule, cite_rules};
pub use cite_target::{CiteTarget, is_local, label_of, label_of_line, resolve_cite, rule_of_cite};
pub use row_check::{RowCheck, check_nodes as check_rows};
pub use ingest::{Claimed, Duplicate, FileSymbols, IngestReport, ingest_corpus, ingest_file_symbols, walk_qbit};
pub use pdf_doc::{PdfSection, sections as pdf_sections};
pub use pipeline::{BuildFileResult, admitted_count, build_file, emit_file, run_pipeline, warning_count};