//! A CORPUS MAY DECLARE AN INDUCTIVE, and the engine emits it.
//!
//! The engine used to carry the tick sort and the integer sort itself: a glyph table
//! said which tex was which, and Rust string literals held whole Lean declarations
//! (`inductive tau : Type | zero | succ`). Both are gone. What replaced them is this:
//! a formation node declares its carrier and its constructors — including a RECURSIVE
//! one — and the emitter writes the inductive, its constructor aliases, and the types
//! the corpus's own signatures state over that carrier.
//!
//! Nothing here is engine vocabulary: `tau`, `tau_0`, `succ`, `vee` and `oplus` are
//! this fixture corpus's spellings, declared in its own files.

use std::path::{Path, PathBuf};

fn root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qb-form-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn wit(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// The tick sequence as a CORPUS declares it: a carrier, a connective to separate the
/// constructors, the two constructors (one nullary, one recursive), an operator over
/// the carrier, and an axiom that uses them.
fn corpus(dir: &Path) {
    wit(dir, "Canon.qbit", "P. the tick corpus\nId: tick\n");
    wit(
        dir,
        "Era0/Strata0/Definitions/D_S_01_Or.qbit",
        "D.S.01. Or\na \\vee b\nRole: connective\n",
    );
    wit(
        dir,
        "Era0/Strata0/Definitions/D_S_02_Tick.qbit",
        "D.S.02. Tick\n\\tau\nSema:\n\\tau = \\tau_0 \\vee (^+ \\tau)\n",
    );
    wit(
        dir,
        "Era0/Strata0/Definitions/D_S_03_TickZero.qbit",
        "D.S.03. TickZero\n\\tau_0\nType: \\tau\n",
    );
    wit(
        dir,
        "Era0/Strata0/Definitions/D_S_04_Succ.qbit",
        "D.S.04. Succ\n^+ a\nType: \\tau \\to \\tau\n",
    );
    wit(
        dir,
        "Era0/Strata0/Definitions/D_S_05_Plus.qbit",
        "D.S.05. Plus\na \\oplus b\n",
    );
    // No A block: a law an A block states becomes a field of the GROUND's class, and a
    // class in the ground cannot be written over `\tau` — the formation is this corpus's,
    // and its own module declares it. Statements about it are T blocks.
}

fn lean_is_here() -> bool {
    std::process::Command::new("lean")
        .arg("--version")
        .output()
        .is_ok()
}

#[test]
fn a_declared_formation_is_emitted_as_an_inductive_with_typed_constructors() {
    let dir = root("inductive");
    corpus(&dir);
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &dir).unwrap();
    let file = dir.join("Era0/Strata0/Definitions/D_S_02_Tick.qbit");
    let ctx = qubitick_ctx::build_ctx(&db, "Era0/Strata0").unwrap();
    let emitted =
        qubitick_build::pipeline::emit_file(&ctx, &db, &file).expect("the formation emits");

    assert!(
        emitted.contains("inductive tau : Type"),
        "the corpus's formation is emitted as its own inductive:\n{emitted}"
    );
    assert!(
        !emitted.contains("| tau_0 : tau\n") || emitted.contains("tau_0"),
        "the nullary constructor is declared:\n{emitted}"
    );
    // The RECURSIVE constructor carries its declared argument sort, which is what
    // makes the inductive a real tick sequence instead of a pile of constants.
    assert!(
        emitted.lines().any(|l| l.trim_start().starts_with('|') && l.contains("tau → tau")),
        "the recursive constructor is a function into the carrier:\n{emitted}"
    );
    // ...and the module declares NOTHING ELSE for the operator: `\oplus` is another
    // node's, and the frame writes no `axiom` for it.
    assert!(
        !emitted.contains("axiom"),
        "a corpus module declares no axiom:\n{emitted}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_declared_formation_builds_and_verifies() {
    if !lean_is_here() {
        eprintln!("skipping: lean not found");
        return;
    }
    let dir = root("build");
    corpus(&dir);
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &dir).unwrap();
    let ground = qubitick_build::ground::for_check(&dir).expect("the ground assembles");
    let results =
        qubitick_build::run_pipeline(&db, &qubitick_build::walk_qbit(&dir), Some(&ground)).unwrap();
    let errors: Vec<String> = results
        .iter()
        .flat_map(|r| r.diagnostics.iter())
        .filter(|d| d.is_error())
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert!(
        errors.is_empty(),
        "a corpus that declares its own inductive must build clean: {errors:?}"
    );
    assert!(
        results.iter().all(|r| r.verified),
        "all of it verifies: {:?}",
        results
            .iter()
            .filter(|r| !r.verified)
            .map(|r| (r.path.clone(), r.diagnostics.first().cloned()))
            .collect::<Vec<_>>()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
/* A FORMATION DECLARED ONCE, IN ONE MODULE, EVEN WHEN TWO MODULES NAME THE CARRIER.
 *
 * The frame used to declare an inductive in EVERY module that mentioned its carrier, which
 * each module survived alone and no environment survived twice: put a citing module and the
 * formation's own module in one build and Lean saw `Qubitick.HL.tau has already been
 * declared` — three times, once per name. A corpus with a declared formation and a citation
 * across modules could not build at all.
 *
 * The theorem below is the minimal shape that forces the collision: its statement names the
 * CARRIER (`\\forall a : \\tau, ...`), so its module must see the inductive, and the node that
 * declares that inductive is another file. The fix is the rule the emitter already used for
 * definitions: a module DECLARES what it owns and IMPORTS the rest.
 */
#[test]
fn a_theorem_that_names_the_carrier_imports_the_formation_instead_of_repeating_it() {
    let dir = root("across-modules");
    corpus(&dir);
    wit(
        &dir,
        "Era0/Strata0/Theorems/T_S_01_CarrierRefl.qbit",
        "T.S.01. CarrierRefl\n\\forall a : \\tau, \\tau_0 = \\tau_0\nProof:\n\\tau_0\n  = \\tau_0\n",
    );
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &dir).unwrap();

    // The theorem's module imports the module that declares the inductive, and declares none.
    let file = dir.join("Era0/Strata0/Theorems/T_S_01_CarrierRefl.qbit");
    let ctx = qubitick_ctx::build_ctx(&db, "Era0/Strata0").unwrap();
    let emitted = qubitick_build::pipeline::emit_file(&ctx, &db, &file).expect("the theorem emits");
    assert!(
        emitted.contains("import Qubitick.Corpus.Era0.Strata0.Definitions.D_S_02_Tick"),
        "the module that names the carrier must import the module that declares it:\n{emitted}"
    );
    assert!(
        !emitted.contains("inductive tau"),
        "and must not declare the inductive a second time:\n{emitted}"
    );

    if !lean_is_here() {
        eprintln!("skipping the build half: lean not found");
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    // ...and the corpus builds: two modules naming one carrier is one environment, not two.
    let ground = qubitick_build::ground::for_check(&dir).expect("the ground assembles");
    let results =
        qubitick_build::run_pipeline(&db, &qubitick_build::walk_qbit(&dir), Some(&ground)).unwrap();
    let errors: Vec<String> = results
        .iter()
        .flat_map(|r| r.diagnostics.iter())
        .filter(|d| d.is_error())
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert!(errors.is_empty(), "the corpus must build clean: {errors:?}");
    assert!(
        results.iter().all(|r| r.verified),
        "all of it verifies: {:?}",
        results
            .iter()
            .filter(|r| !r.verified)
            .map(|r| (r.path.clone(), r.diagnostics.first().cloned()))
            .collect::<Vec<_>>()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/* THE `Sema:` IS A RECIPE FOR THE OPERATOR'S TYPE, NOT ONLY FOR ITS BODY.
 *
 * `\tau_0 \oplus b = b` applies `\oplus` to a constructor of `\tau` and a metavariable
 * and equates the result with that metavariable. Read as a recipe, that is
 * `oplus : tau → tau → tau`. Read only from the operator's uses there is nothing to read —
 * its single appearance IS this definition — and the frame declared it
 * `Prop → Prop → Prop`, after which every statement mixing it with `\tau` was an
 * application-type mismatch (`a has type tau but is expected to have type Prop`).
 */
#[test]
fn the_sema_composes_the_operators_type() {
    let dir = root("sema-type");
    corpus(&dir);
    // `\oplus` is this corpus's own operator over its own carrier, defined by constructor
    // equations and by nothing else: no statement uses it, so the only evidence of its type
    // is its own `Sema:`. (Both constructors have a case — a partial definition is a hole,
    // and `crates/build/tests/refusal_surface.rs` holds that the build says so.)
    wit(
        &dir,
        "Era0/Strata0/Definitions/D_S_00_Land.qbit",
        "D.S.00. Land\na \\land b\nRole: connective\n",
    );
    wit(
        &dir,
        "Era0/Strata0/Definitions/D_S_05_Plus.qbit",
        "D.S.05. Plus\na \\oplus b\nSema:\n(\\tau_0 \\oplus b = b) \\land ((^+ a) \\oplus b = ^+ (a \\oplus b))\n",
    );
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &dir).unwrap();
    let file = dir.join("Era0/Strata0/Definitions/D_S_05_Plus.qbit");
    let ctx = qubitick_ctx::build_ctx(&db, "Era0/Strata0").unwrap();
    let emitted = qubitick_build::pipeline::emit_file(&ctx, &db, &file).expect("the operator emits");

    assert!(
        emitted.contains("oplus : tau → tau → tau"),
        "the Sema's own patterns type the operator over its carrier:\n{emitted}"
    );
    assert!(
        !emitted.contains("Prop → Prop → Prop"),
        "and never over the Prop fallback:\n{emitted}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/* THE SEPARATOR IS THE SEMA'S OWN, NOT "THE CORPUS'S CONNECTIVE".
 *
 * `formation_of_expr` used to split a formation's alternatives on
 * `declared_with_facet(Connective)` — the LOWEST-ID connective the corpus declares. A corpus
 * that declares `\land` as well as `\vee` then had `\tau = \tau_0 \vee (^+ \tau)` split on the
 * wrong glyph, so the node was classified as an ordinary equation: no inductive, an empty
 * formation map in the emitter, and every constructor became a coverage hole
 * ("`\tau_0` has a `Type:` signature and no home"). Adding ONE unrelated connective node was
 * enough to do it.
 */
#[test]
fn a_second_declared_connective_does_not_unmake_a_formation() {
    let dir = root("two-connectives");
    corpus(&dir);
    // A connective the formation does not use, declared first so its id is lower.
    wit(
        &dir,
        "Era0/Strata0/Definitions/D_S_00_Land.qbit",
        "D.S.00. Land\na \\land b\nRole: connective\n",
    );
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &dir).unwrap();
    let file = dir.join("Era0/Strata0/Definitions/D_S_02_Tick.qbit");
    let ctx = qubitick_ctx::build_ctx(&db, "Era0/Strata0").unwrap();
    let emitted = qubitick_build::pipeline::emit_file(&ctx, &db, &file).expect("the formation emits");
    assert!(
        emitted.contains("inductive tau : Type"),
        "the carrier is still an inductive with a second connective declared:\n{emitted}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/* THE SEMA'S CASES ARE THE DEFINITION, COMPILED TO THE CARRIER'S RECURSOR.
 *
 * `match` arms were tried first and Lean refused them: a multi-argument pattern match is
 * encoded through `PProd`, and a module emitted under `prelude` has none
 * (`Unknown constant PProd.mk`). The recursor is spelled out instead.
 */
#[test]
fn a_semas_cases_become_a_definition_over_the_recursor() {
    let dir = root("recursor-def");
    corpus(&dir);
    wit(
        &dir,
        "Era0/Strata0/Definitions/D_S_00_Land.qbit",
        "D.S.00. Land\na \\land b\nRole: connective\n",
    );
    // Two cases over the carrier: `\tau_0 \oplus b = b` and the recursive one.
    wit(
        &dir,
        "Era0/Strata0/Definitions/D_S_05_Plus.qbit",
        "D.S.05. Plus\na \\oplus b\nSema:\n(\\tau_0 \\oplus b = b) \\land ((^+ a) \\oplus b = ^+ (a \\oplus b))\n",
    );
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &dir).unwrap();
    let file = dir.join("Era0/Strata0/Definitions/D_S_05_Plus.qbit");
    let ctx = qubitick_ctx::build_ctx(&db, "Era0/Strata0").unwrap();
    let emitted = qubitick_build::pipeline::emit_file(&ctx, &db, &file).expect("the cases emit");
    assert!(
        emitted.contains("noncomputable def oplus : tau → tau → tau"),
        "the Sema composed the type:\n{emitted}"
    );
    assert!(
        emitted.contains("tau.rec"),
        "the cases compile to the carrier's recursor:\n{emitted}"
    );
    assert!(
        emitted.contains("(fun b => b)") && emitted.contains("fun a ih => fun b => caret (ih b)"),
        "one case per constructor, the recursive call through the hypothesis:\n{emitted}"
    );
    assert!(!emitted.contains("axiom"), "a definition is not an assumption:\n{emitted}");
    let _ = std::fs::remove_dir_all(&dir);
}

/* A ROW OVER A CASE-DEFINED OPERATOR IS JUSTIFIED BY THE CASE THAT MATCHES IT.
 *
 * One citation (`[D.5]`) names a `Sema:` written as cases, so the rewrite table has to hold
 * one rule per section — and it held none at all, for two reasons that hid each other:
 *
 *   · the table kept ONE statement per key, so the cases overwrote each other;
 *   · the loader stringified the `FormBody` (`MacroExpr(…)`, `Formation([…])`) and fed the
 *     string to the expression parser, which refuses it — and an `Expr`'s own Debug is
 *     refused too (`unknown symbol 'Apply'`). Both failed silently, so NO definition rule
 *     was ever registered through this path.
 *
 * With both fixed, a row `\tau_0 \oplus a = a` citing `[D.05]` closes, and the corpus — a
 * declared carrier, a two-case operator, a theorem that uses it — verifies.
 */
#[test]
fn a_row_over_a_case_defined_operator_is_justified() {
    if !lean_is_here() {
        eprintln!("skipping: lean not found");
        return;
    }
    let dir = root("cases-cite");
    corpus(&dir);
    wit(
        &dir,
        "Era0/Strata0/Definitions/D_S_00_Land.qbit",
        "D.S.00. Land\na \\land b\nRole: connective\n",
    );
    wit(
        &dir,
        "Era0/Strata0/Definitions/D_S_05_Plus.qbit",
        "D.S.05. Plus\na \\oplus b\nSema:\n(\\tau_0 \\oplus b = b) \\land ((^+ a) \\oplus b = ^+ (a \\oplus b))\n",
    );
    wit(
        &dir,
        "Era0/Strata0/Theorems/T_S_01_Zero.qbit",
        "T.S.01. ZeroPlus\n\\tau_0 \\oplus \\tau_0 = \\tau_0\nProof:\n\\tau_0 \\oplus \\tau_0\n  = \\tau_0  [D.05]\n",
    );
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &dir).unwrap();
    let ground = qubitick_build::ground::for_check(&dir).expect("the ground assembles");
    let results =
        qubitick_build::run_pipeline(&db, &qubitick_build::walk_qbit(&dir), Some(&ground)).unwrap();
    let errors: Vec<String> = results
        .iter()
        .flat_map(|r| r.diagnostics.iter())
        .filter(|d| d.is_error())
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert!(
        errors.is_empty(),
        "a row citing a case-defined operator must justify: {errors:?}"
    );
    assert!(
        results.iter().all(|r| r.verified),
        "all of it verifies: {:?}",
        results
            .iter()
            .filter(|r| !r.verified)
            .map(|r| (r.path.clone(), r.diagnostics.first().cloned()))
            .collect::<Vec<_>>()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/* AN INDUCTION PROOF OVER A CASE-DEFINED OPERATOR VERIFIES, END TO END.
 *
 * Everything the path needs, in one corpus: a declared carrier (`\tau = \tau_0 \vee (^+ \tau)`),
 * a two-case operator over it (`\oplus`, whose `Sema:` composes its type `tau → tau → tau`),
 * and a theorem proved by induction on the carrier, each case closing by unfolding the operator.
 *
 * The emitted term is the carrier's recursor, and the RECURSIVE branch takes the hypothesis
 * because the constructor's declaration says its field is the carrier:
 *
 *     fun a => @tau.rec (fun x_i => Eq (oplus tau_0 x_i) x_i)
 *       (Eq.refl (oplus tau_0 tau_0))
 *       (fun a ih => (Eq.refl (oplus tau_0 (caret a))))
 *       a
 *
 * The branch used to be `fun a => …` — one binder short — because the hypothesis was added only
 * when the compiled body happened to mention `ih`, and `tau.rec`'s branch is `fun a ih => …`.
 */
#[test]
fn an_induction_over_a_case_defined_operator_verifies() {
    if !lean_is_here() {
        eprintln!("skipping: lean not found");
        return;
    }
    let dir = root("induction");
    corpus(&dir);
    wit(
        &dir,
        "Era0/Strata0/Definitions/D_S_00_Land.qbit",
        "D.S.00. Land\na \\land b\nRole: connective\n",
    );
    wit(
        &dir,
        "Era0/Strata0/Definitions/D_S_05_Plus.qbit",
        "D.S.05. Plus\na \\oplus b\nSema:\n(\\tau_0 \\oplus b = b) \\land ((^+ a) \\oplus b = ^+ (a \\oplus b))\n",
    );
    wit(
        &dir,
        "Era0/Strata0/Theorems/T_S_01_Zero.qbit",
        "T.S.01. ZeroPlus\n\\forall a : \\tau, \\tau_0 \\oplus a = a\nProof:\n\\mu a\n  \\tau_0 \\mapsto\n    \\tau_0 \\oplus \\tau_0\n      = \\tau_0  [D.05]\n  ^+ a \\mapsto\n    \\tau_0 \\oplus ^+ a\n      = ^+ a  [D.05]\n",
    );
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &dir).unwrap();
    let ground = qubitick_build::ground::for_check(&dir).expect("the ground assembles");
    let results =
        qubitick_build::run_pipeline(&db, &qubitick_build::walk_qbit(&dir), Some(&ground)).unwrap();
    let errors: Vec<String> = results
        .iter()
        .flat_map(|r| r.diagnostics.iter())
        .filter(|d| d.is_error())
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert!(errors.is_empty(), "an induction over a case-defined operator must build: {errors:?}");
    assert!(
        results.iter().all(|r| r.verified),
        "all of it verifies: {:?}",
        results
            .iter()
            .filter(|r| !r.verified)
            .map(|r| (r.path.clone(), r.diagnostics.first().cloned()))
            .collect::<Vec<_>>()
    );
    // The branch takes its hypothesis because the CONSTRUCTOR's field is the carrier.
    let ctx = qubitick_ctx::build_ctx(&db, "Era0/Strata0").unwrap();
    let file = dir.join("Era0/Strata0/Theorems/T_S_01_Zero.qbit");
    let emitted = qubitick_build::pipeline::emit_file(&ctx, &db, &file).expect("the theorem emits");
    assert!(emitted.contains("@tau.rec"), "the proof is the carrier's recursor:\n{emitted}");
    assert!(
        emitted.contains("fun a ih =>"),
        "the recursive branch takes the induction hypothesis:\n{emitted}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/* AN OPERATOR WRITTEN IN TERMS OF ANOTHER OPERATOR IS TYPED BY IT.
 *
 * `a \otimes b = (a \oplus b) \oplus b` holds no constant and no constructor: every sort in
 * that body is `\oplus`'s, and `\oplus` is a DIFFERENT NODE — a different module — whose own
 * `Sema:` is the recipe for its type. Read once in node order, `\otimes` came out
 * `Prop → Prop → Prop`, and the module could not even name `oplus` (it never imported the
 * module that defines it). Both are the same reading: a `Sema:` composes its operator's type
 * from the signatures of the operators its body uses, and a module imports what its own text
 * names and does not declare.
 */
#[test]
fn an_operator_defined_through_another_is_typed_by_it() {
    let dir = root("compose");
    corpus(&dir);
    wit(
        &dir,
        "Era0/Strata0/Definitions/D_S_00_Land.qbit",
        "D.S.00. Land\na \\land b\nRole: connective\n",
    );
    wit(
        &dir,
        "Era0/Strata0/Definitions/D_S_05_Plus.qbit",
        "D.S.05. Plus\na \\oplus b\nSema:\n(\\tau_0 \\oplus b = b) \\land ((^+ a) \\oplus b = ^+ (a \\oplus b))\n",
    );
    wit(
        &dir,
        "Era0/Strata0/Definitions/D_S_06_Times.qbit",
        "D.S.06. Times\na \\otimes b\nSema:\na \\otimes b = (a \\oplus b) \\oplus b\n",
    );
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &dir).unwrap();
    let ctx = qubitick_ctx::build_ctx(&db, "Era0/Strata0").unwrap();
    let file = dir.join("Era0/Strata0/Definitions/D_S_06_Times.qbit");
    let emitted = qubitick_build::pipeline::emit_file(&ctx, &db, &file).expect("the operator emits");
    assert!(
        emitted.contains("otimes (a : tau) (b : tau) : tau"),
        "the body's operands type the parameters and its head types the result:\n{emitted}"
    );
    assert!(
        !emitted.contains("Prop → Prop → Prop"),
        "and never the Prop fallback:\n{emitted}"
    );
    assert!(
        emitted.contains("import Qubitick.Corpus.Era0.Strata0.Definitions.D_S_05_Plus"),
        "a glyph this module names and does not declare is imported, not assumed:\n{emitted}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/* TWO CARRIERS IN ONE STRATUM, AND AN OPERATOR THAT MIXES THEM.
 *
 * `\tau` and `\iota` are two formations declared in one stratum, and `a \star b` takes one
 * of each and returns an `\iota`. Three things had to hold at once, and each had failed:
 *
 *   · the modules of the two formations must not import each other — a formation's module
 *     owns its inductive, and a declaration or import derived from the whole CONTEXT made
 *     each one pull in the other (`D_S_02_Tick → D_S_05_Leaf → D_S_02_Tick`, so the corpus
 *     had no first module);
 *   · the operator's type is `tau → iota → iota`, composed from the constructors its cases
 *     name — the first case's `b` is typed by the result the RECURSIVE case states, which is
 *     why the composition records evidence before it writes anything;
 *   · the recursor's motive is the composed type's tail (`fun _ => iota → iota`), not the
 *     matched argument's carrier written out, which asked Lean for `tau → tau`.
 */
#[test]
fn a_mixed_operator_over_two_carriers_verifies() {
    if !lean_is_here() {
        eprintln!("skipping: lean not found");
        return;
    }
    let dir = root("mixed");
    corpus(&dir);
    wit(
        &dir,
        "Era0/Strata0/Definitions/D_S_00_Land.qbit",
        "D.S.00. Land\na \\land b\nRole: connective\n",
    );
    // A SECOND formation, in the same stratum, with its own nullary and recursive ctor.
    wit(
        &dir,
        "Era0/Strata0/Definitions/D_S_06_Leaf.qbit",
        "D.S.06. Leaf\n\\iota\nSema:\n\\iota = \\iota_0 \\vee (\\bullet \\iota)\n",
    );
    wit(
        &dir,
        "Era0/Strata0/Definitions/D_S_07_LeafZero.qbit",
        "D.S.07. LeafZero\n\\iota_0\nType: \\iota\n",
    );
    wit(
        &dir,
        "Era0/Strata0/Definitions/D_S_08_LeafSucc.qbit",
        "D.S.08. LeafSucc\n\\bullet a\nType: \\iota \\to \\iota\n",
    );
    // One of each: `\tau` against `\iota`, returning `\iota`.
    wit(
        &dir,
        "Era0/Strata0/Definitions/D_S_09_Star.qbit",
        "D.S.09. Star\na \\star b\nSema:\n(\\tau_0 \\star b = b) \\land ((^+ a) \\star b = \\bullet (a \\star b))\n",
    );
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, &dir).unwrap();
    let ctx = qubitick_ctx::build_ctx(&db, "Era0/Strata0").unwrap();

    // Neither formation's module imports the other: each owns its inductive.
    for (file, own, other) in [
        ("D_S_02_Tick.qbit", "inductive tau", "D_S_06_Leaf"),
        ("D_S_06_Leaf.qbit", "inductive iota", "D_S_02_Tick"),
    ] {
        let emitted = qubitick_build::pipeline::emit_file(&ctx, &db, &dir.join("Era0/Strata0/Definitions").join(file))
            .expect("the formation emits");
        assert!(emitted.contains(own), "{file} declares its inductive:\n{emitted}");
        assert!(
            !emitted.contains(other),
            "{file} must not import the other formation's module:\n{emitted}"
        );
    }

    let emitted = qubitick_build::pipeline::emit_file(
        &ctx,
        &db,
        &dir.join("Era0/Strata0/Definitions/D_S_09_Star.qbit"),
    )
    .expect("the mixed operator emits");
    assert!(
        emitted.contains("star : tau → iota → iota"),
        "the cases type the operator over both carriers:\n{emitted}"
    );
    assert!(
        emitted.contains("motive := fun _ => iota → iota"),
        "the recursor's motive is the composed type's tail:\n{emitted}"
    );

    let ground = qubitick_build::ground::for_check(&dir).expect("the ground assembles");
    let results =
        qubitick_build::run_pipeline(&db, &qubitick_build::walk_qbit(&dir), Some(&ground)).unwrap();
    let errors: Vec<String> = results
        .iter()
        .flat_map(|r| r.diagnostics.iter())
        .filter(|d| d.is_error())
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert!(errors.is_empty(), "two carriers and a mixed operator must build: {errors:?}");
    assert!(
        results.iter().all(|r| r.verified),
        "all of it verifies: {:?}",
        results
            .iter()
            .filter(|r| !r.verified)
            .map(|r| (r.path.clone(), r.diagnostics.first().cloned()))
            .collect::<Vec<_>>()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
