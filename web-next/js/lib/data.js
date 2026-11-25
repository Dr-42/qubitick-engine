/**
 * lib/data.js — one way to read a data module.
 *
 * Modules live under js/defaults/ as plain JSON: themes, blueprints, brands,
 * registries. Reading them is the same three lines every time, so it lives here:
 * fetch, check, parse, and a cache keyed by url so twenty tiles asking for the
 * theme index fetch it once.
 */

const cache = new Map();

/**
 * Fetch and parse a JSON data module.
 * @param {string} url - absolute path, e.g. '/js/defaults/themes/index.json'
 * @param {{fresh?: boolean}} [opts] - `fresh` bypasses the cache
 * @returns {Promise<any>}
 */
export async function loadJson(url, opts = {}) {
  if (!opts.fresh && cache.has(url)) return cache.get(url);
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: ${res.status} ${res.statusText}`);
  const data = await res.json();
  cache.set(url, data);
  return data;
}

/** Forget one url (or everything) — used when a theme is edited in place. */
export function forget(url) {
  if (url) cache.delete(url);
  else cache.clear();
}

/**
 * Pure-JSON module readers that do not need the network — handed a parsed object,
 * they normalise it. Kept here so tests can exercise them without a browser.
 */

/** The twelve colour properties a theme owns, in editor order. */
export const THEME_VARS = ['bg', 'panel', 'panel2', 'inset', 'line', 'ink', 'muted', 'dim', 'cyan', 'green', 'amber', 'violet'];

/**
 * Relative luminance of a #rrggbb colour, 0..1 (sRGB, perceptual weights).
 * @param {string} hex
 * @returns {number}
 */
export function luminance(hex) {
  const h = String(hex || '').replace('#', '');
  const n = h.length === 3 ? h.split('').map((c) => c + c).join('') : h;
  if (!/^[0-9a-f]{6}$/i.test(n)) return 0;
  const lin = [0, 2, 4].map((i) => {
    const c = parseInt(n.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2];
}

/**
 * Is a theme light or dark? Prefers the declared mode, falls back to luminance
 * of its background — so a hand-written theme cannot lie about which it is.
 * @param {{mode?: string, vars?: object}} mod
 * @returns {'light'|'dark'}
 */
export function modeOf(mod) {
  if (mod && mod.mode === 'light') return 'light';
  if (mod && mod.mode === 'dark') return 'dark';
  return luminance(mod && mod.vars && mod.vars.bg) > 0.42 ? 'light' : 'dark';
}

/**
 * Check a theme module's shape. Returns human-readable problems; empty is good.
 * Themes are user-editable data, so this runs on every theme before it is applied.
 * @param {any} mod
 * @returns {string[]}
 */
export function validateTheme(mod) {
  const bad = [];
  if (!mod || typeof mod !== 'object') return ['theme is not an object'];
  if (!mod.id) bad.push('missing id');
  if (!mod.name) bad.push('missing name');
  if (!mod.vars || typeof mod.vars !== 'object') bad.push('missing vars');
  else for (const k of THEME_VARS) {
    const v = mod.vars[k];
    if (typeof v !== 'string' || !/^#[0-9a-f]{3,8}$/i.test(v)) bad.push(`vars.${k} is not a hex colour`);
  }
  if (mod.mode && mod.mode !== 'light' && mod.mode !== 'dark') {
    bad.push(`mode must be 'light' or 'dark', not '${mod.mode}'`);
  }
  if (mod.syntax && typeof mod.syntax !== 'object') bad.push('syntax must be an object');
  if (mod.blueprint && typeof mod.blueprint !== 'object') bad.push('blueprint must be an object');
  return bad;
}