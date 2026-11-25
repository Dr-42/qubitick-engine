/*
 * host/canvas/persistence.js — the workspace document: what it is made of, and who may restore it.
 *
 * Extracted from the inline script in web-next/index.html. The RULES already live in modules —
 * the shape is `js/app/layout-doc.js`, which document wins is `js/app/workspace-load.js`, the
 * debounce and the mirror-first write are `js/app/workspace-save.js`, and the restore order is
 * `js/app/boot-workspace.js`. What was in the page was the COMPOSITION of those, and the part
 * only this app can answer: which tiles and clusters exist, what the camera is, which theme is
 * on, and which files have unsaved edits.
 *
 * Three things here are worth knowing:
 *
 *   A SOLO TILE IS SAVED WHERE IT CAME FROM. `wrOf` answers with the rect the workspace knows,
 *   not the viewport rect the tile is currently covering — a save taken while a tile is
 *   fullscreen must not write the screen size into the document.
 *
 *   A RESTORE IS NOT A PREFERENCE CHANGE. `applying` marks the window in which the view prefs
 *   are being applied from a document, so the wire switch's callback does not save the document
 *   it is being restored FROM.
 *
 *   SAFE MODE SAVES NOTHING AND CLEARS NOTHING. The saver is told, so `?safe=1` can look at a
 *   workspace that is stuck without overwriting the copy that can still be repaired. The
 *   mirror itself is the app's (`app.mirror`): this module never touches browser storage.
 */

import { serializeLayout, collectWorkspace } from '../../app/layout-doc.js';
import { createSaver } from '../../app/workspace-save.js';
import { bootWorkspace } from '../../app/boot-workspace.js';
import { treeToJSON, treeFromJSON } from '../../app/tile-tree.js';
import { isLayoutDocument, layoutToCanvas } from '../../app/layout-to-canvas.js';

/**
 * @param {object} app
 *   panes, clusters, cam, UI, DOCS, theField(), THEMES, customThemes(), activeDoc(),
 *   url, fetch, and the browser mirror as `mirror` { get(), set(key,v), remove(key) },
 *   restore(layout), applyEditorPrefs(), applyViewPrefs(), render(p), toast(m),
 *   ensureConsole(), loadText(path), setActive(path), reload(),
 *   projectKey(), safe, dynWires(), pendingWires()/setPendingWires, wireLayer()
 */
export function createPersistence(app) {
  let LAYOUTS = {};                       // every project's workspace, keyed by project id
  let applying = false;                   // a restore is not a preference change
  let PENDING_WIRES = null;               // wires restored before the wire layer is armed

  const isApplying = () => applying;

  /** The workspace's own state, as the document writer wants it. */
  function snapshot() {
    const edited = document.documentElement.dataset.themeEdited === '1';
    const field = app.theField();
    const active = app.activeDoc();
    return {
      clusters: app.clusters, panes: app.panes,
      wrOf: (p) => app.wrOf(p), treeToJSON, cam: app.cam,
      themeId: app.THEMES.currentId(), themeName: app.THEMES.currentName(),
      theme: edited ? app.THEMES.readVars() : undefined, themeEdited: edited,
      field: field ? field.space : (app.UI.view.field || null),
      banner: field ? field.banner() : (app.UI.view.banner || null),
      customThemes: app.THEMES.custom.slice(),
      active: active ? active.path : null,
      docs: [...app.DOCS.values()].filter((d) => d.dirty).map((d) => ({ path: d.path, text: d.text })),
    };
  }

  /** The document that gets written. */
  const documentOf = () => collectWorkspace(snapshot(), {
    wires: app.dynWires(), editor: app.UI.editor, view: app.UI.view, splits: app.UI.splits,
  });

  /** The layout only (the shape a probe or a debug hook asks for). */
  const layoutOf = () => serializeLayout(snapshot());

  /** Apply a saved workspace document — or an ENGINE LAYOUT DOCUMENT, opened here. */
  function apply(doc) {
    if (!doc) return false;
    /* A LAYOUT DOCUMENT FROM THE ENGINE (`mode` + roots whose trees hold the panes, js/layout/
       schema.js) is not this app's workspace document: it is converted to one
       (js/app/layout-to-canvas.js) and then restored like anything else. So a `mode: 'canvas'`
       document — the same workspace the canvas writes, in the engine's shape — can be opened in
       the canvas, and the two documents are one schema in both directions. */
    const asLayout = isLayoutDocument(doc) ? doc
      : (isLayoutDocument(doc.layout) ? doc.layout : null);
    if (asLayout) doc = { ...doc, layout: layoutToCanvas(asLayout) };
    if (doc.editor) Object.assign(app.UI.editor, doc.editor);
    if (doc.view) Object.assign(app.UI.view, doc.view);
    // a split lives in a pane's markup, so restoring one means redrawing the pane that shows it
    if (doc.splits) {
      /* MERGED IN PLACE, never replaced. `UI.splits` is a table other things hold: the console
         splitter is given it once and keeps the reference, so a restore that swapped the object
         left the splitter writing into a table nobody reads — the tile drew two columns while
         `UI.splits[kind]` said `null`, and the seam that was just dragged was not in the saved
         document either. `editor` and `view` two lines up are already merged for the same
         reason. */
      Object.assign(app.UI.splits, doc.splits);
      for (const q of app.panes) if (typeof app.UI.splits[q.kind] === 'number') app.render(q);
    }
    applying = true;
    app.applyEditorPrefs();
    app.applyViewPrefs();
    applying = false;
    if (doc.layout && (doc.layout.roots || doc.layout.free)) app.restore(doc.layout);
    if (Array.isArray(doc.wires)) {
      const wires = doc.wires.filter((w) => w && w.from && w.to);
      if (app.wireLayer() && app.wireLayer().rebuildSchedule) {
        const dyn = app.dynWires();
        dyn.length = 0;
        for (const w of wires) dyn.push({ id: 'd' + dyn.length, ...w });
        app.wireLayer().rebuildSchedule();
      } else {
        // the wire layer arms asynchronously; it picks these up when it does
        PENDING_WIRES = wires;
      }
    }
    return true;
  }

  const SAVER = createSaver({
    document: () => documentOf(),
    projectKey: () => app.projectKey(),
    safe: () => app.safe,
    layouts: () => LAYOUTS,
    mirror: (key, doc) => {
      if (doc) app.mirror.set(app.mirrorKey, JSON.stringify(Object.assign({ project: key }, doc)));
      else app.mirror.remove(app.mirrorKey);
    },
    fetch: (u, init) => app.fetch(u, init),
    url: (name) => app.url(name),
    toast: (m) => app.toast(m),
    timers: { setTimeout: (fn, ms) => setTimeout(fn, ms), clearTimeout: (t) => clearTimeout(t) },
  });

  const scheduleSave = () => SAVER.schedule();
  const save = (opts) => SAVER.save(opts);
  /** Clear the mirror and the server document, then reload into an empty page. */
  const reset = () => SAVER.reset({ reload: () => app.reload() });

  /** The two-pass restore: routes, projects, safe mode, mirror, then the server. */
  function loadSettings() {
    window.__diag = window.__diag || {};
    return bootWorkspace({
      routesReady: app.routesReady ? app.routesReady() : Promise.resolve(),
      refreshProjects: () => app.refreshProjects(),
      projectKey: () => app.projectKey(),
      safe: app.safe,
      mirror: () => app.mirror.get(),
      apply: (doc) => apply(doc),
      setLayouts: (table) => { LAYOUTS = table; },
      url: (name) => app.url(name),
      fetch: (u, init) => app.fetch(u, init),
      activeDoc: () => app.activeDoc(),
      loadText: (path) => app.loadText(path),
      setActive: (path) => app.setActive(path),
      ensureConsole: () => app.ensureConsole(),
      ensureCorpus: () => app.ensureCorpus(),
      paneCount: () => app.panes.length,
      diag: (o) => Object.assign(window.__diag, o),
    });
  }

  return {
    LAYOUTS: () => LAYOUTS,
    snapshot, documentOf, layoutOf, apply,
    scheduleSave, save, reset, loadSettings,
    pendingWires: () => PENDING_WIRES,
    clearPendingWires: () => { PENDING_WIRES = null; },
    applying: isApplying,
    treeToJSON, treeFromJSON,
  };}
