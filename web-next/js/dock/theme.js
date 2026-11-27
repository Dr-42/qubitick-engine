/**
 * dock/theme.js — the shell wears the workspace's themes.
 *
 * A theme is DATA: one JSON module per theme under `js/defaults/themes/`, indexed by
 * `index.json`. The dock shell reads the same files the canvas reads and writes the same
 * twelve custom properties onto `:root` through the same function (`js/live/theme.js`), so a
 * theme added to the install is a theme both apps have — there is no second palette.
 *
 * The chunked import is lazy on purpose: a 20-theme install costs one JSON import for the
 * theme in use, and the picker fetches a name when it is opened.
 */

const indexPath = '/js/defaults/themes/index.json';
const themePath = (file) => '/js/defaults/themes/' + file;

let index = null;
const cache = new Map();

/** The theme index (`{ default, themes: [{id, path}] }`), fetched once. */
export async function themeIndex() {
  if (index) return index;
  const res = await fetch(indexPath, { cache: 'no-store' });
  if (!res.ok) throw new Error('theme index: HTTP ' + res.status);
  index = await res.json();
  return index;
}

/** One theme module by id. */
export async function loadTheme(id) {
  if (cache.has(id)) return cache.get(id);
  const idx = await themeIndex();
  const entry = (idx.themes || []).find((t) => t.id === id);
  if (!entry) throw new Error('no such theme: ' + id);
  const res = await fetch(themePath(entry.path), { cache: 'no-store' });
  if (!res.ok) throw new Error('theme ' + id + ': HTTP ' + res.status);
  const mod = await res.json();
  cache.set(id, mod);
  return mod;
}

/** Put a theme on the document, through the workspace's own applier. */
export async function applyTheme(id) {
  const { applyTheme: apply } = await import('../live/theme.js');
  const mod = await loadTheme(id);
  const root = document.documentElement;
  apply(root, mod);
  root.classList.add('dock-mode');
  return mod;
}

/** The theme id to start with: the stored choice, else the install's default. */
export async function initialTheme() {
  let stored = null;
  try { stored = window.localStorage.getItem('qubitick.dock.theme'); } catch { /* private mode */ }
  if (stored) return stored;
  const idx = await themeIndex();
  return idx.default || 'midnight';
}

export function rememberTheme(id) {
  try { window.localStorage.setItem('qubitick.dock.theme', id); } catch { /* nothing to do */ }
}