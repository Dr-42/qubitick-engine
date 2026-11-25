/**
 * lib/cite-text.js — Pure citation text helpers (M4.13).
 *
 * Ported from app.js 734–751 (delimIsSized, stripTrailingCite,
 * hasTrailingCite, rowState).
 *
 * These functions operate on plain strings — no CM6 or DOM dependency.
 */

/**
 * Citation reference regex (matches A1.2, T.BT.1, T5, D.AB.3, BI.2.4, etc.)
 */
export const CITE_RE = /\b(?:A\d+\.\d+|T\.[A-Z]{2}\.\d+|T\d+|D\.[A-Z]{2}\.\d+|BI\.\d+\.\d+)\b/g;

/**
 * Sized LaTeX delimiters that should not be treated as citation boundaries.
 */
const SIZED_DELIMS = [
  '\\left', '\\right', '\\bigl', '\\bigr', '\\Bigl', '\\Bigr',
  '\\biggl', '\\biggr', '\\Biggl', '\\Biggr',
];

/**
 * Check if position i in string s ends inside a sized delimiter
 * (\left, \right, \bigl, \bigr, \Bigl, \Bigr, \biggl, \biggr, \Biggl, \Biggr).
 */
export function delimIsSized(s, i) {
  const prefix = s.slice(0, i);
  return SIZED_DELIMS.some(d => prefix.endsWith(d));
}

/**
 * Strip a trailing citation bracket from a string.
 * Returns { body, cite } where cite is the trailing [...] part.
 */
export function stripTrailingCite(s) {
  let b = String(s == null ? '' : s).replace(/\s+$/, '');
  if (!b.endsWith(']') || delimIsSized(b, b.length - 1)) return { body: b, cite: '' };
  let depth = 0;
  for (let i = b.length - 1; i >= 0; i--) {
    if (b[i] === ']') {
      if (delimIsSized(b, i)) continue;
      depth++;
    } else if (b[i] === '[') {
      if (delimIsSized(b, i)) continue;
      depth--;
      if (depth === 0) return { body: b.slice(0, i).replace(/\s+$/, ''), cite: b.slice(i) };
    }
  }
  return { body: b, cite: '' };
}

/**
 * Check if a string has a trailing citation bracket.
 */
export function hasTrailingCite(s) {
  return stripTrailingCite(s).cite !== '';
}

/**
 * Extract the body of a proof row (text after `=` or `→`, minus trailing cite).
 * Returns null if the row is not a proof step.
 */
export function rowState(text) {
  const m = /^(?:_\s*)?(=|\u2192)\s*([\s\S]*)$/.exec(String(text).trim());
  if (!m) return null;
  return stripTrailingCite(m[2]).body.trim();
}
