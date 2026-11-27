//! A NODE'S IDENTITY IS ITS TIER AND ITS NUMBER.
//!
//! A corpus that grows by tier (`D.BT.0.1`, `D.QB.0.1`, `T.HD.14.3`) numbers within the tier, so
//! two nodes of one kind legitimately carry the same number. The id the author writes is what a
//! citation names: the header's text without its last segment is the TIER, the last segment is
//! the number, and both are part of the identity.
//!
//! Before this, the number alone was the identity (every digit concatenated, so `T.HD.14.3` was
//! 143), which made `[D.BT.0.1]` and `[D.QB.0.1]` one citation — resolved to whichever node the
//! walk reached first. A corpus that numbers inside a tier could not be cited at all.

use std::path::{Path, PathBuf};

fn corpus(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qb-tier-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Era0/Strata0/Definitions")).unwrap();
    std::fs::create_dir_all(dir.join("Era0/Strata0/Theorems")).unwrap();
    std::fs::write(dir.join("Canon.qbit"), format!("P. tier {tag}\nId: tier{tag}\n")).unwrap();
    let defs = dir.join("Era0/Strata0/Definitions");
    // One carrier, declared once.
    std::fs::write(defs.join("D_BT_0_0_Or.qbit"), "D.BT.0.0. Or\na \\vee b\nRole: connective\n").unwrap();
    std::fs::write(defs.join("D_BT_0_1_Iota.qbit"), "D.BT.0.1. Iota\n\\iota\nSema:\n\\iota = \\iota_0 \\vee (\\bullet \\iota)\n").unwrap();
    std::fs::write(defs.join("D_BT_0_2_Zero.qbit"), "D.BT.0.2. Zero\n\\iota_0\nType: \\iota\n").unwrap();
    std::fs::write(defs.join("D_BT_0_3_Succ.qbit"), "D.BT.0.3. Succ\n\\bullet a\nType: \\iota \\to \\iota\n").unwrap();
    std::fs::write(defs.join("D_BT_0_4_Land.qbit"), "D.BT.0.4. Land\na \\land b\nRole: connective\n").unwrap();
    // TWO definitions, both numbered 1 — one per tier.
    std::fs::write(
        defs.join("D_BT_0_5_Plus.qbit"),
        "D.BT.0.5. Plus\na \\oplus b\nSema:\n(\\iota_0 \\oplus b = b) \\land ((\\bullet a) \\oplus b = \\bullet (a \\oplus b))\n",
    )
    .unwrap();
    std::fs::write(
        defs.join("D_QB_0_1_Plus2.qbit"),
        "D.QB.0.1. Plus2\na \\otimes b\nSema:\n(\\iota_0 \\otimes b = b) \\land ((\\bullet a) \\otimes b = \\bullet (a \\otimes b))\n",
    )
    .unwrap();
    dir
}

fn theorems(dir: &Path, bt_cite: &str, qb_cite: &str) {
    let docs = dir.join("Era0/Strata0/Theorems");
    std::fs::write(
        docs.join("T_BT_1_UsesBT.qbit"),
        format!("T.BT.1. UsesBT\n\\forall a : \\iota, \\iota_0 \\oplus a = a\nProof:\n\\iota_0 \\oplus a\n  = a  [{bt_cite}]\n"),
    )
    .unwrap();
    std::fs::write(
        docs.join("T_QB_1_UsesQB.qbit"),
        format!("T.QB.1. UsesQB\n\\forall a : \\iota, \\iota_0 \\otimes a = a\nProof:\n\\iota_0 \\otimes a\n  = a  [{qb_cite}]\n"),
    )
    .unwrap();
}

/// Every error message a build produced, and whether every file verified.
fn build(dir: &Path) -> (bool, Vec<String>, Vec<String>) {
    let mut db = qubitick_db::open_in_memory().unwrap();
    qubitick_build::ingest_corpus(&mut db, dir).unwrap();
    let ground = qubitick_build::ground::for_check(dir).unwrap();
    let results =
        qubitick_build::run_pipeline(&db, &qubitick_build::walk_qbit(dir), Some(&ground)).unwrap();
    let errors = results
        .iter()
        .flat_map(|r| r.diagnostics.iter())
        .filter(|d| d.is_error())
        .map(|d| d.message.clone())
        .collect();
    let duplicates = results
        .iter()
        .flat_map(|r| {
            r.diagnostics
                .iter()
                .filter(|d| d.code == "corpus.duplicate")
                .map(|d| d.message.clone())
        })
        .collect::<Vec<_>>();
    (
        !results.is_empty() && results.iter().all(|r| r.verified),
        errors,
        duplicates,
    )
}

#[test]
fn two_tiers_numbering_the_same_id_are_two_nodes() {
    let dir = corpus("same-id");
    // Each theorem cites its OWN tier's definition, both numbered 1.
    theorems(&dir, "D.BT.0.5", "D.QB.0.1");
    let (verified, errors, duplicates) = build(&dir);
    assert!(verified, "a corpus that numbers per tier builds: {errors:?}");
    assert!(
        duplicates.is_empty(),
        "and the two 1s are not a duplicate claim: {duplicates:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_citation_that_names_no_tier_still_resolves_by_number() {
    // `[D.1]` names no tier: the wildcard the flat corpora write. It resolves to the tier that
    // numbers a 1 — the behaviour every corpus written before tiers relied on.
    let dir = corpus("wildcard");
    theorems(&dir, "D.5", "D.1");
    let (verified, errors, _) = build(&dir);
    assert!(verified, "an untiered citation keeps working: {errors:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_citation_of_a_tier_nothing_claims_is_reported_with_that_tier() {
    let dir = corpus("dangling");
    theorems(&dir, "D.ZZ.0.1", "D.QB.0.1");
    let (verified, errors, _) = build(&dir);
    assert!(!verified, "a citation of a tier nothing claims resolves to nothing");
    let said = errors.join(" | ");
    assert!(
        said.contains("D.ZZ.0.1"),
        "the refusal quotes the id the author wrote, tier and all: {said}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}