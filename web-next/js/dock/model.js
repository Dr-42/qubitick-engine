/**
 * dock/model.js — the docking shell as DATA.
 *
 * The shell is five regions — left, centre, right, bottom, and the status strip — and every
 * region holds one or more TAB GROUPS. That is the whole model, and it is deliberately the
 * smallest shape that covers all three presets:
 *
 *   Minimal   left and right are railed (a 26px spine), bottom is one line.
 *   Standard  left 18%, centre 52%, right 30% split Goal/Lean, bottom 22% with three tabs.
 *   Power     left split tree/outline, centre split into two editors, right three ways,
 *             bottom two groups side by side.
 *
 * So a dock is `{ size, groups: [{ tabs: [panelId], active }], sizes, collapsed, hidden }`
 * and nothing else. Splitting a dock adds a group; tabbing adds a tab to a group; collapsing
 * sets `collapsed` — the three operations A0 asks for, each one field.
 *
 * A PANEL IS AN ID WITH A KIND. The id is stable (`diag`, `build`, `editor1`), the kind names
 * a pane module (`diag`, `build`, `editor`), and the two are separate because a preset may
 * open a second editor (`editor2`) without inventing a pane kind for it.
 */

/** Every panel the shell can show: its pane kind and the name the tab carries. */
export const PANELS = {
  tree: { kind: 'tree', title: 'Corpus' },
  outline: { kind: 'nodes', title: 'Outline' },
  editor1: { kind: 'editor', title: 'Editor' },
  editor2: { kind: 'editor', title: 'Editor · cited' },
  goal: { kind: 'goal', title: 'Goal' },
  steps: { kind: 'ir', title: 'Steps' },
  lean: { kind: 'lean', title: 'Lean' },
  render: { kind: 'render', title: 'Render' },
  diag: { kind: 'diag', title: 'Diagnostics' },
  build: { kind: 'build', title: 'Build' },
  graph: { kind: 'graph', title: 'Graph' },
  /* A1 lists "Symbols/Palette" as one panel: the corpus's own symbol table, which is what the
     `symbols` pane draws. There is no separate palette KIND — the add-a-pane palette is the
     design layer's, not a panel — so one panel, one kind. */
  symbols: { kind: 'symbols', title: 'Symbols' },
  stats: { kind: 'stats', title: 'Stats' },
};

/** The docks, in the order the shell paints them. `rail` is what a collapsed dock becomes. */
export const DOCKS = ['left', 'center', 'right', 'bottom'];

/** How narrow a collapsed dock is, in px: enough for a vertical title, no more. */
export const RAIL = 26;

/**
 * Per-dock limits, in px. A splitter cannot make a dock smaller than `min` or larger than
 * `max`; `max` is resolved against the viewport at drag time, so it is a FRACTION here and a
 * pixel number there (a 400px right dock is most of a small window).
 */
export const LIMITS = {
  left: { min: 140, maxFrac: 0.45, default: 240 },
  right: { min: 180, maxFrac: 0.55, default: 380 },
  bottom: { min: 80, maxFrac: 0.6, default: 180 },
};

/** A dock with nothing in it, for a preset that omits one. */
function emptyDock() {
  return { size: 0, groups: [], sizes: [], collapsed: false, hidden: true };
}

/** One tab group. */
export function group(tabs, active = 0) {
  return { tabs: [...tabs], active };
}

/**
 * The three presets, plus `custom` — the slot that fills itself when the user rearranges
 * anything (see `customise`). Sizes are in px for the side docks and the bottom dock; the
 * centre takes what is left.
 */
export const PRESETS = {
  minimal: {
    key: 'minimal',
    name: 'Minimal',
    left: { size: RAIL, collapsed: true, groups: [group(['tree'])] },
    center: { size: 0, groups: [group(['editor1'])] },
    right: { size: RAIL, collapsed: true, groups: [group(['goal'])] },
    /* A LINE, NOT A PANEL. The strip under the docks already carries the build state and the
       counts, so Minimal's bottom dock is railed to a spine that Ctrl+J opens — that is what
       "only a one-line status strip" can honestly mean when a status strip also exists. */
    bottom: { size: RAIL, collapsed: true, groups: [group(['diag', 'build'])] },
  },
  standard: {
    key: 'standard',
    name: 'Standard',
    left: { size: 240, collapsed: false, groups: [group(['tree'])] },
    center: { size: 0, groups: [group(['editor1'])] },
    right: {
      size: 380,
      collapsed: false,
      groups: [group(['goal']), group(['lean', 'render'])],
      sizes: [0.6, 0.4],
    },
    bottom: { size: 180, collapsed: false, groups: [group(['diag', 'build', 'graph'])] },
  },
  power: {
    key: 'power',
    name: 'Power',
    left: { size: 260, collapsed: false, groups: [group(['tree']), group(['outline', 'graph'])], sizes: [0.6, 0.4] },
    center: { size: 0, groups: [group(['editor1']), group(['editor2'])], sizes: [0.62, 0.38] },
    right: {
      size: 400,
      collapsed: false,
      groups: [group(['goal']), group(['steps', 'symbols']), group(['lean', 'render']), group(['stats'])],
      sizes: [0.32, 0.26, 0.26, 0.16],
    },
    bottom: { size: 220, collapsed: false, groups: [group(['diag']), group(['build'])], sizes: [0.6, 0.4] },
  },
  custom: null, // created from whatever the user is looking at
};

export const PRESET_ORDER = ['minimal', 'standard', 'power', 'custom'];

/**
 * The sizes a document came up with — the target of a splitter's double-click.
 *
 * A splitter's reset has to answer "back to what?", and the answer is the layout as it was
 * before the drag: the preset's sizes for a preset, and the sizes a custom layout was first
 * arranged at for a custom one. Kept IN the document so it survives a reload, which a value
 * held in memory would not.
 */
export function snapshot(doc) {
  const out = {};
  for (const d of DOCKS) {
    const dock = doc.docks[d];
    if (!dock) continue;
    out[d] = { size: dock.size, collapsed: !!dock.collapsed, sizes: (dock.sizes || []).slice() };
  }
  return out;
}

/** A deep copy, so a preset document is never mutated by the shell. */
export function clone(doc) {
  return JSON.parse(JSON.stringify(doc));
}

/**
 * A preset as a working document.
 *
 * THROUGH `normalize`, because a preset states what it SHOWS and not every field: a dock a
 * preset omits has no `hidden` flag of its own, and a default of `true` (the empty dock's)
 * would quietly rail every dock the preset does describe. Normalising makes `hidden` mean
 * what it says — a dock with no groups is hidden, a dock with groups is not.
 */
export function fromPreset(key) {
  const p = PRESETS[key];
  if (!p) return fromPreset('standard');
  const raw = { key: p.key, name: p.name, docks: {} };
  for (const d of DOCKS) {
    // ONLY THE PRESET'S OWN FIELDS. An empty dock carries `hidden: true` — true of a dock
    // with no groups, and a lie about one that has them; normalise decides that below.
    raw.docks[d] = p[d] ? clone(p[d]) : emptyDock();
  }
  return normalize(raw);
}

/**
 * The document a saved/edited dock set becomes: defaults filled in, ids that name a real
 * panel dropped when they do not, and every group carrying at least one tab (a group with
 * none is a divider that eats space).
 *
 * Normalising on the way IN is what makes a stored document safe to trust: a preset from an
 * older build may name a panel this build does not ship, and a tab that cannot mount is a
 * blank rectangle with a title on it.
 */
export function normalize(doc) {
  if (!doc || typeof doc !== 'object') return fromPreset('standard');
  const out = { key: typeof doc.key === 'string' ? doc.key : 'custom', name: doc.name || 'Custom', docks: {} };
  for (const d of DOCKS) {
    const src = doc.docks && doc.docks[d];
    if (!src) { out.docks[d] = emptyDock(); continue; }
    const groups = [];
    for (const g of (src.groups || [])) {
      const tabs = (g.tabs || []).filter((id) => PANELS[id]);
      if (!tabs.length) continue;
      groups.push({ tabs, active: Math.min(Math.max(0, g.active | 0), tabs.length - 1) });
    }
    const size = Number.isFinite(src.size) ? src.size : LIMITS[d] ? LIMITS[d].default : 0;
    /* A DOCK WITH GROUPS IS SHOWN unless the document says otherwise, and a dock with none
       is hidden whatever it says: `hidden` is a decision about content, not a flag to trust
       past it. (A preset states what it shows; the `hidden` a saved document carries is the
       user closing the dock by hand.) */
    const hidden = src.hidden !== undefined ? !!src.hidden : groups.length === 0;
    out.docks[d] = {
      size,
      groups,
      sizes: Array.isArray(src.sizes) && src.sizes.length === groups.length ? src.sizes : evenSizes(groups.length),
      collapsed: !!src.collapsed,
      hidden: groups.length === 0 ? true : hidden,
    };
  }
  out.defaults = (doc.defaults && typeof doc.defaults === 'object') ? clone(doc.defaults) : snapshot(out);
  // The centre is the one dock that is never hidden or railed: it is where writing happens.
  const centre = out.docks.center;
  if (!centre.groups.length) centre.groups = [group(['editor1'])];
  centre.hidden = false;
  centre.collapsed = false;
  return out;
}

export function evenSizes(n) {
  if (n <= 0) return [];
  return Array.from({ length: n }, () => 1 / n);
}

/** Which panel is active in a dock's first group — what `Ctrl+1..9` and the status strip read. */
export function activePanel(doc, dock) {
  const d = doc.docks[dock];
  if (!d || !d.groups.length) return null;
  return d.groups[0].tabs[d.groups[0].active] || null;
}

/** Every panel the document shows, in paint order. */
export function visiblePanels(doc) {
  const out = [];
  for (const d of DOCKS) {
    const dock = doc.docks[d];
    if (!dock || dock.hidden) continue;
    for (const g of dock.groups) {
      const id = g.tabs[g.active];
      if (id && !out.includes(id)) out.push(id);
    }
  }
  return out;
}

/** The panes map the instance manager wants: `{ [panelId]: { kind, params } }`. */
export function paneMap(doc) {
  const panes = {};
  for (const d of DOCKS) {
    const dock = doc.docks[d];
    if (!dock || dock.hidden) continue;
    for (const g of dock.groups) {
      for (const id of g.tabs) {
        const def = PANELS[id];
        if (!def) continue;
        panes[id] = { kind: def.kind, params: {} };
      }
    }
  }
  return panes;
}

/** Clamp a dock size against its limits and the viewport. */
export function clampSize(dock, px, viewport) {
  const lim = LIMITS[dock];
  if (!lim) return Math.max(0, px);
  const max = Math.max(lim.min, Math.round((viewport.width || 1400) * lim.maxFrac));
  return Math.min(max, Math.max(lim.min, Math.round(px)));
}

/** Whether a dock is drawn as a rail (collapsed to a spine). */
export function isRailed(doc, dock) {
  const d = doc.docks[dock];
  return !!d && !d.hidden && !!d.collapsed;
}