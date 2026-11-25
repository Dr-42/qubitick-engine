//! Identifier and scope types (DataSchema §2, §3).
//!
//! `SymbolId` and `NodeId` are integer keys; glyph strings exist only at the
//! file boundary (`.qbit`) and the presentation boundary (rendered Unicode).
//! Bound variables are `VarName`s (plain strings) and never corpus symbols.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Globally-unique integer key for an atomic symbol (DataSchema §3.1).
pub type SymbolId = u32;

/// Globally-unique integer key for a node within the corpus (DataSchema §5).
pub type NodeId = u32;

/// A locally-bound variable name (DataSchema §3.4). Not a corpus symbol.
pub type VarName = String;

/// Corpus era (DataSchema §2). Squashes to the scope string `Era{0..5}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Era {
    Era0,
    Era1,
    Era2,
    Era3,
    Era4,
    Era5,
}

impl Era {
    pub fn index(self) -> u8 {
        match self {
            Era::Era0 => 0,
            Era::Era1 => 1,
            Era::Era2 => 2,
            Era::Era3 => 3,
            Era::Era4 => 4,
            Era::Era5 => 5,
        }
    }
}

/// Corpus strata, `0..=15`, contiguous within an era (DataSchema §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Strata(pub u8);

impl Strata {
    pub fn index(self) -> u8 {
        self.0
    }
}

/// A scope path such as `Era0/Strata0`. A symbol with scope `S` is visible to a
/// node with scope `N` iff `S` is `N` or an ancestor path of `N` (DataSchema §2).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Scope(pub String);

impl Scope {
    pub fn new(era: Era, strata: Strata) -> Self {
        Scope(format!("Era{}/Strata{}", era.index(), strata.index()))
    }

    /// The era index of this scope (`EraN/StrataM` → `N`; `EraN` → `N`; `''` → None).
    fn era_index(&self) -> Option<u8> {
        self.0
            .split('/')
            .next()
            .and_then(|seg| seg.strip_prefix("Era")?.parse().ok())
    }

    /// The strata index of this scope (`EraN/StrataM` → `M`; else None).
    fn strata_index(&self) -> Option<u8> {
        self.0
            .split('/')
            .nth(1)
            .and_then(|seg| seg.strip_prefix("Strata")?.parse().ok())
    }

    /// DataSchema §2: whether the symbol at `ancestor` is visible to the node at
    /// `self`. Visible when `ancestor` is global `''`, equal, an ancestor path, or
    /// shares the node's era at an equal-or-earlier strata (within-era substrate).
    pub fn visible_from(&self, ancestor: &Scope) -> bool {
        if ancestor.0.is_empty() {
            return true;
        }
        if self.0 == ancestor.0 || self.0.starts_with(&format!("{}/", ancestor.0)) {
            return true;
        }
        // Within an era, earlier (lower) strata are visible to later ones.
        match (
            self.era_index(),
            ancestor.era_index(),
            self.strata_index(),
            ancestor.strata_index(),
        ) {
            (Some(se), Some(ae), Some(ss), Some(as_)) => se == ae && as_ <= ss,
            _ => false,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Default for Scope {
    fn default() -> Self {
        Scope::new(Era::Era0, Strata(0))
    }
}