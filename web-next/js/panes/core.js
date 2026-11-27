/**
 * panes/core.js — Core pane: the workspace itself.
 *
 * Create tiles, cluster them, save the workspace, run the graph. This is the pane that
 * cannot be closed — everything else can — so it is also where the keyboard that drives
 * the canvas is written down: a workspace you cannot steer is a workspace you cannot
 * leave.
 *
 * In:  {}   Out: {}   Call: `save`, `step`, `reset`
 *
 * The pane is a list of sections and calls; every action belongs to the app, reached
 * through ctx (addTile, clusterLoose, addRoot, save, resetLayout, frameAll, defaultView,
 * graph). It builds buttons, wires them to those calls, and reports the counts it can
 * see — so the console says what the workspace is, not what it was when the tile opened.
 */

import { callPort } from '../core/ports.js';
import { ctlRow, ctlSec, ctlNote, ctlBtn, ctlPill, ctlGroup } from '../app/ctl.js';

export default {
  kind: 'core',
  title: 'Core',
  ports: { in: {}, out: {}, call: { save: 'call', step: 'call', reset: 'call' } },
  create(ctx) {
    let el = null, pause = null, steps = null;
    let ops = null, data = null, wires = null, graphLine = null;

    const graph = () => (ctx.graph ? ctx.graph() : { running: false, tick: 0 });
    const counts = () => (ctx.counts ? ctx.counts() : { panes: 0, roots: 0 });

    /** The three readouts and the pause glyph, refreshed without re-mounting. */
    function paint() {
      const g = graph();
      if (pause) pause.textContent = g.running ? '⏸' : '▶';
      if (steps) steps.textContent = 'tick ' + (g.tick || 0);
      const c = ctx.counts ? ctx.counts() : null;
      if (ops) ops.textContent = String(c && c.ops != null ? c.ops : '–');
      if (wires) wires.textContent = String(c && c.wires != null ? c.wires : '–');
      if (data) {
        const d = (ctx.data || {});
        const nodes = ((d.graph && d.graph.nodes) || []).length;
        const syms = (d.symbols || []).length;
        data.textContent = nodes + ' / ' + syms;
      }
      if (graphLine) graphLine.textContent = 'tick ' + (g.tick || 0) + ' · ' + (c ? c.panes : 0) + ' tiles · ' + (c ? c.roots : 0) + ' roots';
    }

    const callSave = callPort(() => { if (ctx.saveWorkspace) ctx.saveWorkspace(); return { ok: true }; });
    const callStep = callPort(() => { if (ctx.graphStep) { ctx.graphStep(); paint(); } return { ok: true, tick: graph().tick }; });
    const callReset = callPort(() => { if (ctx.resetLayout) ctx.resetLayout(); return { ok: true }; });

    return {
      mount(host) {
        el = host;

        const kindSel = document.createElement('select');
        kindSel.id = 's-kind';
        kindSel.title = 'what kind of tile to make';
        if (ctx.populateKinds) ctx.populateKinds(kindSel);

        const add = ctlBtn('b-add', '+ add', 'make a loose tile');
        const newNode = ctlBtn('b-new-node', '+ node', 'add an axiom, definition or theorem');
        newNode.addEventListener('click', () => { if (ctx.addTile) ctx.addTile('new-node'); });
        const cluster = ctlBtn('b-cluster', '+ cluster', 'tile the loose tiles into one cluster');
        const root = ctlBtn('b-newcluster', '+ root', 'another cluster of the default kinds');
        const save = ctlBtn('b-save', 'save layout', 'write the arrangement (tiles, positions, wires) to the server now');
        const saveFile = ctlBtn('b-save-file', 'save file', 'write the open document to the corpus  (Ctrl+S; what follows it is the save mode, in the prefs tile)');
        saveFile.addEventListener('click', () => { if (ctx.saveActiveDoc) ctx.saveActiveDoc(); });
        const reset = ctlBtn('b-reset-layout', 'reset', 'forget the saved workspace entirely, then reload');
        pause = ctlBtn('b-pause', graph().running ? '⏸' : '▶', 'pause or resume the graph');
        const step = ctlBtn('b-step', '⏭ step', 'advance the graph one tick');
        ops = ctlPill('h-ops', '–');
        data = ctlPill('h-data', '–');
        wires = ctlPill('h-wires', '–');

        add.addEventListener('click', () => { if (ctx.addTile) ctx.addTile(kindSel.value); });
        cluster.addEventListener('click', () => {
          if (!ctx.clusterLoose || !ctx.clusterLoose()) ctx.ui.toast('nothing loose to cluster');
        });
        root.addEventListener('click', () => { if (ctx.addRoot) ctx.addRoot(); });
        save.addEventListener('click', () => callSave.invoke());
        reset.addEventListener('click', () => callReset.invoke());
        pause.addEventListener('click', () => {
          if (!ctx.graphRun) return;
          ctx.graphRun(!graph().running);
          paint();
        });
        step.addEventListener('click', () => callStep.invoke());

        const def = ctlBtn('b-default-view', 'default view', 'put the views back the way they ship: the writing root framed, the corpus pair, the reference root and the console one pan away');
        const fitAll = ctlBtn('b-fit-all', 'fit', 'frame every tile  (0)');
        def.addEventListener('click', () => { if (ctx.defaultView) ctx.defaultView(); });
        fitAll.addEventListener('click', () => { if (ctx.frameAll) ctx.frameAll(); });

        steps = document.createElement('span');
        steps.className = 'pill';
        graphLine = document.createElement('div');
        graphLine.className = 'ctl-meta';
        paint();

        const keys = document.createElement('div');
        keys.id = 'keys';
        keys.innerHTML = '<b>Ctrl+wheel</b> / pinch zoom · <b>wheel</b> pans · <b>+</b> / <b>−</b> zoom<br>'
          + '<b>0</b> fit everything · <b>1</b> 100% · <b>Space+drag</b> pans<br>'
          + '<b>◎</b> pins a text tile to its file · <b>⌖</b> shares a selection<br>'
          + '<b>⛶</b> fills the viewport · <b>Esc</b> leaves it · <b>w</b> wires';

        const sections = [
          ctlSec('create', [ctlRow('add tile', kindSel, 'add'), ctlRow('tile', ctlGroup(add, newNode, cluster)), ctlRow('tiling', ctlGroup(root))]),
          ctlSec('views', [ctlRow('restore', ctlGroup(def)), ctlRow('camera', ctlGroup(fitAll)), ctlRow('run', ctlGroup(pause, step, steps))]),
          ctlSec('workspace', [ctlRow('persist', ctlGroup(save, saveFile, reset))]),
          ctlSec('run', [ctlRow('ops · results', ops), ctlRow('wires live', wires)]),
          ctlSec('data', [ctlRow('nodes / symbols / ops', data)]),
          graphLine,
          keys,
          ctlNote('the core tile stays in the workspace: everything else can be closed'),
        ];

        if (ctx.controlColumns) ctx.controlColumns(host, sections);
        else host.append(...sections);
        paint();
      },
      unmount() {
        el = pause = steps = ops = data = wires = graphLine = null;
      },
      refresh: paint,
      in: {}, out: {},
      call: { save: callSave, step: callStep, reset: callReset },
      get ports() { return { in: this.in, out: this.out, call: this.call }; },
    };
  },
};