/*
 * app/prefs.js — what the editor and view preferences MEAN.
 *
 * Extracted from the inline script in web-next/index.html, where the two functions that
 * apply them were also half maintenance code for a settings panel that had already moved into
 * tiles: every `$('#ed-font')`/`$('#c-snap')` lookup in them returned null (the ids are not in
 * the markup any more), so a third of both functions was dead weight that made the live part
 * hard to see.
 *
 * The editor preferences are three CSS custom properties plus the live editors' own options.
 * The view preferences are DERIVED switches: `lod` and `snap` are on unless written false,
 * so a workspace saved by an older build (which never wrote them) still behaves; `wireMode`
 * is off unless written true. The derivation lives here, in one place, because two places
 * that disagree about the default are how a preference appears to reset itself.
 */

/**
 * Apply the editor preferences: the document's three custom properties, and every live
 * editor's own options.
 * @param {CSSStyleDeclaration} style   the root style (the CSS variables land here)
 * @param {Iterable<object>} editors    live editor handles (each may have `setOpts`)
 * @param {{fontSize:number, lineHeight:number, tabSize:number, wrap:boolean}} prefs
 * @returns {object} the values as applied, so a caller can echo them
 */
export function applyEditorPrefs(style, editors, prefs) {
  const e = prefs || {};
  const fontSize = e.fontSize != null ? e.fontSize : 13;
  const lineHeight = e.lineHeight != null ? e.lineHeight : 1.75;
  const tabSize = e.tabSize != null ? e.tabSize : 2;
  const wrap = e.wrap !== false;
  if (style) {
    style.setProperty('--ed-font', fontSize + 'px');
    style.setProperty('--ed-lh', String(lineHeight));
    style.setProperty('--ed-tab', String(tabSize));
  }
  for (const ed of editors || []) {
    // one bad editor must not stop the others being retuned
    if (ed && typeof ed.setOpts === 'function') {
      try { ed.setOpts({ wrap, tabSize }); } catch (err) { /* the editor's own business */ }
    }
    /* A TILE IS MORE THAN AN EDITOR: the check strip under it reads `suggest`, which is neither a
       font nor a tab, so `setOpts` cannot tell it (js/live/check-strip.js asks for the rewrites the
       setting turns on). The hook sits on the same handle, so it is dropped with the tile that owns
       it — and one tile that throws must not stop the others being told. */
    if (ed && typeof ed.onPrefs === 'function') {
      try { ed.onPrefs(e); } catch (err) { /* that tile's own business */ }
    }
  }
  return { fontSize, lineHeight, tabSize, wrap };
}

/**
 * The switches the view preferences stand for. Written as "on unless false" on purpose: an
 * older workspace has no such keys, and a preference that defaults to OFF for a document
 * that never mentioned it is a feature that disappears on reload.
 */
/**
 * What a save does besides writing the file.
 *
 * The symbol injection happens on the engine's side either way (`POST /api/file`), so this is only the
 * follow-up: nothing, the file's own check, or a full rebuild. `'none'` is the default because a save
 * that silently starts a build is a save that hangs the interface for seconds without being asked.
 */
export const SAVE_MODES = [
  { id: 'none', label: 'do nothing on save' },
  { id: 'eval', label: 'eval file on save' },
  { id: 'rebuild', label: 'rebuild project on save' },
];

/** The mode a stored preference means, or the default. */
export const saveModeOf = (v) => (SAVE_MODES.some((m) => m.id === v) ? v : 'none');

export function viewState(ui) {
  const v = (ui && ui.view) || {};
  return {
    lod: v.lod !== false,
    snap: v.snap !== false,
    wireMode: v.wireMode === true,
    newTilePin: v.newTilePin === true,
    newTileSel: v.newTileSel !== false,
    syntax: v.syntax !== false,
    saveMode: saveModeOf(v.saveMode),
  };
}