//! Scope visibility helpers (DataSchema §2).

use qubitick_model::Scope;

/// True if a symbol declared at `sym_scope` is visible to a node at `node_scope`.
/// A symbol with scope `S` is visible to a node with scope `N` iff `S` is `N` or
/// an ancestor path of `N`. `''` (global) is visible everywhere.
pub fn scope_visible(sym_scope: &str, node_scope: &str) -> bool {
    if sym_scope.is_empty() {
        return true;
    }
    Scope(node_scope.to_string()).visible_from(&Scope(sym_scope.to_string()))
}

/// The depth of a scope path (`Era0/Strata0` → 2; global → 0).
pub fn scope_depth(scope: &str) -> usize {
    if scope.is_empty() {
        0
    } else {
        scope.split('/').count()
    }
}

/// The scope of a node given its era and strata as a `Scope`.
pub fn scope_of_node(era: qubitick_model::Era, strata: qubitick_model::Strata) -> Scope {
    Scope::new(era, strata)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_rule() {
        assert!(scope_visible("", "Era1/Strata3")); // global
        assert!(scope_visible("Era0/Strata0", "Era0/Strata0")); // own
        assert!(scope_visible("Era0", "Era0/Strata1")); // ancestor
        assert!(!scope_visible("Era1/Strata0", "Era0/Strata0")); // different era
        assert!(scope_visible("Era1/Strata1", "Era1/Strata2")); // same era, earlier strata
        assert!(!scope_visible("Era1/Strata2", "Era1/Strata1")); // later strata not visible to earlier
        assert_eq!(scope_depth("Era0/Strata0"), 2);
        assert_eq!(scope_depth(""), 0);
    }
}