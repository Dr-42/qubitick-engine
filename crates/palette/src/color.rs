//! Per-symbol color from corpus usage and ring composition (CodeArch §5.6).

use std::collections::HashMap;

use qubitick_db::queries::{self, SymbolRow};
use serde::Serialize;

use crate::axes::{AXIS_CAP, AXIS_DELTA, AXIS_EMPTY, AXIS_FULL, axis_hue};

/// A computed palette entry for one symbol.
#[derive(Debug, Clone, Serialize)]
pub struct PaletteEntry {
    pub symbol: String,
    pub lean: String,
    pub kind: String,
    pub color: String,
    pub hue: f64,
    pub usage: u64,
}

const ROLES: [(&str, (f64, f64, f64)); 8] = [
    ("prefix", (262.0, 78.0, 68.0)),
    ("pairfn", (210.0, 78.0, 68.0)),
    ("notation", (318.0, 72.0, 66.0)),
    ("type", (140.0, 58.0, 62.0)),
    ("scalar", (24.0, 60.0, 64.0)),
    ("scalarop", (24.0, 60.0, 64.0)),
    ("bracketfn", (48.0, 82.0, 60.0)),
    ("norm", (200.0, 78.0, 68.0)),
];

fn role_style(kind: &str) -> (f64, f64, f64) {
    ROLES
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, v)| *v)
        .unwrap_or((200.0, 70.0, 65.0))
}

/// Composite hue from the ratio of base-axis glyphs appearing in a definition.
fn composition_hue(definition: &str) -> Option<f64> {
    let mut x = 0.0f64;
    let mut y = 0.0f64;
    let mut w = 0.0f64;
    for (g, h) in [
        ("Δ", AXIS_DELTA),
        ("∩", AXIS_CAP),
        ("𝟙", AXIS_FULL),
        ("∅", AXIS_EMPTY),
    ] {
        let c = definition.matches(g).count() as f64;
        if c > 0.0 {
            let r = h.to_radians();
            x += c * r.cos();
            y += c * r.sin();
            w += c;
        }
    }
    if w == 0.0 {
        return None;
    }
    Some((y.atan2(x).to_degrees() + 360.0) % 360.0)
}

fn hsl_hex(h: f64, s: f64, l: f64) -> String {
    let s = (s.clamp(0.0, 100.0) / 100.0).clamp(0.0, 1.0);
    let l = (l.clamp(0.0, 100.0) / 100.0).clamp(0.0, 1.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - ((hp % 2.0) - 1.0).abs());
    let (r, g, b) = match hp as u8 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let f = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    format!("#{:02x}{:02x}{:02x}", f(r), f(g), f(b))
}

/// Compute the palette for all symbols in the corpus DB.
pub fn generate_palette(conn: &qubitick_db::Db) -> anyhow::Result<Vec<PaletteEntry>> {
    // Every symbol the corpus has. A palette is a list of the glyphs the OPEN PROJECT
    // writes; asking the empty (global) scope listed the reserved `=` and nothing else,
    // whatever the corpus declared.
    let rows = queries::symbols_all(conn)?;
    // Definition RHS is reconstructed from the emitted surface; the palette
    // currently keys off role/axis style. Kept as an empty map so the
    // composition-hue enhancement can be added later without changing callers.
    let definitions: HashMap<String, String> = HashMap::new();
    let usage: HashMap<i64, u64> = queries::symbol_usage_counts(conn)?
        .into_iter()
        .map(|(id, count)| (id, count as u64))
        .collect();
    let max_use = usage.values().copied().max().unwrap_or(1).max(1) as f64;

    let mut out = Vec::new();
    for r in &rows {
        out.push(palette_for(r, &definitions, &usage, max_use));
    }
    Ok(out)
}

fn palette_for(
    r: &SymbolRow,
    definitions: &HashMap<String, String>,
    usage: &HashMap<i64, u64>,
    max_use: f64,
) -> PaletteEntry {
    let definition = definitions.get(&r.lean_name);
    let (hue, sat0, light0) = match (definition, axis_hue(&r.display)) {
        (Some(d), _) => match composition_hue(d) {
            Some(h) => (h, 88.0, 66.0),
            None => role_style(&r.kind),
        },
        (None, Some(h)) => (h, 90.0, 64.0),
        (None, None) => role_style(&r.kind),
    };
    let count = *usage.get(&r.id).unwrap_or(&0) as f64;
    let v = if max_use <= 1.0 {
        1.0
    } else {
        0.45 + 0.55 * (count.ln_1p() / max_use.ln_1p())
    };
    let sat = sat0 * v;
    let light = 50.0 + (light0 - 50.0) * (0.55 + 0.45 * v);
    PaletteEntry {
        symbol: r.display.clone(),
        lean: r.lean_name.clone(),
        kind: r.kind.clone(),
        color: hsl_hex(hue, sat, light),
        hue: hue.round(),
        usage: count as u64,
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hue_is_css_hex() {
        let c = hsl_hex(42.0, 60.0, 50.0);
        assert!(c.starts_with('#') && c.len() == 7, "got {c}");
        assert!(c.chars().skip(1).all(|ch| ch.is_ascii_hexdigit()));
    }

    #[test]
    fn role_fallback_used_for_unknown() {
        assert_eq!(role_style("madeup"), (200.0, 70.0, 65.0));
    }
}