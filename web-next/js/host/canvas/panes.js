/*
 * host/canvas/panes.js — the pane registry, and every kind the canvas ships with.
 *
 * Extracted from the inline script in web-next/index.html. The engine is generic — titles, the
 * add-tile menu, the wire layer's port table and "edit source" all key off this registry — and
 * what the page had was the machinery plus the registrations. They belong together, because the
 * registrations ARE the list of what a tile can be.
 *
 * Three rules:
 *
 *   A KIND IS REGISTERED WITH ITS PORTS. Inputs and outputs are what the wire layer and the pane
 *   host read; a registration without them is a tile nothing can be wired to, and the symptom is
 *   a wire that refuses to connect for no reason anybody can see.
 *
 *   A USER PANE IS A MANIFEST, AND A BAD ONE IS A TILE THAT SAYS SO. `js/panes/user-panes.js`
 *   compiles a manifest's code once, inside a try, so a typo becomes a tile that explains itself
 *   rather than an exception during a re-render. The registry keeps the raw manifests so "save"
 *   can write them back.
 *
 *   THE KIND TABLES ARE THE CONTRACT. `code` and `program` panes are editable, `builtin` panes
 *   redirect to the kind they delegate to, and an app kind is read-only — which is
 *   `js/panes/source-of.js`'s rule, asked here with this registry in hand.
 */

import { mountOf as userMountOf, behaviourMount as behaviourMountOf, loadManifests, portsOf } from '../../panes/user-panes.js';
import { sourceOf, applySource, saveSource } from '../../panes/source-of.js';

/**
 * @param {object} app
 *   editor(), toast, graph(), fetch(u,init), url(name), render(p), markDirty(), save(),
 *   nodeRuns(), behaviourCtx(), paneFocus(p), makeTile(id,kind,wr,home,state), nextTileId(kind),
 *   setHotPane(el), populateKinds(), mounts(kind) → a mount function or null
 * @param {object} deps { DATA, panes, cam, viewport(), loadKatex, openFile }
 */
export function createRegistry(app, deps) {
  const { DATA, panes } = deps;

  const PANES = {};                 // id -> { id, title, inputs:[], outputs:[], render, mount }
  /** The context user code-block pane handlers get. Getters, so it follows the app. */
  const CTX = {
    get EDITOR() { return app.editor(); },
    get DATA() { return DATA; },
    get katex() { return window.katex; },
    get loadKatex() { return deps.loadKatex; },
    get openFile() { return deps.openFile; },
    get toast() { return app.toast; },
    get clock() { return app.graph().run.tick; },
    get graphRun() { return app.graph().run; },
  };

  function pane(id, title, mount, opts = {}) {
    PANES[id] = Object.assign({ id, title, inputs: [], outputs: [], render: null, mount }, opts);
  }

  /* a manifest's render descriptor becomes a mount fn in js/panes/user-panes.js: user code is
     compiled once, inside a try, so a typo is a tile that says what is wrong rather than an
     exception during a re-render */
  const userMount = (m) => userMountOf(m, CTX, PANES);

  /** Register the kinds this client ships with. `mounts(kind)` decides how each is built. */
  function registerBuiltins(mounts = (kind) => app.mounts(kind)) {
    pane('tree', 'Corpus tree', mounts('tree'), { outputs: [['selection', 'NodeId']] });
    pane('graph', 'Graph', mounts('graph'), { inputs: [['highlight', 'NodeId']], outputs: [['selection', 'NodeId']] });
    /* `inputs` here is what the DRAWER offers to a wire, so a port on this list has to be a port
       the tile can receive. `highlight` was on both this row and the graph's and nothing read it:
       js/panes/stats.js declares no in-ports, no host forwarder fills one, and the tile before it
       was a module (renderStats) that ignored the value too — a wire a person could draw that
       arrived nowhere — a port on this row has to be a port the tile reads. */
    pane('stats', 'Stats', mounts('stats'));
    pane('symbols', 'Symbols', mounts('symbols'), { inputs: [{ name: 'file', type: 'Path' }], outputs: [{ name: 'symbol', type: 'Text' }] });
    pane('editor', 'Editor', mounts('editor'), { inputs: [{ name: 'syntax', type: 'Bool' }], outputs: [['file', 'Path'], ['cursor', 'Cursor'], ['buffer', 'Text']] });
    pane('goal', 'Goal', mounts('goal'), { inputs: [['cursor', 'Cursor'], ['buffer', 'Text'], ['file', 'Path']] });
    pane('output', 'Output', mounts('output'), { inputs: [{ name: 'log', kind: 'event', type: 'Text' }] });
    pane('render', 'Lean · Render', mounts('render'), { inputs: [['buffer', 'Text'], { name: 'syntax', type: 'Bool' }] });
    pane('ir', 'IR · computed', mounts('ir'), { inputs: [['buffer', 'Text']] });
    pane('lean', 'Lean 4 · computed', mounts('lean'), { inputs: [['file', 'Path']] });
    /* THE LEAN SCRATCH EVALUATOR: a tile you type Lean into and get a term back. `ink` is the text it
       evaluates (a wire can fill it), and the rendered `latex`/`html` of the answer go out on its two
       out ports (js/panes/slate.js). */
    pane('slate', 'Slate · Lean scratch', mounts('slate'), { inputs: [['ink', 'Text']], outputs: [['latex', 'Text'], ['html', 'Text']] });
    /* the corpus build report: errors and warnings, each with its file and line */
    pane('diag', 'Diagnostics · report', mounts('diag'), { category: 'settings' });
    /* the wire layer's own switch: a boolean input, so a Constant or a program node can throw
       it, and a checkbox in the tile for when nothing is wired */
    pane('wires', 'Wires · view', mounts('wires'), { category: 'settings', inputs: [{ name: 'show', type: 'Bool' }], outputs: [{ name: 'visible', type: 'Bool' }] });
    pane('handwrite', 'Handwriting', mounts('handwrite'), { inputs: [{ name: 'file', type: 'Path' }] });
    /* data + behaviour (program) nodes — the two node categories */
    pane('constant', 'Constant · data', mounts('constant'), { category: 'data', outputs: [['value', 'any']] });
    /* a boolean source: flip it here, and any tile wired to its `on` port follows */
    pane('switch', 'Switch · data', mounts('switch'), { category: 'data', outputs: [{ name: 'on', type: 'Bool' }] });
    /* THE SWAP: a project tile names one project, a corpus tile serves the corpus of whatever
       project is wired into it, and the app reads that tile's four ports into its `data` — so one
       wire is the whole project interface, and snapping it empties the interface */
    pane('project', 'Project · source', mounts('project'), { category: 'source', outputs: [{ name: 'project', type: 'Project' }] });
    pane('corpus', 'Corpus · source', mounts('corpus'), {
      category: 'source',
      inputs: [{ name: 'project', type: 'Project' }],
      outputs: [['files', 'any'], ['graph', 'any'], ['stats', 'any'], ['symbols', 'any']],
    });
    /* MAKING A NODE — the feature the pre-migration client had and the canvas had lost: the pane
       exists (js/panes/new-node.js) and the engine host opens it, but this table never listed the
       kind, so the canvas could not hold one. `parent` is the path a tile was opened on, which the
       pane reads to offer the corpus's own eras, strata and directories. */
    pane('new-node', 'New node', mounts('new-node'), {
      category: 'source',
      /* NO `parent` INPUT HERE. That is the ENGINE host's notion — a pane opened ON a path, which
         the layout document gives it (js/defaults/wiring/_flows.js). A canvas tile is not opened on
         anything, so the port could never be filled and the drawer offered a wire into a hole; the
         pane reads the corpus itself (js/live/new-node-plan.js). */
      outputs: [{ name: 'node', type: 'Text' }, { name: 'created', type: 'Path' }],
    });
    /* the console, as tiles: core cannot be closed, the other two can */
    pane('source', 'Source · module', mounts('source'), { category: 'settings' });
    pane('core', 'Core · workspace', mounts('core'), { category: 'settings' });
    pane('view', 'View · settings', mounts('view'), { category: 'settings' });
    pane('projects', 'Project · charter', mounts('projects'), { category: 'settings' });
    pane('prefs', 'Editor · prefs', mounts('prefs'), { category: 'settings' });
    pane('js-node', 'JS Program · behaviour', null, {
      category: 'behaviour', inputs: [{ name: 'content', from: 'editor_buffer', type: 'Text' }], outputs: [{ name: 'result', type: 'any' }],
      program: 'function(inputs, ctx){\n\tvar t = inputs.content || "";\n\treturn {\n\t\tlines: t.trim() ? t.split("\\n").length : 0,\n\t\tchars: t.length,\n\t\thas_axiom: /axiom|\\\\forall/i.test(t)\n\t};\n}',
    });
    /* a settings node: colour palette → outputs CSS that restyles every pane live */
    /* THE BOARD PUBLISHES; IT DOES NOT TAKE. js/panes/palette.js declares `in: {}` and says why:
       "the board owns no theme state — it asks ctx.themes what exists and what is on, and every
       pick is a call back into an app that already knows how to apply and save a theme". The row
       offered a `theme` input anyway, so the drawer offered a wire into a port that is not there. */
    pane('palette', 'Theme · settings', mounts('palette'), {
      category: 'settings',
      outputs: [{ name: 'theme', type: 'CSS' }],
    });
    pane('theme-editor', 'Theme · editor', mounts('theme-editor'), { category: 'settings', inputs: [{ name: 'theme', type: 'JSON' }], outputs: [{ name: 'theme', type: 'JSON' }] });
    /* clock node: a time source other nodes can consume (loops run through it) */
    pane('clock', 'Clock · behaviour', null, {
      category: 'behaviour', inputs: [], outputs: [{ name: 'tick', type: 'num' }],
      program: 'function(inputs, ctx){\n\treturn { tick: ctx.clock, running: ctx.graphRun.running };\n}',
    });
    /* track map node: consumes the clock node's output (node→node wire demo) */
    pane('track', 'Track · behaviour', null, {
      /* `from: 'clock'`, NOT `'clock'`: a bare name is a NODE, and both readers of it
         (js/app/wires.js::resolveValue and js/app/graph-run.js::valueOf) answer a node with its
         whole result — so `inputs.tick` was the OBJECT `{tick, running}` on the tick a node is
         first run (before the scheduler's own ticked inputs take precedence), and the track program
         computed `beats` from NaN. Measured: `{"tick":{"tick":1,"running":true},"is_even":false,
         "beats":""}` on the first tick, the number from the second on. The field is what the port's
         type says it is. */
      category: 'behaviour', inputs: [{ name: 'tick', from: 'clock.tick', type: 'num' }], outputs: [{ name: 'meta', type: 'any' }],
      program: 'function(inputs, ctx){\n\tvar t = inputs.tick || 0;\n\treturn { tick: t, is_even: (t%2)===0, beats: "♥".repeat((t%5)+1) };\n}',
    });
  }

  /* ---- user panes: a manifest is a kind ---- */
  let manifests = [];               // the raw panes/manifests.json, so a program can be written back
  async function loadUserPanes() {
    try {
      manifests = await loadManifests((u) => app.fetch(u));
      for (const m of manifests) {
        const ports = portsOf(m);
        PANES[m.id] = Object.assign({ inputs: ports.inputs, outputs: ports.outputs, render: null, mount: null }, m);
        PANES[m.id].mount = userMount(m.render || null);
        PANES[m.id]._manifest = m;         // the entry this kind came from
      }
      app.populateKinds();
    } catch (e) { /* user panes are optional */ }
  }

  /* ---- "edit source": which source a kind has, and how it is edited and saved ---- */
  const paneSourceOf = (p) => sourceOf(p, { registry: PANES, panes });

  function applyPaneSource(target, text) {
    return applySource(target, text, {
      registry: PANES, panes, nodeRuns: app.nodeRuns(),
      userMount: (render) => userMount(render),
      behaviourMount: (rec) => behaviourMountOf(rec, app.behaviourCtx()),
      render: (q) => app.render(q),
      dirty: () => app.markDirty(),
      save: () => app.save(),
    });
  }

  async function savePaneSource(target) {
    return saveSource(target, {
      registry: PANES, panes, manifests,
      saveManifest: async (list) => {
        const r = await app.fetch(app.url('file.save'), {
          method: 'POST', headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ path: 'web-next/panes/manifests.json', content: JSON.stringify(list, null, 2) + '\n' }),
        });
        return !!r.ok;
      },
      toast: (m) => app.toast(m),
    });
  }

  /** One source tile per pane: asking twice brings the first one to the front. */
  function paneEditSource(p) {
    const existing = panes.find((q) => q.kind === 'source' && q.subjectPane === p.id);
    if (existing) { app.paneFocus(existing); app.toast('source · ' + p.kind); return existing; }
    const vp = deps.viewport();
    const cx = (vp.w / 2 - deps.cam.x) / deps.cam.z;
    const cy = (vp.h / 2 - deps.cam.y) / deps.cam.z;
    const np = app.makeTile(app.nextTileId('source'), 'source', { x: Math.round(cx - 280), y: Math.round(cy - 200), w: 560, h: 400 }, 'free', { subjectPane: p.id });
    app.setHotPane(np.el);
    app.toast('source · ' + p.kind);
    app.markDirty();
    app.save();
    return np;
  }

  return {
    PANES, CTX, pane, userMount, registerBuiltins, loadUserPanes,
    manifests: () => manifests,
    paneSourceOf, applyPaneSource, savePaneSource, paneEditSource,
    behaviourMount: (rec) => behaviourMountOf(rec, app.behaviourCtx()),
  };}
