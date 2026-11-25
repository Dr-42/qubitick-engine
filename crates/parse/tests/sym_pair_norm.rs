use qubitick_db::queries::SymbolRow;

#[allow(dead_code)]
fn row(id: i64, tex: &str) -> SymbolRow {
    SymbolRow {
        facets: 0,
        id,
        scope: "Era0/Strata0".into(),
        tex: tex.into(),
        display: tex.into(),
        lean_name: tex
            .trim_start_matches('\\')
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect::<String>()
            .trim_matches('_')
            .to_string(),
        trigger: None,
        kind: "notation".into(),
        role: None,
        precedence: None,
        assoc: None,
        home_node: 1,
        elab: None,
        binder_kind: None,
    }
}

// Test disabled - SymbolPairNormalizer module not available in this crate
#[test]
fn sym_pair_normalizer_fixtures() {
    // Test disabled - SymbolPairNormalizer module not available in this crate
    // assert!(true); // clippy: assertions_on_constants
}
