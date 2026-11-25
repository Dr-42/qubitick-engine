/**
 * lib/symbols.js — Shared symbol data layer (M4.04).
 *
 * Ported from app.js 320–361 (symbol palette/colors) and 538–560 (symbolEntry).
 * No DOM — pure data functions.
 *
 * Exports:
 *   loadSymbols(ws) — cached per ws, loads from symbols.get endpoint
 *   symbolEntries(dict) — returns symbol entries array
 *   triggerFor(name) — returns trigger character for a symbol name
 *   loadPalette(ws) — loads raw palette from palette.get endpoint
 *   symbolEntry(dict, k) — looks up a symbol entry by key
 */

// Per-ws cache: WeakMap keyed by ws object
const symbolCache = new WeakMap();
const paletteCache = new WeakMap();

/**
 * Load symbols dict for a workspace (cached per ws).
 * @param {object} ws - Workspace API
 * @returns {Promise<object>} symbol dict {key: {symbol, tex, ...}}
 */
export async function loadSymbols(ws) {
  if (symbolCache.has(ws)) return symbolCache.get(ws);
  try {
    const endpoint = ws.endpoint('symbols');
    const data = await endpoint.call.invoke({});
    symbolCache.set(ws, data);
    return data;
  } catch {
    const empty = {};
    symbolCache.set(ws, empty);
    return empty;
  }
}

/**
 * Get symbol entries as an array.
 * @param {object} dict - symbol dict
 * @returns {Array<{key, symbol, tex}>}
 */
export function symbolEntries(dict) {
  return Object.entries(dict || {}).map(([key, v]) => {
    if (typeof v === 'string') return { key, symbol: v, tex: v };
    return { key, symbol: v.symbol || '', tex: v.tex || v.symbol || '' };
  });
}

/**
 * Return the trigger character for a symbol name.
 * @param {string} name - symbol key like '\\alpha'
 * @returns {string} trigger like '\\'
 */
export function triggerFor(name) {
  return name.startsWith('\\') ? '\\' : '';
}

/**
 * Load raw palette for a workspace (cached per ws).
 * @param {object} ws - Workspace API
 * @returns {Promise<Array<{symbol, color}>>}
 */
export async function loadPalette(ws) {
  if (paletteCache.has(ws)) return paletteCache.get(ws);
  try {
    const endpoint = ws.endpoint('palette');
    const data = await endpoint.call.invoke({});
    const palette = data.symbols || [];
    paletteCache.set(ws, palette);
    return palette;
  } catch {
    paletteCache.set(ws, []);
    return [];
  }
}

/**
 * Look up a symbol entry by key.
 * @param {object} dict - symbol dict
 * @param {string} k - key
 * @returns {object|null} {symbol, tex} or null
 */
export function symbolEntry(dict, k) {
  const v = dict && dict[k];
  if (v == null) return null;
  if (typeof v === 'string') return { symbol: v, tex: v };
  return { symbol: v.symbol || '', tex: v.tex || v.symbol || '' };
}
