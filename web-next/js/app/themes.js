/*
 * themes.js — theme identity: which module a workspace is wearing, and how a saved palette
 * finds it again.
 *
 * Extracted from the inline script in web-next/index.html. A theme is DATA — one JSON module
 * per theme, named by an index — and this file holds the rules that keep an identity stable
 * while the data changes underneath it:
 *
 *   A SAVED WORKSPACE MAY NAME ITS THEME THREE WAYS. By id (current), by NAME (older
 *   documents wrote the human name), or by carrying a bare palette and no identity at all
 *   (oldest, from before themes were modules). All three restore, and only the first is
 *   written back — so an old workspace is not broken by being old, and a new one does not
 *   inherit the old shape.
 *
 *   AN EDITED PALETTE KEEPS ITS IDENTITY. Editing a shipped theme's colours forks nothing:
 *   the workspace says "midnight, edited", so "reset" can still mean "back to midnight,
 *   unedited" — which it could not if a colour change had thrown the provenance away.
 *
 *   A CUSTOM THEME MAY NOT TAKE A SHIPPED ID. Installing one would lose a shipped theme to a
 *   workspace, for ever and silently. It gets suffixed instead, and a name a user typed that
 *   collides is suffixed again until it does not.
 *
 *   FORGETTING A CUSTOM THEME DOES NOT FORGET A SHIPPED ONE. If the id belongs to the
 *   install, the install's theme comes back rather than leaving a hole in the picker.
 */

/** The identity a saved workspace is asking for, out of the three shapes it may carry. */
export function resolveThemeIdentity(saved, { byId, byName } = {}) {
  if (!saved) return { id: null, edited: null, how: 'none' };
  const named = saved.themeName
    ? (byName ? byName(String(saved.themeName)) : null)
    : null;
  const id = saved.themeId || (named && named.id) || null;
  const known = !!(id && byId && byId(id));
  const edited = (saved.theme && Object.keys(saved.theme).length) ? saved.theme : null;
  if (known) return { id, edited, how: saved.themeId ? 'id' : 'name' };
  /* No identity that this install knows. A bare palette is still a choice the user made,
     so it is applied — it just has no name to go back to. */
  if (edited) return { id: null, edited, how: 'palette-only' };
  return { id: null, edited: null, how: 'unknown' };
}

/** Is this id one of the shipped themes (as opposed to one this workspace made)? */
export function isShipped(id, index) {
  if (!id || !index) return false;
  return !!(index.themes || []).some((t) => t.id === id);
}

/** A stable id for a theme, from a name a human typed. */
export function slug(name) {
  return String(name || '').trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
}

/**
 * The id a theme may be registered under.
 *
 * `taken` is everything already in use (loaded modules AND the shipped index): an id has to
 * be free in both, or a registration would replace a shipped theme or another custom one.
 * A collision is suffixed (`-2`, `-3`, ...) rather than refused, because the thing the user
 * asked for was "a theme by this name", not "this exact id".
 */
export function freeThemeId(want, taken) {
  const base = slug(want) || 'custom';
  const held = taken instanceof Set ? taken : new Set(taken || []);
  if (!held.has(base)) return base;
  let n = 2;
  while (held.has(base + '-' + n)) n++;
  return base + '-' + n;
}

/**
 * Register a theme's identity. Returns the module as it should be stored.
 *
 * A custom theme is suffixed when its id belongs to the install; a shipped one keeps its id.
 * `source` records which of the two it is, because the picker groups by it.
 */
export function registerIdentity(mod, { custom = false, index, taken } = {}) {
  if (!mod || !mod.vars) return null;
  const out = { ...mod };
  if (custom && isShipped(out.id, index)) {
    // never take a shipped id: the install would lose a theme to a workspace
    out.id = freeThemeId(out.id + '-custom', taken);
  } else if (custom && taken && new Set(taken).has(out.id)) {
    out.id = freeThemeId(out.id, taken);
  }
  out.source = custom ? 'custom' : (out.source || 'qubitick');
  return out;
}

/**
 * What forgetting a theme means.
 *
 * A custom theme is dropped. A SHIPPED one is not forgettable — its id comes back from the
 * install rather than leaving a hole — and if the forgotten theme was the one in use, the
 * caller has to switch to something, which is the default. The answer says which of the
 * three happened so the caller does not have to re-derive it.
 */
export function forgetResult(id, { custom = [], index, currentId } = {}) {
  const isCustom = (custom || []).some((t) => t.id === id);
  const shipped = isShipped(id, index);
  const wasCurrent = currentId === id;
  const fallback = (wasCurrent || shipped) && index ? index.default : null;
  return { drop: isCustom, restore: shipped, fallback: wasCurrent ? (fallback || null) : null, wasCurrent };
}