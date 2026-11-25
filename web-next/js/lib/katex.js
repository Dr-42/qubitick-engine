/**
 * katex.js — KaTeX math rendering helpers (M4.02).
 *
 * Ports of app.js 268–319 (texifyGlyphs, mathHtml, loadKatexMacros).
 *
 * Exports:
 *   texifyGlyphs(s, symbolMap) — pure glyph→TeX substitution
 *   mathHtml(s, display) — render via window.katex or escaped fallback
 *   loadKatexMacros(ws) — fetch symbols via endpoint, build macro/glyph tables
 */

// Module-level state (populated by loadKatexMacros)
let glyphTex = {};
let glyphTexKeys = [];
let katexMacros = {};

/**
 * Pure: replace unicode glyphs with their TeX equivalents.
 * @param {string} s - input string
 * @param {object} symbolMap - map of glyph → TeX string
 * @returns {string}
 */
export function texifyGlyphs(s, symbolMap) {
  let t = String(s == null ? "" : s);
  const keys = Object.keys(symbolMap).sort((a, b) => b.length - a.length);
  for (const g of keys) {
    if (t.indexOf(g) >= 0) t = t.split(g).join(symbolMap[g]);
  }
  return t;
}

/**
 * Render math via KaTeX, with escaped-text fallback.
 * @param {string} s - LaTeX source
 * @param {boolean|string} display - display mode flag
 * @returns {string} HTML
 */
export function mathHtml(s, display) {
  const src = texifyGlyphs(s, glyphTex);
  if (!src) return "";
  if (typeof window !== 'undefined' && window.katex) {
    try {
      return window.katex.renderToString(src, {
        throwOnError: false,
        macros: katexMacros,
        displayMode: !!display,
        strict: false,
        trust: false,
        maxExpand: 1000,
      });
    } catch (e) {
      // fall through to fallback
    }
  }
  return `<span class="math-fallback">${escapeHtml(src)}</span>`;
}

/**
 * HTML-escape a string for safe fallback rendering.
 */
export function escapeHtml(s) {
  return String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

/**
 * Load KaTeX macros from the server via the symbols endpoint.
 * @param {object} ws - Workspace API (provides ws.endpoint('symbols').call.invoke)
 * @returns {Promise<void>}
 */
export async function loadKatexMacros(ws) {
  try {
    const endpoint = ws.endpoint('symbols');
    const syms = await endpoint.call.invoke({});
    const macros = {};
    const glyphs = {};

    for (const [k, info] of Object.entries(syms || {})) {
      if (!info || !info.tex) continue;
      if (k && k[0] === '\\' && /^\\[A-Za-z]+$/.test(k) && info.tex !== k && !(k in macros)) {
        macros[k] = info.tex;
      }
      const g = info.symbol;
      if (g && [...g].some(ch => ch.codePointAt(0) > 0x2000) && !(g in glyphs)) {
        glyphs[g] = info.tex;
      }
    }

    // Display-only successor glyphs
    if ('⁺' in glyphs) glyphs['⁺'] = '{}^{+}';;
    if ('⁻' in glyphs) glyphs['⁻'] = '{}^{-}';;

    // Probe macros for infinite loops and drop bad ones
    if (typeof window !== 'undefined' && window.katex) {
      for (const k of Object.keys(macros)) {
        try {
          window.katex.renderToString(k, {
            throwOnError: true,
            macros: { [k]: macros[k] },
            strict: false,
            trust: false,
            maxExpand: 50,
          });
        } catch (e) {
          if (/Too many expansions|infinite loop/i.test(String(e && e.message))) {
            delete macros[k];
          }
        }
      }
    }

    glyphTex = glyphs;
    glyphTexKeys = Object.keys(glyphs).sort((a, b) => b.length - a.length);
    katexMacros = macros;

    // Re-render any [data-tex] elements
    if (typeof document !== 'undefined') {
      document.querySelectorAll('[data-tex]').forEach((el) => {
        el.innerHTML = mathHtml(el.getAttribute('data-tex'));
      });
    }
  } catch {
    // Silent failure — KaTeX macros optional
  }
}

// Expose internal state for testing
export { glyphTex, glyphTexKeys, katexMacros };
