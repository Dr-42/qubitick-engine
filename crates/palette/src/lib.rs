//! `qubitick-palette` — per-symbol color from corpus usage (L2).
//!
//! Computes a stable HSL color for every symbol from its role/kind and the four
//! base-axis hues. Depends on `model` and `db`.

pub mod axes;
pub mod color;

pub use axes::{AXIS_CAP, AXIS_DELTA, AXIS_EMPTY, AXIS_FULL, axis_hue};
pub use color::{PaletteEntry, generate_palette};