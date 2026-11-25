/**
 * live/theme.js — themes are data, not a dropdown.
 *
 * A theme is one JSON module under js/defaults/themes/ named by an index. This
 * module reads that index, loads modules lazily, and turns one into the small set
 * of CSS custom properties the whole UI paints from. It knows nothing about tiles
 * or menus: the caller hands it a root element and a theme, and it styles it.
 *
 * 'live/' because it is app-facing (it fetches), unlike lib/data.js which is pure.
 */
import { loadJson, forget } from '../lib/data.js';
import { THEME_VARS, luminance, modeOf, validateTheme } from '../lib/data.js';

// re-exported so a caller needs one import for 'what is a theme' and 'what does
// a theme look like on screen'
export { THEME_VARS, luminance, modeOf, validateTheme };

export const THEME_DIR = '/js/defaults/themes';

let index = null;                 // { default, themes: [{id, path}] }
const modules = new Map();        // id -> theme module

/**
 * Read the theme index (once).
 * @returns {Promise<{default: string, themes: {id: string, path: string}[]}>}
 */
export async function loadIndex() {
  if (index) return index;
  const j = await loadJson(`${THEME_DIR}/index.json`);
  const themes = (j.themes || []).filter((t) => t && t.id && t.path);
  index = { default: j.default || (themes[0] && themes[0].id) || null, themes };
  return index;
}

/**
 * Load one theme module by id.
 * @param {string} id
 * @returns {Promise<object>} the theme module
 */
export async function loadTheme(id) {
  if (modules.has(id)) return modules.get(id);
  const idx = await loadIndex();
  const entry = idx.themes.find((t) => t.id === id);
  if (!entry) throw new Error(`no such theme: ${id}`);
  const mod = await loadJson(`${THEME_DIR}/${entry.path}`);
  const bad = validateTheme(mod);
  if (bad.length) throw new Error(`theme ${id} is invalid: ${bad.join('; ')}`);
  modules.set(id, normalise(mod));
  return modules.get(id);
}

/**
 * Load every theme in the index, in parallel. The picker shows all of them, so
 * this is what boot uses; a single apply only needs loadTheme().
 * @returns {Promise<{index: object, themes: object[]}>}
 */
export async function loadAll() {
  const idx = await loadIndex();
  const themes = await Promise.all(idx.themes.map((t) => loadTheme(t.id).catch((e) => {
    console.warn('[theme]', e.message);
    return null;
  })));
  return { index: idx, themes: themes.filter(Boolean) };
}

/** Fill in what a hand-written theme may leave out. */
export function normalise(mod) {
  const out = { ...mod };
  out.mode = modeOf(mod);
  out.vars = { ...(mod.vars || {}) };
  out.syntax = { ...(mod.syntax || {}) };
  out.roles = mod.roles || THEME_VARS;
  out.blueprint = mod.blueprint || {};
  // a theme that names a colour role it does not have gets its accent, not grey
  const v = out.vars;
  const accent = v.cyan || v.amber || v.ink || '#888888';
  for (const key of ['comment', 'string', 'keyword', 'number', 'operator', 'type', 'func', 'variable', 'meta', 'atom']) {
    if (!out.syntax[key]) out.syntax[key] = key === 'variable' ? (v.ink || accent) : accent;
  }
  if (!out.syntax.invalid) out.syntax.invalid = '#ff5f6d';
  return out;
}

/**
 * The CSS custom properties a theme sets on :root.
 * @param {object} mod - a theme module
 * @returns {Record<string,string>}
 */
export function themeVars(mod) {
  const out = {};
  for (const k of THEME_VARS) {
    const v = mod.vars && mod.vars[k];
    if (typeof v === 'string' && v) out[`--${k}`] = v;
  }
  return out;
}

/**
 * Apply a theme to a root element: the twelve colour properties, the mode, and
 * the identity the rest of the app and the probes read back.
 * @param {HTMLElement} rootEl - usually document.documentElement
 * @param {object} mod - a theme module
 * @returns {{id: string, mode: string, vars: object}} what was applied
 */
export function applyTheme(rootEl, mod) {
  const vars = THEME_VARS.reduce((acc, k) => {
    if (mod.vars && mod.vars[k]) acc[k] = mod.vars[k];
    return acc;
  }, {});
  for (const [k, v] of Object.entries(themeVars(mod))) rootEl.style.setProperty(k, v);
  const mode = modeOf(mod);
  rootEl.dataset.theme = mod.id || mod.name || 'custom';
  rootEl.dataset.mode = mode;
  rootEl.dataset.themeName = mod.name || mod.id || 'Custom';
  rootEl.style.colorScheme = mode;
  if (rootEl.style.setProperty) {
    rootEl.style.setProperty('--theme-lum', String(Math.round(luminance(mod.vars && mod.vars.bg) * 100) / 100));
  }
  return { id: mod.id, mode, vars };
}

/**
 * Apply a bare {bg, panel, ...} object — what the theme tile's colour pickers
 * edit. Kept as its own entry point so an edited palette keeps the identity of
 * the theme it was derived from ('midnight (edited)') instead of becoming a
 * nameless custom blob in the saved workspace.
 * @param {HTMLElement} rootEl
 * @param {Record<string,string>} vars
 * @param {{base?: string}} [opts]
 */
export function applyVars(rootEl, vars, opts = {}) {
  const full = {};
  for (const k of THEME_VARS) if (vars[k]) full[k] = vars[k];
  for (const [k, v] of Object.entries(full)) rootEl.style.setProperty(`--${k}`, v);
  const mode = modeOf({ vars: full });
  rootEl.dataset.theme = opts.base || 'custom';
  rootEl.dataset.mode = mode;
  rootEl.dataset.themeEdited = '1';
  rootEl.style.colorScheme = mode;
  return { id: opts.base || 'custom', mode, vars: full };
}

/** Drop the cached copy of a theme, so an edited module can be reloaded. */
export function reload(id) {
  forget(`${THEME_DIR}/${id}.json`);
  modules.delete(id);
  index = null;
}

/** Every theme already loaded, in index order (used by the picker). */
export function loaded() {
  if (!index) return [];
  return index.themes.map((t) => modules.get(t.id)).filter(Boolean);
}

/** The index's default theme id. */
export function defaultId() {
  return (index && index.default) || 'midnight';
}

/**
 * The blueprint a theme asks for, with the fallbacks the field needs.
 * @param {object} mod
 * @returns {{preset: string, minor: number, minorAlpha: number, majorAlpha: number,
 *            tickAlpha: number, dotAlpha: number, hatch: number, border: number,
 *            label: number, accent: string, ink: string, brand: string|null}}
 */
export function blueprintOf(mod) {
  const bp = (mod && mod.blueprint) || {};
  const dark = modeOf(mod) === 'dark';
  return {
    preset: bp.preset || 'graph',
    minor: Number(bp.minor) > 0 ? Number(bp.minor) : 8,
    minorAlpha: num(bp.minorAlpha, dark ? 0.1 : 0.16),
    majorAlpha: num(bp.majorAlpha, dark ? 0.2 : 0.26),
    tickAlpha: num(bp.tickAlpha, 0.32),
    dotAlpha: num(bp.dotAlpha, 0.32),
    hatch: num(bp.hatch, 0.05),
    border: num(bp.border, dark ? 0.16 : 0.2),
    label: num(bp.label, dark ? 0.2 : 0.26),
    accent: bp.accent || 'cyan',
    ink: bp.ink || 'ink',
    brand: brandOf(mod),
  };
}

/**
 * Normalise a theme's `brand`: either the id of a wordmark module, or an object
 * naming one plus how the banner is printed. A theme that ships no banner is
 * quiet, not broken.
 * @param {object} mod
 * @returns {{mark: string, rotate: number, border: boolean, scale: number,
 *            place: string, opacity: number}|null}
 */
export function brandOf(mod) {
  const b = mod && mod.brand;
  if (!b) return null;
  if (typeof b === 'string') return { mark: b, ...BRAND_DEFAULTS };
  if (!b.mark) return null;
  return { ...BRAND_DEFAULTS, ...b };
}
export const BRAND_DEFAULTS = { rotate: -7, border: true, scale: 1, place: 'center', opacity: 1 };

function num(v, fallback) {
  return Number.isFinite(Number(v)) ? Number(v) : fallback;
}