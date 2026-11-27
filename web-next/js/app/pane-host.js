/*
 * pane-host.js — put a pane module into a tile.
 *
 * The canvas app has tiles, and a pane module has `create(ctx) -> {mount, unmount}`. This
 * is the two lines of glue between them, plus the part worth having: one ctx for every
 * pane, and an unmount that actually runs when a tile is re-rendered.
 *
 * The ctx is what a pane is allowed to know about the app (see web-next/js/README.md):
 *
 *   data    the corpus the page loaded (files, graph, symbols, stats)
 *   editor  the editor's ports (buffer, file, cursor) — valuePorts from js/core/ports.js
 *   url     route lookup from js/defaults/endpoints.js; a pane never writes a path
 *   ui      element builders and `esc` (js/app/ctl.js)
 *   wires   the live wire list, for the panes that show it
 *
 * None of it is global. A pane that reaches for `window` is the bug this file exists to
 * make unnecessary — and the reason the same module can be mounted by the shell.
 */

/**
 * @param {object} services  the app services above, as getters or values
 * @returns {{ mount(def, p), unmount(p) }}
 */
import { bindPanePorts, writeInto } from './pane-ports.js';

export function createPaneHost(services) {
  /** A ctx with the same shape for every pane, reading the app through the services. */
  const ctxFor = (p) => ({
    // `p` is the tile: a pane may read its own subject (a pinned file, a selection)
    tile: p,
    get data() { return services.data(); },
    get editor() { return services.editor(); },
    get wires() { return services.wires(); },
    url: (name) => services.url(name),
    /* THE ROUTE TABLE ARRIVES ASYNCHRONOUSLY (js/net/routes.js), and asking for a route before
       it lands is not an error to be swallowed: it THROWS. A pane that talks to the server the
       moment it mounts — the corpus tile does, it has to fetch what it serves — waits for this
       first, which is the same rule the app's own loaders follow (js/app/boot-workspace.js). */
    routesReady: () => (services.routesReady ? services.routesReady() : Promise.resolve()),
    ui: services.ui,
    /* THE ENDPOINT NODE FOR A ROUTE NAME, for a pane migrated from the engine host (js/panes/slate.js).
       Named for the capability and NOT `ws`: `ctx.ws` is what the panes read as "this is the engine
       host" (js/panes/new-node.js picks its modal shape on it), and this host is not one.
       It has to be forwarded HERE, not merely offered by the services table: this ctx is a WHITELIST,
       so a service a pane asks for and this object does not name is simply not there. */
    endpoint: (name) => (services.endpoint ? services.endpoint(name) : null),
    // the workspace's own size: how many tiles and roots exist right now
    counts: () => (services.counts ? services.counts() : null),
    /* THE CORPUS CHECK: its state and its run. `js/panes/diag.js` reads the first and calls the
       second — "check corpus" is the same build the CLI runs (`POST /api/build`) — and NEITHER was
       forwarded here, so in the canvas that tile's button did nothing at all: it asked for
       `ctx.runCheck`, which was undefined, and the tile went on saying "never checked" while the
       build it was supposed to start never started. The shell host has always provided both (its
       Build button reaches the pane's `check`/`rerun` call ports through the shared wiring flow), so
       the pane worked in one host and was inert in the other — the exact failure this table's own
       comment warns about for `addProject`. A pane's ctx is part of its contract: if it asks, it
       must be answered. */
    check: () => (services.check ? services.check() : null),
    runCheck: () => { if (services.runCheck) services.runCheck(); },
    /* MAKING A NODE, and OPENING WHAT A DIAGNOSTIC NAMES. `js/panes/new-node.js` writes a corpus
       file through whoever owns the route (js/app/node-services.js) and `js/panes/diag.js` opens a
       file by calling `ctx.openFile` — a call the canvas answered with `undefined`, so clicking a
       diagnostic row did nothing in the app that has the corpus in it. */
    createNode: (node) => (services.createNode ? services.createNode(node) : Promise.resolve(null)),
    openFile: (path) => { if (services.openFile) services.openFile(path); },
    // text already in hand for a path: the app caches what it has fetched
    fileText: (path) => (services.fileText ? services.fileText(path) : ''),
    // the wire layer belongs to the app: a pane asks it whether it is on, and says so
    wireOn: () => (services.wireOn ? services.wireOn() : false),
    wireCount: () => (services.wireCount ? services.wireCount() : 0),
    setWireMode: (on) => { if (services.setWireMode) services.setWireMode(on); },
    // the source tile: which panes exist, what a pane is built from, and how to edit it
    panes: () => (services.panes ? services.panes() : []),
    paneSource: (pane) => (services.paneSource ? services.paneSource(pane) : null),
    applyPaneSource: (pane, text) => { if (services.applyPaneSource) services.applyPaneSource(pane, text); },
    savePaneSource: (pane) => { if (services.savePaneSource) services.savePaneSource(pane); },
    loadEditor: () => (services.loadEditor ? services.loadEditor() : Promise.resolve(null)),
    /* The settings panes are a stack of sections that can become two columns. That
       arrangement (measuring the sections, placing the seam, remembering the fraction) is
       the page's, so a pane offers sections and the host arranges them. */
    controlColumns: (host, sections) => {
      // `p` is this ctx's own tile, closed over above
      if (services.controlColumns) { services.controlColumns(p, host, sections); return; }
      host.append(...sections);
    },
    // editor preferences: one set of values, every editor tile
    editorPrefs: () => (services.editorPrefs ? services.editorPrefs() : {}),
    setEditorPref: (key, value) => { if (services.setEditorPref) services.setEditorPref(key, value); },
    applyEditorPrefs: () => { if (services.applyEditorPrefs) services.applyEditorPrefs(); },
    saveWorkspace: () => { if (services.saveWorkspace) services.saveWorkspace(); },
    // the open DOCUMENT (not the arrangement), and the mode that decides what follows a save
    saveActiveDoc: () => { if (services.saveActiveDoc) services.saveActiveDoc(); },
    saveMode: () => (services.saveMode ? services.saveMode() : 'none'),
    setSaveMode: (id) => (services.setSaveMode ? services.setSaveMode(id) : 'none'),
    editorRange: () => (services.editorRange ? services.editorRange() : null),
    // the workspace's own actions: a console pane is a list of buttons over these
    addTile: (kind) => { if (services.addTile) services.addTile(kind); },
    clusterLoose: () => (services.clusterLoose ? services.clusterLoose() : false),
    addRoot: () => { if (services.addRoot) services.addRoot(); },
    resetLayout: () => { if (services.resetLayout) services.resetLayout(); },
    defaultView: () => { if (services.defaultView) services.defaultView(); },
    frameAll: () => { if (services.frameAll) services.frameAll(); },
    populateKinds: (sel) => { if (services.populateKinds) services.populateKinds(sel); },
    graph: () => (services.graph ? services.graph() : { running: false, tick: 0 }),
    graphRun: (running) => { if (services.graphRun) services.graphRun(running); },
    graphStep: () => { if (services.graphStep) services.graphStep(); },
    // one namespace per subsystem: the camera/tiles/theme/field family
    get view() { return services.view || null; },
    // the theme subsystem: boards, picks, the palette and the editor that owns it
    get themes() { return services.themes || null; },
    // the app's own ports, for the panes that declare matching in-ports (see bindPorts)
    get appPorts() { return services.ports ? services.ports() : {}; },
    /* A PANE MAY PUBLISH A NAMED SOURCE: the wire layer resolves a source by `<paneId>_<port>`
       (js/live/wire/host.js), and a pane whose out-port is not a known convention registers it
       here and gets a remover back (js/panes/switch.js and js/panes/corpus.js do). */
    registerSource: (name, port) => (services.registerSource ? services.registerSource(name, port) : () => {}),
    /* and the project hooks the app owns: which projects the engine can serve, how to make it
       serve one of them, and how a PATH becomes one of them. `addProject` was missing here while
       the projects pane had a button calling it — a forwarder is part of the ctx or the pane
       using it is a button that does nothing (js/panes/projects.js) */
    projects: () => (services.projects ? services.projects() : { list: [], active: {} }),
    useProject: (id) => { if (services.useProject) services.useProject(id); },
    loadProjects: () => (services.loadProjects ? services.loadProjects() : Promise.resolve(null)),
    addProject: (path) => (services.addProject ? services.addProject(path) : Promise.resolve(false)),
    /* the other half of `addProject`: without a forwarder here a `forget` button is the same
       button-that-does-nothing `addProject` was before this table had it (js/panes/projects.js) */
    forgetProject: (path) => (services.forgetProject ? services.forgetProject(path) : Promise.resolve(false)),
    /* follow the project document: a switch made by another pane, or by a tile's own `use` call, is
       not something a tile hears about any other way (js/panes/project.js) */
    subscribeProjects: (fn) => (services.subscribeProjects ? services.subscribeProjects(fn) : () => {}),
  });

  /**
   * Connect the app to a pane's declared in-ports BY NAME.
   *
   * A pane declares `in: { cursor:'value', buffer:'value' }` and does not care who fills
   * them. In the shell that is a wire; in the canvas app the same names exist on the
   * editor (`EDITOR.cursor`, `.buffer`, `.file`). Binding by name is what lets one module
   * run in both hosts without a line of adapter code per pane.
   *
   * A cursor is the one composition: the app's cursor says where in the document, the app's
   * file says which document, and a pane wants both — so `cursor` is delivered as
   * `{...cursor, path: file}`.
   */
  function bindPorts(inst, p) {
    const inPorts = (inst && (inst.in || (inst.ports && inst.ports.in))) || {};
    const app = ctxFor(p).appPorts;

    /* WHICH document and WHICH text. A tile may be pinned to its own file (that is what
       docBound means) or follow the workspace's active one; a pane wants whichever its
       tile is showing, so the host answers with the tile's document when it has one. */
    const fileOf = () => (p.doc && p.doc.path) || (app.file && app.file.get && app.file.get()) || '';
    const textOf = () => (p.doc && p.doc.text != null) ? p.doc.text
      : (app.buffer && app.buffer.get ? app.buffer.get() : '') || '';

    /* EVERY PORT A PANE READS DESCRIBES ONE DOCUMENT — THE TILE'S. `fileOf`/`textOf` above already
       answer "which document is this tile showing", and the `cursor` composition uses `fileOf()`,
       but `buffer` and `file` were handed over as the APP's own ports: a pane in a pinned tile was
       told its own path and the workspace's active text. A pane that reads both can tell — it asks
       the engine about a buffer at a line (js/panes/goal.js) — and the answer was about two
       documents. The source still WAKES on the app's edge (that is how a follower tile hears about
       an open) and reads through the tile when it wakes. After `...app`, or the app's own
       `buffer`/`file` land on top of these. */
    /* A CONNECTED WIRE WINS. Every app port below is delivered by name whether or not anything is
       wired, which made the wires decoration: `editor.buffer → render` and the four others in
       js/host/canvas/state.js targeted inputs the app already filled, so REMOVING one changed
       nothing at all (measured: eight of them delivered nothing when removed, and the seeded wires
       lead to exactly those eight).
       The wire layer hands a tile every wired input as `ins` and keeps the last set on `p._ins`,
       with a wire that is GONE written back as `undefined` (js/live/wire/host.js::applyInputs puts
       a removed wire's port back to "unset"). So "is this port wired" is answerable here, and the
       app's own value is the fallback rather than the truth:

         · the app sources READ THROUGH the wire, so an app change can never overwrite what a wire
           is carrying (that race is what made the old behaviour look like it worked);
         · a forwarder pushes wired values into the pane's in-ports, and hands the app's own value
           back when a wire is removed — otherwise the pane keeps a dead wire's last value.

       The editor's `syntaxWired` ("the wire is authoritative, not a hint", js/panes/app-live.js) is
       this rule for one port; this is the rule for all of them. */
    const wired = (name) => !!(p._ins && p._ins[name] !== undefined);
    const appValue = (name) => {
      if (name === 'buffer') return textOf();
      if (name === 'file') return fileOf();
      const port = app[name];
      return port && port.get ? port.get() : undefined;
    };
    const effective = (name) => (wired(name) ? p._ins[name] : appValue(name));
    /* `on` hands the value ALONG, not just the news: the loop this feeds is
       js/app/pane-ports.js's, which delivers what the source calls it with (`fn(effective(name))`),
       and a callback that called back with no argument announced a change and then delivered
       `undefined` — the goal tile drew `L1 C1` after the caret had moved to line 42, because the
       value it was handed was nothing at all.
     *
       AND WHILE A WIRE FEEDS THIS PORT, THE WIRES ARE RECOMPUTED FIRST. `effective` reads the wire
       layer's last applied value, and the layer applies its inputs on the HUD's cadence
       (js/world/chrome.js, every HUD_MS) and on structural changes (a wire added, removed, or
       selected) — so an app edge delivered the value from BEFORE the move and the pane caught up a
       HUD tick later. Measured: the goal tile still showed the old line 80 ms after the caret moved,
       which is a race that passes on one project and fails on another. Recomputing here is what
       makes an app-driven source as immediate as it was before
       a wire could outrank it, and it is paid only by the ports that are actually wired. */
    const through = (name, appPort) => ({
      get: () => effective(name),
      on: (fn) => {
        const off = appPort && typeof appPort.on === 'function'
          ? appPort.on(() => {
            if (wired(name) && typeof services.applyInputs === 'function') services.applyInputs();
            fn(effective(name));
          })
          : null;
        return () => { if (typeof off === 'function') { try { off(); } catch (e) {} } };
      },
    });

    /* THE CONNECTION IS js/app/pane-ports.js'S (the same loop the engine's node ports go
       through). What is this host's is the SOURCE TABLE — the app's own ports, by name — and one
       composition: a `cursor` arrives as "where in the document", and the document is the tile's. */
    const sources = {
      in: {
        ...app,
        ...Object.fromEntries(Object.keys(app).map((name) => [name, through(name, app[name])])),
      },
    };
    /* and the wire's value reaches the pane even when nothing else fires: the app's sources are
       edge-driven, so a wire that is connected while everything sits still would arrive nowhere */
    p.onInputs = (ins) => {
      for (const [name, value] of Object.entries(p._ins || ins || {})) {
        const port = inPorts[name];
        if (!port) continue;
        writeInto(port, value === undefined ? appValue(name) : value);
      }
    };
    const unbind = bindPanePorts(inst, sources, {
      map: (name, v) => (name === 'cursor' ? { ...(v || {}), path: fileOf() } : v),
    });

    const offs = [unbind];

    /* The tile's own document can change under a pane (a pinned file is re-opened, the tile
       follows another selection) and that is a new subject: re-deliver the text and path. */
    if (typeof services.bindTileDoc === 'function') {
      offs.push(services.bindTileDoc(p, () => {
        writeInto(inPorts.buffer, textOf());
        writeInto(inPorts.file, fileOf());
        writeInto(inPorts.cursor, { ...(app.cursor && app.cursor.get ? app.cursor.get() : {}), path: fileOf() });
      }));
    }
    return () => { for (const off of offs) { try { off(); } catch (e) {} } };
  }

  function unmount(p) {
    const off = p && p._paneUnmount;
    p._paneUnmount = null;
    if (typeof off === 'function') {
      try { off(); } catch (e) { console.warn('[pane] unmount', e); }
    }
  }

  return {
    unmount,
    /**
     * Mount `def` into a tile's body. The previous occupant is unmounted first: a tile is
     * re-rendered on every change of subject, and a pane that leaks a listener per render
     * is a pane that gets slower the longer you use it.
     */
    mount(def, p) {
      if (!p || !p.body) return null;       // a mount with no tile is a caller bug, not a crash
      unmount(p);
      p.body.textContent = '';
      const inst = def.create(ctxFor(p));
      inst.mount(p.body);
      const unbind = bindPorts(inst, p);
      // a pane that follows app state outside its ports can be told to redraw
      p.refreshPane = () => { if (typeof inst.refresh === 'function') inst.refresh(); };
      p._paneUnmount = () => { unbind(); if (typeof inst.unmount === 'function') inst.unmount(); };
      if (def.title) p.kindTitle = def.title;
      return inst;
    },
  };
}