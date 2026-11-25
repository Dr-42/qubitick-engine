//! Small tag enums shared across node, proof, and definition types.

use serde::{Deserialize, Serialize};

/// Phys | Math division tag carried by Axiom, Definition, Theorem, and Census
/// nodes (DataSchema §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PhysOrMath {
    Phys,
    Math,
}

/// Census charter tag: `A` (structural) or `B` (census) (DataSchema §5.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CensusCharter {
    A,
    B,
}

/// Relation used by a proof `ChainRow` (DataSchema §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StepRel {
    Eq,
    Arrow,
}