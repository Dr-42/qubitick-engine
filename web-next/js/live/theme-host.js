/*
 * live/theme-host.js — which theme is on, and every way that answer can change.
 *
 * A theme is one JSON module (`js/defaults/themes/<id>.json`, named by `index.json`), and
 * `js/live/theme.js` turns it into the twelve CSS custom properties the whole UI paints
 * from. This module decides WHICH module is on and tells everyone who cares: the field, the
 * tiles that show a theme, the workspace document. Nothing here names a colour, and nothing
 * here knows what a tile is.
 *
 * The hard part is not applying a theme, it is the ORDER things arrive in, because four
 * independent things can decide the answer and none of them waits for the other three:
 *
 *   · the theme FILES (a fetch, so they can be late),
 *   · the saved WORKSPACE (a fetch too, and it may name a theme, a bare palette, or
 *     nothing),
 *   · the USER (a pick, an edit in a colour picker, a save),
 *   · the FIELD (its own module, so the theme can be read before there is a field to taste).
 *
 * So the state is small and explicit: `pending` holds a saved choice that arrived before the
 * files could honour it, `user` records that somebody has chosen (so the default does not
 * overwrite it), and every entry point ends by applying ONE module through `applyModule`,
 * which is the only place that touches the document. Applying a bare palette keeps the
 * identity of the theme it came from — a workspace says "midnight, edited" rather than
 * losing the provenance — and still counts as a choice.
 *
 * Identity rules (a custom theme may not take a shipped id, a colliding one is suffixed, a
 * forgotten shipped theme comes back from the install) are `js/app/themes.js`; they were
 * extracted before this module and are pure. This is the state machine around them.
 */

import { registerIdentity, freeThemeId, forgetResult, isShipped as isShippedIn } from '../app/themes.js';

/** Load the theme module itself. A missing file is a reportable answer, not a crash. */
const importThemeModule = () => import('./theme.js');

/**
 * @param {object} host
 * @param {Element} host.root              where the CSS custom properties are written
 * @param {(m:string) => void} [host.toast]
 * @param {() => object|null} [host.field] the field instance, if the canvas has one
 * @param {() => void} [host.applyFieldPrefs]   re-apply the workspace's field choices
 * @param {(mod, vars) => void} [host.onApplied]  the app's own mirror of what is on
 * @param {() => void} [host.redraw]       re-draw the tiles that SHOW a theme
 * @param {() => object} [host.view]       the workspace's view prefs (field, banner)
 * @param {(o:object) => void} [host.diag] the boot diagnostics
 * @param {() => Promise<object>} [host.loadApi] how the theme module itself is loaded: the
 *        real import by default, injected by a test that wants the state machine without a
 *        network
 */
export function createThemeHost(host) {
  const { root } = host;
  const loadApi = host.loadApi || importThemeModule;
  const toast = host.toast || (() => {});
  const say = (o) => { if (host.diag) host.diag(o); };

  let api = null;                     // js/live/theme.js, once loaded
  let index = null;                   // { default, themes: [{ id, path }] }
  const mods = new Map();             // id -> loaded theme module
  const custom = [];                  // themes made in this workspace (kept by identity)
  let id = null;                      // the module currently applied
  let name = 'Midnight';              // its display name, for the header
  let user = false;                   // has anybody chosen?
  let pending = null;                 // a saved choice that beat the theme files

  const redraw = () => { if (host.redraw) host.redraw(); };
  const keys = () => (api ? api.THEME_VARS : []);

  /** The twelve variables as they actually are on :root — an edited palette and a module
   *  look the same to everything downstream, so there is no second copy of the palette. */
  function readVars() {
    const cs = getComputedStyle(root), out = {};
    for (const k of keys()) { const v = cs.getPropertyValue('--' + k).trim(); if (v) out[k] = v; }
    return out;
  }

  /** The one place a theme reaches the document. */
  function applyModule(mod) {
    if (!mod || !mod.vars || !api) return null;
    id = mod.id;
    name = mod.name || mod.id;
    const applied = api.applyTheme(root, mod);
    if (host.onApplied) host.onApplied(mod, Object.assign({}, applied.vars));
    const f = host.field && host.field();
    if (f) {
      f.setTheme(mod, api.blueprintOf(mod));
      if (host.applyFieldPrefs) host.applyFieldPrefs();
    }
    redraw();
    return applied;
  }

  /** Apply a bare {bg, panel, ...} — what the colour pickers edit — keeping the identity of
   *  the theme it came from, so an edited palette is still a choice with a provenance. */
  function applyVars(vars) {
    if (!vars) return null;
    if (!api) {                                 // a saved palette that beat the theme files
      pending = Object.assign({}, pending || {}, { theme: vars });
      user = true;
      return null;
    }
    const def = index && mods.get(index.default);
    const base = current() || def || { id: id || 'custom', name: name || 'Custom', vars: {}, blueprint: {}, brand: null };
    const mod = Object.assign({}, base, { vars: Object.assign({}, base.vars, vars) });
    root.dataset.themeEdited = '1';
    user = true;
    return applyModule(mod);
  }

  /** Choose a theme module by id. Async because a module is a file: the first time an id is
   *  used it is fetched, after which applying it is immediate. */
  async function pick(nextId, opts = {}) {
    if (!api) {
      try { api = await loadApi(); }
      catch (e) { toast('theme module unavailable'); return null; }
      if (!api) { toast('theme module unavailable'); return null; }
    }
    let mod = mods.get(nextId);
    if (!mod) {
      try { mod = await api.loadTheme(nextId); }
      catch (e) { toast('theme: ' + e.message); return null; }
      /* a theme is a FILE named by the index: an id the install does not have resolves to
         nothing, and a picker that reported success for it would leave the UI's own record
         (and the saved workspace) naming a theme that was never applied */
      if (!mod || !mod.vars) { toast('theme: no such theme (' + nextId + ')'); return null; }
      mods.set(mod.id, mod);
    }
    if (opts.user !== false) user = true;
    if (!opts.edited) delete root.dataset.themeEdited;
    applyModule(mod);
    const f = host.field && host.field();
    if (opts.space && f) f.setSpace(opts.space);
    return mod;
  }

  /** Read the index, load every module the picker shows, then put the default on — unless a
   *  saved workspace already chose one, in which case honour that. Runs alongside the
   *  settings restore: whichever gets there first leaves `pending`. */
  async function boot() {
    try {
      api = await loadApi();
      const all = await api.loadAll();
      index = all.index;
      for (const t of all.themes) mods.set(t.id, t);
      if (pending) {                              // a saved workspace won the race with the files
        const s = pending; pending = null;
        restore(s);
      } else if (!user) {
        const first = mods.get(index.default) || all.themes[0] || null;
        if (first) applyModule(first);
      }
    } catch (e) {
      console.warn('[theme]', e.message);
      toast('themes unavailable: ' + e.message);
    }
    redraw();
    say({ themes: index ? index.themes.length : mods.size, customThemes: custom.length });
  }

  /** Restore what a saved workspace recorded: a theme by id, by name (older), or a bare
   *  palette (oldest). All three restore; only the first is written back. */
  function restore(saved) {
    if (!saved) return null;
    if (!api) { pending = saved; return null; }
    const view = host.view ? host.view() : null;
    if (view) {
      if (saved.field) view.field = saved.field;
      if (saved.banner) view.banner = saved.banner;
    }
    if (Array.isArray(saved.customThemes)) {
      for (const t of saved.customThemes) if (t && t.vars) register(t, { custom: true });
    }
    const byName = saved.themeName
      ? [...mods.values()].find((m) => (m.name || '').toLowerCase() === String(saved.themeName).toLowerCase())
      : null;
    const wantId = saved.themeId || (byName && byName.id) || null;
    if (wantId && mods.has(wantId)) {
      user = true;
      id = wantId;
      const mod = mods.get(wantId);
      const applied = api.applyTheme(root, mod);
      if (host.onApplied) host.onApplied(mod, Object.assign({}, applied.vars));
      const f = host.field && host.field();
      if (f) f.setTheme(mod, api.blueprintOf(mod));
      /* an edited palette is the saved theme: only the VALUES travel, the identity stays,
         so reset still means "back to midnight, unedited" */
      if (saved.theme && Object.keys(saved.theme).length) applyVars(saved.theme);
      if (host.applyFieldPrefs) host.applyFieldPrefs();
      redraw();
      return mod;
    }
    if (saved.theme && Object.keys(saved.theme).length) { user = true; return applyVars(saved.theme); }
    redraw();
    return null;
  }

  const current = () => mods.get(id) || null;
  const currentName = () => {
    const m = current();
    return (m && m.name) || name || 'Midnight';
  };
  /** Every theme the picker can show, shipped first, then the ones made here. */
  function cards() {
    const shipped = index ? index.themes.map((t) => mods.get(t.id)).filter(Boolean) : [];
    const mine = custom.filter((t) => mods.has(t.id));
    return shipped.concat(mine);
  }
  const isShipped = (want) => isShippedIn(want, index);

  /** Register a theme module. The identity rules are `js/app/themes.js`. */
  function register(mod, opts = {}) {
    if (!mod || !mod.vars) return null;
    const normalised = api ? api.normalise(mod) : mod;
    const taken = [...mods.keys(), ...((index && index.themes) || []).map((t) => t.id)];
    const m = registerIdentity(normalised, { custom: !!opts.custom, index, taken });
    mods.set(m.id, m);
    if (opts.custom) {
      const at = custom.findIndex((t) => t.id === m.id);
      if (at >= 0) custom[at] = m; else custom.push(m);      // in place: the list has readers
    }
    return m;
  }

  /** Forget a theme: a custom one is dropped, a SHIPPED one comes back from the install, and
   *  a forgotten theme that was in use is replaced by the default. */
  function forget(wantId) {
    const f = forgetResult(wantId, { custom, index, currentId: id });
    if (f.drop) {
      const at = custom.findIndex((t) => t.id === wantId);
      if (at >= 0) custom.splice(at, 1);
    }
    if (f.restore) mods.delete(wantId);
    if (f.fallback) pick(f.fallback, { user: true });
    redraw();
    return f;
  }

  /** A module the user is entitled to edit: the shipped ones are read-only on disk, so an
   *  edit forks them into a custom theme with an id that cannot collide. */
  function fork(mod, wantName) {
    if (!mod) return null;
    const base = (wantName || (mod.name || mod.id) + ' custom').trim();
    const taken = [...mods.keys(), ...((index && index.themes) || []).map((t) => t.id)];
    return Object.assign({}, JSON.parse(JSON.stringify(mod)), { id: freeThemeId(base, taken), name: base, source: 'custom' });
  }

  return {
    boot, pick, applyModule, applyVars, restore,
    keys, current, currentId: () => id, currentName, cards, isShipped,
    register, forget, fork, readVars,
    api: () => api,
    index: () => index,
    mods,
    custom,
    user: () => user,
    setUser: (v) => { user = !!v; },
    pending: () => pending,
  };
}