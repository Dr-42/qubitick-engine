/*
 * host/canvas/core.js — the canvas's state and helpers: the documents, the corpus, the server, the console.
 *
 * This is the first of the canvas's three attaches: `core` (state and helpers), `wiring` (the
 * object graph: registry, tiles, graph, themes, clusters) and `runtime` (what runs: view, input,
 * chrome, storage). Everything here is either DATA or a small service the other two read, which
 * is why it goes first — nothing in it depends on a tile existing.
 *
 * Four things that are rules rather than plumbing:
 *
 *   A DOCUMENT'S "DIRTY" IS THE WORKSPACE'S DIRTY. `js/app/docs.js` owns documents, views and
 *   the editor's ports; what it cannot know is that an edit means the workspace must be saved,
 *   so that flag is wired here, in one place, and nothing else sets it for an edit.
 *
 *   THE CORPUS IS LOADED AS A PROGRESS (see js/host/canvas/data.js): each request logs and
 *   carries on, so a corpus with no symbol table is still a corpus you can read.
 *
 *   SAFE MODE IS A CLASS ON THE DOCUMENT, set before anything else runs: everything downstream
 *   reads it and restores/saves nothing.
 *
 *   THE CONSOLE SPLITTER IS ONE INSTANCE, created on first use — the panes ask it for their
 *   column geometry, and a second instance would have a second copy of every fraction.
 */

import { createDocs } from '../../app/docs.js';
import { createData } from './data.js';
import { createNet } from './net-app.js';
import { createConsole } from '../../world/console.js';
import { createTileState } from './tile-state.js';
import { SAFE, UI, EDITORS, DATA, REG, PORT_SRC, WIRES, DYN_WIRES, LS_KEY, loadRegistry as stateLoadRegistry } from './state.js';
/* saving the open document: the capability the interface did not have (js/host/canvas/doc-save.js) */
import { createDocSave } from './doc-save.js';
import { saveModeOf } from '../../app/prefs.js';

/**
 * @param {object} app  the shared app object; this function fills the core names on it
 * @param {object} env  { $, viewport, world, viewportSize(), requestFrame() }
 */
export function attachCore(app, env) {
  const $ = env.$;
  if (SAFE) document.documentElement.dataset.safe = '1';

  /* ---------------- the state the rest of the app reads and writes ---------------- */
  let dirty = true, lodOn = true;
  let lodCounts = { live: 0, frozen: 0, card: 0, culled: 0 };
  let drawStats = { drawn: 0, built: 0, total: 0 };

  Object.assign(app, {
    // state (identity-shared)
    cam: app.cam, panes: app.panes, clusters: app.clusters, UI, DATA, REG, PORT_SRC, WIRES, DYN_WIRES,
    EDITORS, LS_KEY, SAFE, world: env.world, viewport: env.viewport,
    // flags as functions: a module may read or set one, and never hold a stale copy
    dirty: () => dirty,
    clean: () => { dirty = false; },
    markDirty: () => { dirty = true; },
    lodOn: () => lodOn,
    setLod: (v) => { lodOn = !!v; },
    lodCounts: () => lodCounts,
    drawStats: () => drawStats,
    setStats: (st) => { lodCounts = st.counts; drawStats = { drawn: st.drawn, built: st.built, total: st.total }; },
    viewport: () => env.viewportSize(),
    el: (sel) => $(sel),
  });

  /* ---------------- documents: the model is js/app/docs.js ---------------- */
  const fileTextCache = (path) => DATA.fileText[path];
  fileTextCache.set = (path, text) => { DATA.fileText[path] = text; };
  const DOCS_API = createDocs({
    readFile: async (path) => {
      const r = await app.fetch(app.url('file.get') + '?path=' + encodeURIComponent(path));
      const j = await r.json();
      return (j.file && j.file.content) || j.content || '';
    },
    markDirty: () => { dirty = true; },
    paint: (q) => { if (q && q.chrome) q.chrome(); },
    cachedText: fileTextCache,
  });
  const { EDITOR, TREE, DOCS } = DOCS_API;

  /* ---------------- the corpus and the two pieces of feedback ---------------- */
  const DATA_APP = createData({
    el: (sel) => $(sel),
    url: (name) => app.url(name),
    routesReady: () => app.ROUTES_READY,
    data: DATA,
    editor: () => EDITOR,
    panes: app.panes,
    tiles: () => app.panes,
    render: (p) => app.render(p),
    setActive: (path) => DOCS_API.setActive(path),
    readFile: async (path) => {
      if (!DATA.fileText[path]) {
        try {
          const r = await app.fetch(app.url('file.get') + '?path=' + encodeURIComponent(path));
          const j = await r.json();
          DATA.fileText[path] = (j.file && j.file.content) || j.content || '';
        } catch (e) { DATA.fileText[path] = ''; }
      }
      return DATA.fileText[path];
    },
    warn: (m) => console.warn('[' + m.split(' ')[0] + ']', m),
  });

  /* ---------------- saving the open document ---------------- */
  /* AFTER `DOCS_API` and after `NET`: it needs the document registry to read the text and to clear the
     dirty flag, and the routes for the save itself, the per-file check and the build. `mode` is read per
     save (a setting a person can change while a tile is open), never captured at construction. */
  const DOC_SAVE = createDocSave({
    docs: DOCS_API,
    url: (name) => NET.url(name),
    fetch: (u, init) => fetch(u, init),
    mode: () => saveModeOf(UI.view && UI.view.saveMode),
    toast: (m) => { if (app.toast) app.toast(m); },
    runCheck: () => { if (app.runCheck) app.runCheck(); },
    rows: (content) => NET.rows(content),
    /* EVERY tile showing that path, not just the active one: two views of one document are two tiles, and
       both show the same unsaved mark. */
    repaint: (path) => {
      for (const q of app.panes || []) {
        if (q.doc && q.doc.path === path && q.chrome) { try { q.chrome(); } catch (e) { /* a gone tile */ } }
      }
    },
  });

  /* ---------------- the server: routes, projects, the corpus check ---------------- */
  const NET = createNet({
    fetch: (u, init) => fetch(u, init),
    toast: (m) => app.toast(m),
    paintChip: () => app.paintChip(),
    paintDiag: () => { for (const p of app.panes) if (p.kind === 'diag' && p.chrome) p.chrome(); },
    reload: () => location.reload(),
    /* THIS ARGUMENT IS NOT THE APP, it is what the net layer needs from it (see `createNet`'s doc
       comment) — so the save mode has to be handed over by name, exactly like the toast. Read per write:
       a setting a person can change while a tile is open is not a value to capture at construction. */
    saveMode: () => saveModeOf(UI.view && UI.view.saveMode),
  });

  /* ---------------- the console splitter, one instance ---------------- */
  let CONSOLE = null;
  const consoleHost = () => CONSOLE || (CONSOLE = createConsole({
    cam: app.cam,
    splits: UI.splits,
    render: (q) => app.render(q),
    save: () => app.scheduleSave(),
    toast: (m) => app.toast(m),
    gripAt: () => app.gripAt(),
    gripBand: (wr) => app.gripBand(wr),
    minSize: env.minSize,
  }));

  /* ---------------- what the app now knows ---------------- */
  Object.assign(app, {
    // documents
    EDITOR, TREE, DOCS,
    docOf: (path, text) => DOCS_API.docOf(path, text),
    docEdited: (d, text, origin, changes) => DOCS_API.docEdited(d, text, origin, changes),
    setActive: (path) => DOCS_API.setActive(path),
    loadText: (path) => DOCS_API.loadText(path),
    showDoc: (q, d, opts) => DOCS_API.showDoc(q, d, opts),
    watchActive: (q) => DOCS_API.watchActive(q),
    docBound: (q, fn) => DOCS_API.docBound(q, fn),
    docText: (q) => DOCS_API.docText(q),
    docPath: (q) => DOCS_API.docPath(q),
    activeDoc: () => DOCS_API.active,
    // corpus and feedback
    DATA_APP, toast: (m) => DATA_APP.toast(m), setLoad: (p) => DATA_APP.setLoad(p),
    fetchJSON: (u) => DATA_APP.fetchJSON(u), openFile: (path) => DATA_APP.openFile(path),
    corpusFrom: (inst, p) => DATA_APP.corpusFrom(inst, p),
    /* SAVE. `saveDoc(p)` writes p's own document (a pinned tile saves its own file); `saveActiveDoc()`
       is the keyboard's version, on the document the editor is following. Both go through one routine, so
       the mode, the symbol report and the dirty flag cannot disagree between a button and a shortcut. */
    saveDoc: (p) => DOC_SAVE.save(p),
    /* THE SAVE MODE, persisted with the workspace like any other view preference (js/app/prefs.js owns
       the vocabulary). Read per save by `DOC_SAVE`, so changing it takes effect on the next Ctrl+S. */
    setSaveMode: (id) => {
      const next = saveModeOf(id);
      UI.view.saveMode = next;
      if (app.scheduleSave) app.scheduleSave();
      return next;
    },
    saveActiveDoc: () => DOC_SAVE.save({ doc: DOCS_API.active }),
    // server
    NET, ROUTES: NET.ROUTES, url: (name) => NET.url(name), ROUTES_READY: NET.ROUTES_READY,
    // the endpoint NODE for a route name, for a pane migrated from the engine host (js/panes/slate.js)
    endpoint: (name) => NET.endpoint(name),
    projects: () => NET.projects(), projectKey: () => NET.projectKey(),
    loadProjects: () => NET.loadProjects(), useProject: (id) => NET.useProject(id),
    addProject: (path) => NET.addProject(path),
    forgetProject: (path) => NET.forgetProject(path),
    subscribeProjects: (fn) => NET.subscribeProjects(fn),
    CHECKS: NET.CHECKS, DIAG: NET.DIAG, DIAG_PATH: NET.DIAG_PATH,
    diagCounts: () => NET.diagCounts(), diagRows: () => NET.diagRows(),
    runCheck: () => NET.runCheck(),
    // the registry of words
    loadRegistry: () => stateLoadRegistry({
      fetch: (u) => fetch(u),
      onConsoleKinds: (kinds) => app.setConsoleKinds(kinds),
      onFilled: () => app.populateKinds($('#s-kind') || undefined),
    }),
    // the console splitter
    consoleHost,
    paneBodyW: (p) => consoleHost().bodyW(p),
    splitAvail: (p) => consoleHost().avail(p),
    splitRange: (p) => consoleHost().range(p),
    refitSplit: (p) => consoleHost().refit(p),
    paneMin: (p) => consoleHost().paneMin(p),
    renderControlColumns: (p, sections) => consoleHost().renderColumns(p, sections),
  });
  return app;}
