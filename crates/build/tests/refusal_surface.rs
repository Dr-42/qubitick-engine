//! HOSTILE INPUT IS REFUSED BY NAME, AND NEVER PANICS.
//!
//! The engine reads files a person wrote. Every shape below is one somebody can plausibly
//! type — a keyword for a glyph, a name the ground already owns, a body split over two lines,
//! a `Sema:` whose cases are not parenthesised, a citation of a node that does not exist —
//! and the contract is the same for all of them:
//!
//!   * nothing PANICS (a panic is not a diagnostic; it takes the server down with it),
//!   * nothing is silently dropped or silently invented, and
//!   * a corpus that is legal in every one of those shapes still builds and verifies.
//!
//! The two defects this file was written after were exactly of that kind: a parenthesised-free
//! case chain handed the format's `=` to the name derivation and overflowed (a Rust panic on
//! corpus input), and a second bare line in a definition was dropped without a word (the
//! operator then emitted as an assumption).

use std::path::{Path, PathBuf};

struct Outcome {
    /// Every file verified.
    all_verified: bool,
    /// `(code, message)` for every error diagnostic.
    errors: Vec<(String, String)>,
    /// The text a file emitted, by file name (only for the legal shapes).
    emitted: Vec<(String, String)>,
}

fn corpus(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qb-refuse-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Canon.qbit"), format!("P. {tag}\nId: {tag}\n")).unwrap();
    for (rel, text) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

fn run(dir: &Path, emit: &[&str]) -> Outcome {
    // A REFUSAL is decided before Lean: the diagnostics this file is about come from the
    // parser, the coverage pass, and the emitter, so the refusal cases skip compiling (the
    // legal ones ask for it — verifying is the point of them).
    let lean = emit.iter().any(|e| e.starts_with('E'));
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, dir).expect("ingestion never refuses a corpus");
    let ground = if lean {
        Some(qubitick_build::ground::for_check(dir).expect("the ground assembles"))
    } else {
        None
    };
    let results = qubitick_build::run_pipeline(&db, &qubitick_build::walk_qbit(dir), ground.as_deref())
        .expect("the pipeline runs");
    let mut errors = Vec::new();
    for r in &results {
        for d in r.diagnostics.iter().filter(|d| d.is_error()) {
            errors.push((d.code.clone(), d.message.clone()));
        }
    }
    let mut emitted = Vec::new();
    if !emit.is_empty() {
        let ctx = qubitick_ctx::build_ctx(&db, "Era0/Strata0").unwrap();
        for rel in emit {
            let path = dir.join(rel);
            let text = qubitick_build::pipeline::emit_file(&ctx, &db, &path)
                .unwrap_or_else(|e| format!("emit failed: {e:#}"));
            emitted.push(((*rel).to_string(), text));
        }
    }
    Outcome {
        all_verified: !results.is_empty() && results.iter().all(|r| r.verified),
        errors,
        emitted,
    }
}

fn says(o: &Outcome, needle: &str) -> bool {
    o.errors.iter().any(|(_, m)| m.contains(needle))
}

fn detail(o: &Outcome) -> String {
    o.errors
        .iter()
        .map(|(c, m)| format!("{c}: {m}"))
        .collect::<Vec<_>>()
        .join(" | ")
}

const FORMATION: &[(&str, &str)] = &[
    (
        "Era0/Strata0/Definitions/D_S_00_Or.qbit",
        "D.S.00. Or\na \\vee b\nRole: connective\n",
    ),
    (
        "Era0/Strata0/Definitions/D_S_01_Land.qbit",
        "D.S.01. Land\na \\land b\nRole: connective\n",
    ),
    (
        "Era0/Strata0/Definitions/D_S_02_Tick.qbit",
        "D.S.02. Tick\n\\tau\nSema:\n\\tau = \\tau_0 \\vee (^+ \\tau)\n",
    ),
    (
        "Era0/Strata0/Definitions/D_S_03_TickZero.qbit",
        "D.S.03. TickZero\n\\tau_0\nType: \\tau\n",
    ),
    (
        "Era0/Strata0/Definitions/D_S_04_Succ.qbit",
        "D.S.04. Succ\n^+ a\nType: \\tau \\to \\tau\n",
    ),
];

fn formation_with<'a>(extra: &'a [(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut v: Vec<(&str, &str)> = FORMATION.to_vec();
    v.extend_from_slice(extra);
    v
}

/* ────────────────────────── legal, and must still build ────────────────────────── */

#[test]
fn a_keyword_for_a_glyph_builds_under_an_escaped_name() {
    let files = formation_with(&[(
        "Era0/Strata0/Definitions/D_S_05_Elem.qbit",
        "D.S.05. Elem\na \\in b\nSema:\n(\\tau_0 \\in b = b) \\land ((^+ a) \\in b = ^+ (a \\in b))\n",
    )]);
    let dir = corpus("keyword", &files);
    let o = run(
        &dir,
        &["Era0/Strata0/Definitions/D_S_05_Elem.qbit"],
    );
    assert!(o.all_verified, "a keyword glyph is legal: {}", detail(&o));
    assert!(
        o.emitted[0].1.contains("def in_ "),
        "`\\in` is escaped as `in_`:\n{}",
        o.emitted[0].1
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_glyph_named_like_the_grounds_equality_builds_as_a_letter_code() {
    let files = formation_with(&[(
        "Era0/Strata0/Definitions/D_S_05_EqLike.qbit",
        "D.S.05. EqLike\na \\Eq b\nSema:\n(\\tau_0 \\Eq b = b) \\land ((^+ a) \\Eq b = ^+ (a \\Eq b))\n",
    )]);
    let dir = corpus("groundname", &files);
    let o = run(
        &dir,
        &["Era0/Strata0/Definitions/D_S_05_EqLike.qbit"],
    );
    assert!(o.all_verified, "a ground-owned name is letter-coded: {}", detail(&o));
    assert!(
        o.emitted[0].1.contains("def lx_Eq "),
        "`\\Eq` is emitted as `lx_Eq`, not as the kernel equality:\n{}",
        o.emitted[0].1
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn punctuation_the_name_map_does_not_know_still_gets_a_name() {
    let files = formation_with(&[(
        "Era0/Strata0/Definitions/D_S_05_Dot.qbit",
        "D.S.05. Dot\na \u{2299} b\nSema:\n(\\tau_0 \u{2299} b = b) \\land ((^+ a) \u{2299} b = ^+ (a \u{2299} b))\n",
    )]);
    let dir = corpus("punct", &files);
    let o = run(&dir, &["Era0/Strata0/Definitions/D_S_05_Dot.qbit"]);
    assert!(o.all_verified, "an unknown punctuation glyph is legal: {}", detail(&o));
    assert!(
        o.emitted[0].1.lines().any(|l| l.contains("def sym_")),
        "the name is derived from the codepoints:\n{}",
        o.emitted[0].1
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn two_glyphs_that_read_as_one_lean_name_get_two_names() {
    // `\times` and `×` both derive `times`, and Lean has one namespace: without a rule the
    // corpus has one operator where it declared two, and the modules that each declare
    // `def times` collide the moment anything imports both. The earlier claim keeps the name;
    // the other takes the engine's letter code (which no LaTeX glyph can derive).
    let files = formation_with(&[
        (
            "Era0/Strata0/Definitions/D_S_05_TimesWord.qbit",
            "D.S.05. TimesWord\na \\times b\nSema:\n(\\tau_0 \\times b = b) \\land ((^+ a) \\times b = ^+ (a \\times b))\n",
        ),
        (
            "Era0/Strata0/Definitions/D_S_06_TimesGlyph.qbit",
            "D.S.06. TimesGlyph\na × b\nSema:\n(\\tau_0 × b = b) \\land ((^+ a) × b = ^+ (a × b))\n",
        ),
    ]);
    let dir = corpus("onename", &files);
    let o = run(
        &dir,
        &[
            "Era0/Strata0/Definitions/D_S_05_TimesWord.qbit",
            "Era0/Strata0/Definitions/D_S_06_TimesGlyph.qbit",
        ],
    );
    assert!(o.all_verified, "two glyphs, two names, one build: {}", detail(&o));
    let named = |needle: &str| o.emitted.iter().any(|(_, t)| t.contains(needle));
    assert!(
        named("def times ") && named("def lx_times "),
        "one glyph keeps the name and the other is letter-coded:\n{}\n{}",
        o.emitted[0].1,
        o.emitted[1].1
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/* ────────────────────────── refused, by name ────────────────────────── */

#[test]
fn a_definitions_second_bare_line_is_named_not_dropped() {
    let files = formation_with(&[(
        "Era0/Strata0/Definitions/D_S_05_Two.qbit",
        "D.S.05. Two\na \\in b\nSema:\n\\tau_0 \\in b = b\n^+ a \\in b = b\n",
    )]);
    let dir = corpus("twoline", &files);
    let o = run(&dir, &[]);
    assert!(!o.all_verified, "two bare lines cannot both be the body");
    assert!(
        says(&o, "carries one glyph line") && says(&o, "^+ a \\in b = b"),
        "the refusal quotes the line it will not read: {}",
        detail(&o)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_case_chain_without_parentheses_is_reported_not_panicked() {
    // `A = B \land C = D` binds as `((A = B) = C) ∧ D`: not two sections. The requirement is
    // only that the run finishes and the file is refused — the panic this shape used to cause
    // (an arithmetic overflow deriving a Lean name for `=`) cannot be caught by a test.
    let files = formation_with(&[(
        "Era0/Strata0/Definitions/D_S_05_Paren.qbit",
        "D.S.05. Paren\na \\in b\nSema:\n\\tau_0 \\in b = b \\land ^+ a \\in b = ^+ (a \\in b)\n",
    )]);
    let dir = corpus("parenless", &files);
    let o = run(&dir, &[]);
    assert!(!o.all_verified, "an unreadable body is not a definition");
    assert!(!o.errors.is_empty(), "and the build says so");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_sema_naming_an_undeclared_glyph_is_refused_by_name() {
    let files = formation_with(&[(
        "Era0/Strata0/Definitions/D_S_05_Odd.qbit",
        "D.S.05. Odd\na \\in b\nSema:\n(\\tau_0 \\in b = b) \\land ((^+ a) \\in b = \\frobnicate a b)\n",
    )]);
    let dir = corpus("undeclared", &files);
    let o = run(&dir, &[]);
    assert!(!o.all_verified);
    assert!(
        says(&o, "\\frobnicate"),
        "the sentence names the glyph: {}",
        detail(&o)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_type_with_no_value_is_refused() {
    let files = formation_with(&[(
        "Era0/Strata0/Definitions/D_S_05_Empty.qbit",
        "D.S.05. Empty\n\\varnothing_5\nType:\n",
    )]);
    let dir = corpus("emptytype", &files);
    let o = run(&dir, &[]);
    assert!(!o.all_verified, "a `Type:` with no argument declares nothing");
    assert!(!o.errors.is_empty(), "and the build says so: {}", detail(&o));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unknown_role_is_refused_by_name() {
    let files = formation_with(&[(
        "Era0/Strata0/Definitions/D_S_05_Odd.qbit",
        "D.S.05. Odd\na \\odot b\nRole: sideways\n",
    )]);
    let dir = corpus("role", &files);
    let o = run(&dir, &[]);
    assert!(!o.all_verified);
    assert!(
        says(&o, "sideways"),
        "the sentence names the word it could not read: {}",
        detail(&o)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_citation_of_a_node_that_does_not_exist_is_reported() {
    let files = formation_with(&[
        (
            "Era0/Strata0/Definitions/D_S_05_Plus.qbit",
            "D.S.05. Plus\na \\oplus b\nSema:\n(\\tau_0 \\oplus b = b) \\land ((^+ a) \\oplus b = ^+ (a \\oplus b))\n",
        ),
        (
            "Era0/Strata0/Theorems/T_S_01_Bad.qbit",
            "T.S.01. Bad\n\\forall a : \\tau, \\tau_0 \\oplus a = a\nProof:\n\\tau_0 \\oplus a\n  = a  [D.S.99]\n",
        ),
    ]);
    let dir = corpus("danglingcite", &files);
    let o = run(&dir, &[]);
    assert!(!o.all_verified, "a citation of nothing proves nothing");
    assert!(
        says(&o, "D.S.99") || says(&o, "99"),
        "the sentence names the citation: {}",
        detail(&o)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_sema_body_that_is_not_an_equation_is_refused_by_name() {
    // `\tau_0 \oplus b` is an equation the author did not finish. Read as a term it "cannot be
    // rendered", which names neither the field nor the missing `=`.
    let files = formation_with(&[(
        "Era0/Strata0/Definitions/D_S_05_Half.qbit",
        "D.S.05. Half\na \\oplus b\nSema:\n\\tau_0 \\oplus b\n",
    )]);
    let dir = corpus("notanequation", &files);
    let o = run(&dir, &[]);
    assert!(!o.all_verified, "half an equation defines nothing");
    assert!(
        says(&o, "is not an equation"),
        "the sentence names the field and the shape it wants: {}",
        detail(&o)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_sema_with_no_body_at_all_is_refused_by_name() {
    let files = formation_with(&[(
        "Era0/Strata0/Definitions/D_S_05_Empty.qbit",
        "D.S.05. Empty\na \\oplus b\nSema:\n",
    )]);
    let dir = corpus("emptybody", &files);
    let o = run(&dir, &[]);
    assert!(!o.all_verified, "a `Sema:` that states nothing declares nothing");
    assert!(
        says(&o, "states no body"),
        "the refusal names the field: {}",
        detail(&o)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_sema_section_that_misses_a_constructor_is_a_hole_and_the_build_says_where() {
    let files = formation_with(&[(
        "Era0/Strata0/Definitions/D_S_05_Part.qbit",
        "D.S.05. Part\na \\oplus b\nSema:\n\\tau_0 \\oplus b = b\n",
    )]);
    let dir = corpus("partial", &files);
    let o = run(&dir, &[]);
    assert!(!o.all_verified, "one case does not define a two-constructor operator");
    assert!(!o.errors.is_empty(), "and the build says so: {}", detail(&o));
    let _ = std::fs::remove_dir_all(&dir);
}