/*
 * host/canvas/state.js — the canvas app's singletons, in one place.
 *
 * The canvas used to build its state inside one inline `<script>`: the corpus, the registry
 * of words, the wire declarations, the preferences, the tile and cluster lists, the camera.
 * Every module the migration has produced reads that state through hooks, so the state itself
 * could stay where it was — but then index.html can never be a shell, because a shell has no
 * state in it.
 *
 * So the singletons live here, and the rule for what belongs here is strict:
 *
 *   A VALUE THAT IS NEVER REASSIGNED. Objects and arrays whose IDENTITY matters — the corpus,
 *   the registry, the preferences, the tile list, the camera — live here and are mutated
 *   through their own fields. A `let` that gets reassigned (a flag, a saved-at timestamp) is
 *   not state this module can own; it stays with whoever flips it, or a reader here would
 *   hold a stale copy forever.
 *
 * That distinction is the whole reason this file is small: the arrays are shared, the scalars
 * are not.
 */

/** `?safe=1`: the page looks but does not touch — it restores nothing and saves nothing, so
 *  a workspace dragged into an unusable state can be repaired without being overwritten. */
export const SAFE = (() => {
  try { return new URLSearchParams(location.search).get('safe') === '1'; }
  catch (e) { return false; }
})();

/** The corpus, as the page loaded it. */
export const DATA = { files: [], graph: { nodes: [], edges: [] }, stats: null, symbols: [], fileText: {} };

/* ---------- registry: the vocabulary, as data ----------
   Which kinds a picker offers and in what order, which levels of detail exist, which tab
   widths an editor offers — the renderers keep the behaviour, the data keeps the words.
   js/defaults/registry.json fills this at boot. */
export const REG = {
  kinds: { order: [], default: 'editor', console: ['core', 'view', 'prefs'] },
  lod: { order: ['live', 'frozen', 'card', 'culled'] },
  editor: { tabSizes: [2, 4, 8], fontRange: { min: 11, max: 20, step: 1 }, lineHeightRange: { min: 1.2, max: 2, step: 0.05 } },
  atoms: { order: ['axiom', 'definition', 'theorem', 'calibration'] },
};

/** Read the registry file over the app's own route-independent static path. A missing file
 *  leaves the built-in words: the registry is data, and data being absent is not a crash. */
export async function loadRegistry({ fetch: fetchFn, url, onConsoleKinds, onFilled } = {}) {
  try {
    const r = await fetchFn(url || '/js/defaults/registry.json');
    if (!r.ok) return false;
    const j = await r.json();
    for (const k of Object.keys(REG)) if (j[k]) REG[k] = Object.assign({}, REG[k], j[k]);
    if (onConsoleKinds) onConsoleKinds(REG.kinds.console);
    if (onFilled) onFilled();
    return true;
  } catch (e) {
    return false;                    // the registry is optional: the built-in words stand
  }
}

/** The workspace's preferences: what a new tile is born with, and how a settings pane splits. */
export const UI = {
  editor: { fontSize: 13, lineHeight: 1.75, tabSize: 2, wrap: true },
  view: { lod: true, snap: true, wireMode: false, hud: true, hudAt: null, newTilePin: false, newTileSel: true, syntax: true },
  /* how a settings pane splits into two columns, by tile kind: null = one column (the
     shipped arrangement), a fraction = where the splitter sits. Saved with the workspace,
     so a pane you split stays split. */
  splits: { core: null, view: null, prefs: null },
};

/** Live editor handles, for live retuning (a preference change reaches every open editor). */
export const EDITORS = new Set();

/** The mirror key: one local document per browser, and the saver writes it. */
export const LS_KEY = 'qubitick.workspace';

/** The tiles and the clusters. Identity matters: the wire layer, the restore and the debug
 *  surface all hold this very array. */
export const panes = [];
export const clusters = [];

/** The camera: one object, mutated in place, because everything that moves reads it. */
export const cam = { x: 0, y: 0, z: 1 };

/** Wires drawn by hand — by instance, not by kind (a dragged wire names the tiles it joins). */
export const DYN_WIRES = [];

/** A pane's own port, registered under its instance id, so two switches stay two values. */
export const PORT_SRC = {};

/** The seeded wires: declarations by KIND, expanded to the live pairings when the layer arms.
 *  A declaration names a kind and a port; `expandWires` turns it into one wire per live tile
 *  of that kind, which is why a tile spawned away from its cluster is still wired in. */
export const WIRES = [
  { from: 'editor.buffer', to: 'render', type: 'value', note: 'KaTeX render' },
  { from: 'editor.buffer', to: 'ir', type: 'value', note: 'parse → IR' },
  { from: 'editor.file', to: 'lean', type: 'value', note: 'file → generated Lean' },
  { from: 'editor.cursor', to: 'goal', type: 'value', note: 'goal at cursor' },
  /* `tree.selection → stats` ("node → counters") was here, and it is gone because the port it
     landed on was not there: the stats row offered `highlight` and no stats tile has ever read it,
     so the value was dropped on delivery. With the row's port gone the wire would have no target to
     resolve at all, and `expandWires` skips a wire it cannot land (js/app/wires.js:101) — which is
     the silent kind of nothing this list must not carry. Both halves of the rule hold: a port on a
     row is a port the tile reads, and a wire that cannot land is not on this list. */
  { from: 'clock.tick', to: 'track', type: 'value', note: 'clock → track (ticked pair)' },
];
