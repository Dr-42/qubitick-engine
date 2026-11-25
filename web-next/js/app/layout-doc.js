/*
 * layout-doc.js — the workspace document: what gets saved, and in what shape.
 *
 * Extracted from the inline script in web-next/index.html. A saved workspace is a plain
 * object, and the two functions here are what turn the running app into that object. They
 * are PURE: the page hands over a snapshot (its tiles, its clusters, its geometry helpers,
 * the theme it has on) and gets the document back. That is what makes the shape testable
 * without a browser — and the shape is the thing that breaks quietly, because a saved
 * document that loses a field only fails on the next boot.
 *
 * Two rules the shape keeps, and both of them are compatibility promises:
 *
 *   `id` keeps two tiles of the same kind apart, and `home` is set when a pane is created,
 *   so a restored cluster is not mistaken for a pile of free tiles.
 *
 *   the theme travels as an IDENTITY (which module) plus, only when the palette was
 *   hand-edited, the values. So a theme module updated later still applies, and an edited
 *   palette still comes back as the edit it was.
 *
 * A document carries only the files that were EDITED: the corpus lives on the server, and
 * a workspace that saves 329 unchanged files is a workspace that fights the corpus.
 */

/** A pane as it is written down: geometry, what it is, and what it is showing. */
function paneDoc(p, wrOf) {
  const r = wrOf(p);
  return {
    id: p.id, kind: p.kind, x: r.x, y: r.y, w: r.w, h: r.h,
    pin: !!p.pin, subject: p.subject || null,
    selLink: (p.selLink !== false), selLocal: p.selLocal || null,
    syntax: (p.syntax !== false), subjectPane: p.subjectPane || null,
    // a project tile's subject is a PATH on the server (js/panes/project.js): the workspace
    // remembers which corpus directory it names, or a restored tile names a different one
    path: p.path || null,
  };
}

/**
 * The layout half of a workspace document.
 * @param {object} s snapshot: { clusters, panes, wrOf, treeToJSON, cam, ... }
 */
export function serializeLayout(s) {
  const { clusters = [], panes = [], wrOf = (p) => p.wr, treeToJSON = (t) => t, cam } = s;

  const roots = clusters.map((c) => ({
    id: c.id, x: c.x, y: c.y, w: c.w, h: c.h,
    kinds: (c.panes ? Object.keys(c.panes) : []).map((k) => paneDoc(c.panes[k], wrOf)),
    tree: treeToJSON(c.tree),
  }));

  // a free tile is one whose home is not a cluster: `home` is set when the pane is created
  const free = panes.filter((p) => !String(p.home || '').startsWith('cluster'))
    .map((p) => paneDoc(p, wrOf));

  return {
    canvas: { panX: cam.x, panY: cam.y, zoom: cam.z },
    themeId: s.themeId, themeName: s.themeName,
    theme: s.themeEdited ? s.theme : undefined,
    field: s.field, banner: s.banner,
    customThemes: (s.customThemes || []).slice(),
    roots, free,
    active: s.active || null,
    // only documents with unsaved edits travel: the corpus lives on the server
    docs: (s.docs || []).map((d) => ({ path: d.path, text: d.text })),
  };
}

/**
 * The whole workspace document: the layout plus the wire list and the three preference
 * sets. `savedAt` is stamped here so a caller cannot forget it.
 */
export function collectWorkspace(s, prefs) {
  const { wires = [], editor = {}, view = {}, splits = {} } = prefs || {};
  return {
    savedAt: (s.now ? s.now() : Date.now()),
    layout: serializeLayout(s),
    wires: wires.map((w) => ({ from: w.from, to: w.to, kind: w.kind, type: w.type })),
    editor: { ...editor },
    view: { ...view },
    splits: { ...splits },
  };
}