/*
 * panes/app-mounts.js — how a pane is mounted in THIS app.
 *
 * A pane module is one pane (js/panes/*.js): it draws itself, it declares ports, it owns
 * nothing about the workspace. What the canvas needs on top of that is a thin, per-kind
 * integration — the card's one-line summary, the tile's own wired inputs, a chrome hook for
 * the panes that show state the app changes — and that integration was seventeen near-identical
 * functions in web-next/index.html:
 *
 *   mount the module; set `p.summary`; (sometimes) watch the active file; (sometimes) forward
 *   the tile's wired inputs into the pane's in-port; (sometimes) publish the pane's out-port
 *   where the wire layer looks for it.
 *
 * Written out once per kind, the copies drifted: a pane that publishes a port was wired in one
 * list and not another, and "which kinds follow the active file" was a fact you had to read
 * seventeen functions to learn. Here it is a TABLE, so the differences between kinds are
 * visible in one screen:
 *
 *   module   which pane module to mount
 *   summary  what the tile's card says (it is the tile's, so it is here and not in the pane)
 *   before   what has to happen before the mount (a body class, a pane's own wiring)
 *   after    what to do with the instance (forward an in-port, publish an out-port)
 *
 * A kind that is not in the table is not this module's business: the canvas has three of its
 * own (the tree, the graph and the editor build on dynamic imports and page callbacks) and the
 * behaviour nodes run user code, which js/panes/user-panes.js owns.
 */

import STATS_PANE from './stats.js';
import SYMBOLS_PANE from './symbols.js';
import GOAL_PANE from './goal.js';
import NEW_NODE_PANE from './new-node.js';
import OUTPUT_PANE from './output.js';
import RENDER_PANE from './render.js';
import IR_PANE from './ir.js';
import LEAN_PANE from './lean.js';
import WIRES_PANE from './wires.js';
import CONSTANT_PANE from './constant.js';
import SWITCH_PANE from './switch.js';
import SOURCE_PANE from './source.js';
import CORE_PANE from './core.js';
import VIEW_PANE from './view.js';
import PREFS_PANE from './prefs.js';
import PALETTE_PANE from './palette.js';
import PROJECTS_PANE from './projects.js';
import DIAG_PANE from './diag.js';
import THEME_EDITOR_PANE from './theme-editor.js';
import PROJECT_PANE from './project.js';
import CORPUS_PANE from './corpus.js';
import SLATE_PANE from './slate.js';

/**
 * @param {object} host
 * @param {object} host.paneHost            js/app/pane-host.js: mount(def, tile)
 * @param {object} host.data                the corpus the app loaded
 * @param {(p:object) => string} host.docPath        the file a tile is showing
 * @param {(p:object) => void} host.watchActive      follow the active document
 * @param {(p:object) => void} [host.redraw]         re-render a tile
 * @param {(name:string, port:object) => (() => void)} [host.registerSource]  a named port source
 * @param {() => object} [host.results]     the graph's result table, for the data nodes
 * @param {() => number} [host.wireCount]   how many wires are live, for the wires tile
 * @param {() => object|null} [host.counts] the corpus check's counts, for the report tile
 * @param {() => object} [host.cursor]      the caret, for the goal tile
 * @param {(fn:Function) => Function} [host.watchCursor]  subscribe to the caret
 * @param {() => object[]} [host.panes]     the live tiles, for the source tile
 * @param {(m:string) => void} [host.toast]
 * @param {() => void} [host.save]
 */
export function createAppMounts(host) {
  const { paneHost, data } = host;
  const base = (p, module) => paneHost.mount(module, p);
  const hostClass = (p) => p.body.classList.add('ctl-host');
  const active = (host.watchActive || (() => {}));
  const redraw = host.redraw || (() => {});
  const toast = host.toast || (() => {});
  const save = host.save || (() => {});
  /** How many wires are live: the seeded list plus the ones drawn by hand. */
  const wires = () => (host.wireCount ? host.wireCount() : 0);

  const TABLE = {
    stats: {
      module: STATS_PANE,
      summary: () => (((data().graph && data().graph.nodes) ? data().graph.nodes.length : 0) + ' nodes'),
    },
    symbols: {
      module: SYMBOLS_PANE,
      before: (p) => active(p),
      summary: () => ((data().symbols || []).length + ' symbols'),
    },
    goal: {
      module: GOAL_PANE,
      before: (p) => active(p),
      after: (p) => {
        /* the card says where the caret is, so the card has to follow the caret — and the
           subscription is dropped with the tile, or a closed goal tile keeps a listener on
           every cursor move for the life of the page */
        const stamp = () => {
          p.summary = () => '⊢ L' + (((host.cursor ? host.cursor() : {}) || {}).line || 1);
          if (p._summary && p.card && p.card.lastChild) p.card.lastChild.textContent = p.summary();
        };
        stamp();
        const off = host.watchCursor ? host.watchCursor(stamp) : null;
        if (typeof off === 'function') {
          const release = p._paneUnmount;
          p._paneUnmount = () => { off(); if (release) release(); };
        }
      },
    },
    /* MAKING A NODE. The pane has existed since M4.16 and only the ENGINE host could mount it: the
       canvas's kind table did not list the kind, so no tile could be one and the add-tile menu never
       offered it. It is here now, and its form renders in the tile rather than as a modal
       (js/panes/new-node.js decides which shape its host wants). */
    'new-node': { module: NEW_NODE_PANE, summary: () => 'new node' },
    /* A WIRE CAN APPEND A LINE. The pane's `log` in-port has existed as an event port since the
       log tile was written, and no host delivered a value into it — in the canvas a wire drawn to
       it landed on `onInputs`, which this kind did not define. */
    output: {
      module: OUTPUT_PANE,
      summary: () => 'build · diag',
      after: (p, inst) => {
        /* composes: the host's own forwarder (js/app/pane-host.js) delivers every wired input
           into the pane's ports, and this kind's handler runs after it */
        const prevOnInputs = p.onInputs;
        p.onInputs = (ins) => { if (prevOnInputs) prevOnInputs(ins);
          if (inst && inst.in && inst.in.log && ins && ins.log !== undefined) inst.in.log.emit(ins.log);
        };
      },
    },
    render: {
      module: RENDER_PANE,
      before: (p) => active(p),
      summary: (p) => (basename(host.docPath(p)) || 'render'),
      after: (p, inst) => {
        // the tile's own wired inputs are the pane's syntax port (undefined = nothing wired)
        /* composes: the host's own forwarder (js/app/pane-host.js) delivers every wired input
           into the pane's ports, and this kind's handler runs after it */
        const prevOnInputs = p.onInputs;
        p.onInputs = (ins) => { if (prevOnInputs) prevOnInputs(ins); if (inst && inst.in && inst.in.syntax) inst.in.syntax.set(ins && ins.syntax); };
      },
    },
    ir: {
      module: IR_PANE,
      before: (p) => active(p),
      summary: (p) => 'IR · ' + (basename(host.docPath(p)) || '—'),
    },
    lean: {
      module: LEAN_PANE,
      before: (p) => active(p),
      summary: (p) => 'Lean · ' + (basename(host.docPath(p)) || '—'),
    },
    wires: {
      module: WIRES_PANE,
      summary: () => 'wires · ' + wires(),
      after: (p, inst) => {
        const prev = p.onInputs;
        p.onInputs = (ins) => { if (inst && inst.in && inst.in.show) inst.in.show.set(ins && ins.show); };
        // the page repaints chrome constantly; the pane redraws itself, it is not re-mounted
        p.chrome = () => { if (p.refreshPane) p.refreshPane(); };
        if (!inst) p.onInputs = prev;
      },
    },
    constant: {
      module: CONSTANT_PANE,
      after: (p, inst) => {
        /* A CONSTANT IS A WIRE SOURCE, registered per TILE — the way js/panes/project.js and
           js/panes/switch.js publish theirs. It was mirrored into the graph's results table under the
           bare KIND name instead (`results['constant'] = v`), and the wire resolver looks for the port
           under that name (`js/live/wire/host.js::defaultValue`: `results[kind][port]`), so a wire
           drawn from a Constant carried NOTHING — measured: the slate tile's `ink` stayed undefined
           behind a wire that resolved and was painted. The kind name was
           also the only name, so two Constant tiles in one workspace overwrote each other.
           The results mirror stays, for the readers that ask the graph by KIND (`ctx.resolveFrom
           ('constant')`); the wire path is the registered port. */
        const port = inst && inst.out && inst.out.value;
        if (port && typeof port.subscribe === 'function') {
          const release = p._paneUnmount;
          const offs = [];
          if (host.registerSource) offs.push(host.registerSource(p.id + '_value', port));
          offs.push(port.subscribe((v) => { if (host.results) host.results()['constant'] = v; }));
          // a tile may already have a teardown (the mount sets one): chain, do not replace
          p._paneUnmount = () => { for (const off of offs) { if (typeof off === 'function') off(); } if (typeof release === 'function') release(); };
        }
        if (inst && inst.setSummary) {
          inst.setSummary((v) => {
            p.summary = () => 'data · ' + (typeof v);
            if (p._summary && p.card && p.card.lastChild) p.card.lastChild.textContent = p.summary();
          });
        }
      },
    },
    switch: { module: SWITCH_PANE },
    /* THE SCRATCH EDITOR. Its `ink` in-port is the corpus text to parse, and the pane took that text
       from a wire in the engine host too — this canvas had no kind for it, so the tile could not be
       made at all. It is not a Lean evaluator: the slate route parses `.qbit`. */
    slate: {
      module: SLATE_PANE,
      summary: () => 'corpus text',
      after: (p, inst) => {
        /* composes: the host's own forwarder (js/app/pane-host.js) delivers every wired input
           into the pane's ports, and this kind's handler runs after it */
        const prevOnInputs = p.onInputs;
        p.onInputs = (ins) => { if (prevOnInputs) prevOnInputs(ins);
          if (inst && inst.in && inst.in.ink) inst.in.ink.set(ins && ins.ink);
        };
      },
    },
    source: {
      module: SOURCE_PANE,
      after: (p, inst) => {
        if (inst && inst.setSubject) inst.setSubject(p.subjectPane);
        p.summary = () => {
          const t = (host.panes ? host.panes() : []).find((q) => q.id === p.subjectPane);
          return t ? t.kind + ' · source' : 'source';
        };
      },
    },
    core: { module: CORE_PANE, before: hostClass },
    view: { module: VIEW_PANE, before: hostClass },
    prefs: { module: PREFS_PANE, before: hostClass },
    palette: { module: PALETTE_PANE },
    projects: { module: PROJECTS_PANE },
    diag: {
      module: DIAG_PANE,
      after: (p, inst) => {
        p.chrome = () => { if (inst && inst.refresh) inst.refresh(); };
        // read the counts when the card ASKS: a check that finishes while the tile is a
        // placeholder would otherwise leave the placeholder saying "not checked"
        p.summary = () => {
          const c = host.counts ? host.counts() : null;
          return c ? (c.errors ? c.errors + ' errors' : c.warnings ? c.warnings + ' warnings' : 'clean') : 'not checked';
        };
        if (p._summary && p.card && p.card.lastChild) p.card.lastChild.textContent = p.summary();
      },
    },
    /* THE SWAP, AS TWO ROWS. A project tile publishes the project it names (a named source, like
       the theme editor's); a corpus tile is handed one on its in-port and publishes that project's
       corpus — and the app READS those four ports into its own `data`, which is what every pane in
       the interface draws from. So one wire (project → corpus) is the whole project interface:
       snap it and the app has no corpus, wire another project in and everything follows. */
    project: {
      module: PROJECT_PANE,
      summary: () => 'project',
      after: (p, inst) => {
        const port = inst && inst.out && inst.out.project;
        if (port && host.registerSource) host.registerSource(p.id + '_project', port);
      },
    },
    corpus: {
      module: CORPUS_PANE,
      summary: () => 'corpus',
      after: (p, inst) => {
        // what the wire delivers is this tile's `project` in-port
        /* composes: the host's own forwarder (js/app/pane-host.js) delivers every wired input
           into the pane's ports, and this kind's handler runs after it */
        const prevOnInputs = p.onInputs;
        p.onInputs = (ins) => { if (prevOnInputs) prevOnInputs(ins); if (inst && inst.in && inst.in.project) inst.in.project.set(ins && ins.project); };
        // …and what the tile publishes is the app's corpus
        if (host.corpusFrom) host.corpusFrom(inst, p);
      },
    },
    'theme-editor': {
      module: THEME_EDITOR_PANE,
      after: (p, inst) => {
        const port = inst.out && inst.out.theme;
        if (port && host.registerSource) {
          host.registerSource(p.id + '_theme', port);
          host.registerSource('theme-editor_theme', port);
        }
        // a wire into the tile hands it a theme module; the pane keeps its own draft per tile
        /* composes: the host's own forwarder (js/app/pane-host.js) delivers every wired input
           into the pane's ports, and this kind's handler runs after it */
        const prevOnInputs = p.onInputs;
        p.onInputs = (ins) => { if (prevOnInputs) prevOnInputs(ins);
          if (ins && ins.theme && typeof ins.theme === 'object' && ins.theme.vars && inst.in && inst.in.theme) {
            inst.in.theme.set(ins.theme);
            toast('editor: theme in');
          }
        };
      },
    },
  };

  const has = (kind) => Object.prototype.hasOwnProperty.call(TABLE, kind);

  /**
   * Mount one kind into one tile. Returns the pane instance, or null if this kind is not
   * this app's table's business.
   */
  function mount(kind, p) {
    const entry = TABLE[kind];
    if (!entry) return null;
    if (entry.before) entry.before(p);
    const inst = base(p, entry.module);
    if (entry.after) entry.after(p, inst);
    if (entry.summary) {
      p.summary = () => entry.summary(p);
      if (p._summary && p.card && p.card.lastChild) p.card.lastChild.textContent = p.summary();
    }
    void redraw; void save;
    return inst || null;
  }

  /** A mount function for the pane registry: `PANES[kind].mount = mountFor(kind)`. */
  const mountFor = (kind) => (p) => { mount(kind, p); };

  return { TABLE, has, mount, mountFor, kinds: () => Object.keys(TABLE) };
}

const basename = (path) => String(path || '').split('/').pop();