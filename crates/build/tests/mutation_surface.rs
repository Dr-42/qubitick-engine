//! MUTATED CORPORA: NO PANIC, AND NO INVENTED AXIOM.
//!
//! A corpus is edited, half-written, and mangled by accident. The engine must answer every one
//! of those with a result — a build, a diagnostic, or a refusal — and it must never answer with
//! a PANIC (which takes the request down, leaving the pane blank) or with an `axiom` (an
//! assumption nobody wrote, in place of what the corpus said).
//!
//! The mutations are deterministic: one seeded generator, no `rand` dependency, so a failure
//! here reproduces exactly. Each mutant is a small, plausible edit of a small valid corpus —
//! a dropped line, a duplicated line, a line cut in half, a glyph character replaced, a field
//! keyword broken, a stray bracket — and the checks are:
//!
//!   * ingestion returns (an `Err` is an answer; a panic is not),
//!   * every file the ledger knows can be emitted (again: `Err` is an answer),
//!   * and no emitted module declares an `axiom` — this corpus has no A block, so an axiom in
//!     its output can only be the engine inventing one.

use std::path::{Path, PathBuf};

/// A tiny deterministic generator: xorshift64*, so a failing case is reproducible from its seed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
}

fn base() -> Vec<(String, String)> {
    vec![
        (
            "Canon.qbit".into(),
            "P. mutated corpus\nId: mutated\n".into(),
        ),
        (
            "Era0/Strata0/Definitions/D_S_00_Or.qbit".into(),
            "D.S.00. Or\na \\vee b\nRole: connective\n".into(),
        ),
        (
            "Era0/Strata0/Definitions/D_S_01_Land.qbit".into(),
            "D.S.01. Land\na \\land b\nRole: connective\n".into(),
        ),
        (
            "Era0/Strata0/Definitions/D_S_02_Tick.qbit".into(),
            "D.S.02. Tick\n\\tau\nSema:\n\\tau = \\tau_0 \\vee (^+ \\tau)\n".into(),
        ),
        (
            "Era0/Strata0/Definitions/D_S_03_TickZero.qbit".into(),
            "D.S.03. TickZero\n\\tau_0\nType: \\tau\n".into(),
        ),
        (
            "Era0/Strata0/Definitions/D_S_04_Succ.qbit".into(),
            "D.S.04. Succ\n^+ a\nType: \\tau \\to \\tau\n".into(),
        ),
        (
            "Era0/Strata0/Definitions/D_S_05_Plus.qbit".into(),
            "D.S.05. Plus\na \\oplus b\nSema:\n(\\tau_0 \\oplus b = b) \\land ((^+ a) \\oplus b = ^+ (a \\oplus b))\n".into(),
        ),
        (
            "Era0/Strata0/Theorems/T_S_01_Zero.qbit".into(),
            "T.S.01. Zero\n\\forall a : \\tau, \\tau_0 \\oplus a = a\nProof:\n\\tau_0 \\oplus a\n  = a  [D.S.05]\n".into(),
        ),
    ]
}

/// One plausible edit, applied to one file.
fn mutate(files: &mut [(String, String)], rng: &mut Rng) -> String {
    let which = rng.below(files.len());
    let (name, text) = &mut files[which];
    let lines: Vec<&str> = text.split('\n').collect();
    let what = rng.below(8);
    let edited = match what {
        // drop a line
        0 => {
            let i = rng.below(lines.len());
            lines
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, l)| *l)
                .collect::<Vec<_>>()
                .join("\n")
        }
        // duplicate a line
        1 => {
            let i = rng.below(lines.len());
            let mut out = lines.clone();
            out.insert(i, lines[i]);
            out.join("\n")
        }
        // cut a line in half
        2 => {
            let i = rng.below(lines.len());
            let cut = rng.below(lines[i].len() + 1);
            let mut out = lines.clone();
            out[i] = &lines[i][..cut];
            out.join("\n")
        }
        // replace one character with a glyph-ish one
        3 => {
            let i = rng.below(lines.len());
            if lines[i].is_empty() {
                text.clone()
            } else {
                let at = rng.below(lines[i].len());
                let mut l = lines[i].to_string();
                let ch = ['(', ')', '=', '\\', ' ', '×'][rng.below(6)];
                l.replace_range(at..at + 1, &ch.to_string());
                let mut out = lines.clone();
                out[i] = &l;
                out.join("\n")
            }
        }
        // break a field keyword
        4 => text.replace("Sema:", "Sema").replace("Type:", "Typ:"),
        // swap two lines
        5 => {
            if lines.len() < 2 {
                text.clone()
            } else {
                let i = rng.below(lines.len());
                let j = rng.below(lines.len());
                let mut out = lines.clone();
                out.swap(i, j);
                out.join("\n")
            }
        }
        // append a stray row
        6 => format!("{text}\na \\oplus b = c\n"),
        // empty the file
        _ => String::new(),
    };
    let said = format!("{}: {what}", name);
    *text = edited;
    said
}

fn write(dir: &Path, files: &[(String, String)]) {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    for (rel, text) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

#[test]
fn every_mutant_of_a_small_corpus_answers_and_invents_no_axiom() {
    let dir: PathBuf = std::env::temp_dir().join(format!("qb-mutate-{}", std::process::id()));
    let mut rng = Rng(0x5eed_1234_abcd_0001);
    let mut checked = 0usize;
    for round in 0..40 {
        let mut files = base();
        // One round is the unmodified corpus (the control); the rest carry 1..=3 edits.
        let edits = if round == 0 { 0 } else { 1 + round % 3 };
        let mut applied = Vec::new();
        for _ in 0..edits {
            applied.push(mutate(&mut files, &mut rng));
        }
        write(&dir, &files);

        // 1. INGESTION ANSWERS. Tolerant, so the corpus opens; an `Err` is still an answer.
        let mut db = match qubitick_db::open_in_memory() {
            Ok(d) => d,
            Err(e) => panic!("in-memory db: {e}"),
        };
        let ingested = qubitick_build::ingest_corpus(&mut db, &dir);
        if ingested.is_err() {
            continue;
        }
        let Ok(ctx) = qubitick_ctx::build_ctx(&db, "Era0/Strata0") else {
            continue;
        };
        // 2. THE PIPELINE ANSWERS FOR EVERY FILE IT SEES.
        let reported: Vec<(String, Vec<(String, bool)>)> =
            match qubitick_build::run_pipeline(&db, &qubitick_build::walk_qbit(&dir), None) {
                Ok(results) => results
                    .iter()
                    .map(|r| {
                        let path = r.path.clone();
                        let said = r
                            .diagnostics
                            .iter()
                            .map(|d| (d.code.clone(), d.is_error()))
                            .collect();
                        (path, said)
                    })
                    .collect(),
                Err(_) => continue,
            };
        for (path, said) in &reported {
            // 2. EVERY FILE CAN BE EMITTED (no panic, and an `Err` is an answer).
            let Ok(text) = qubitick_build::pipeline::emit_file(&ctx, &db, Path::new(path)) else {
                continue;
            };
            checked += 1;
            // NOTHING IS VERIFIED SILENTLY: a file the report has an ERROR for is answered.
            if said.iter().any(|(_, err)| *err) {
                continue;
            }
            // 3. AND NOTHING IS INVENTED WITHOUT SAYING SO. A module may write an `axiom` only
            // where the report admits the theorem: `proof.admitted` is that sentence.
            if text.lines().any(|l| l.trim_start().starts_with("axiom ")) {
                let admitted = said.iter().any(|(code, _)| code == "proof.admitted");
                assert!(
                    admitted,
                    "a module emitted an axiom and the report said nothing (round {round}, \
                     edits {applied:?}):\n{text}"
                );
            }
        }
    }
    assert!(checked > 40, "the sweep actually emitted files: {checked}");
    let _ = std::fs::remove_dir_all(&dir);
}