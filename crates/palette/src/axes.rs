//! Fixed the base hues for the four spectrum axes (Δ / ∩ / 𝟙 / ∅).

/// Δ (xor / event ring builder)
pub const AXIS_DELTA: f64 = 42.0;
/// ∩ (and / carrier combiner)
pub const AXIS_CAP: f64 = 192.0;
/// 𝟙 (full / identity element)
pub const AXIS_FULL: f64 = 140.0;
/// ∅ (empty)
pub const AXIS_EMPTY: f64 = 280.0;

/// The hue for a base axis glyph, if it is one of the four axes.
pub fn axis_hue(glyph: &str) -> Option<f64> {
    match glyph {
        "Δ" => Some(AXIS_DELTA),
        "∩" => Some(AXIS_CAP),
        "𝟙" => Some(AXIS_FULL),
        "∅" => Some(AXIS_EMPTY),
        _ => None,
    }
}