/**
 * _flows.js — shared wiring flows (M4.31+).
 *
 * Each preset's wiring script (standard/minimal/power) composes these flows.
 * Flows connect pane out-ports to in-ports via ws.wire() and trigger refresh
 * of corpus data (files/stats) through the endpoints registered in
 * defaults/endpoints.js.
 */

export function flowsSelection(ws) {
  if (!ws || !ws.wire || !ws.pane) return;
  const { wire, pane } = ws;
  const nodes = pane('nodes');
  const tree = pane('tree');
  const editor = pane('editor');
  const goal = pane('goal');
  const lean = pane('lean');
  const ir = pane('ir');
  const render = pane('render');
  const diag = pane('diag');
  const logo = pane('logo');
  const graph = pane('graph');

  if (nodes && nodes.out && nodes.out.selection && editor && editor.in && editor.in.file) {
    wire(nodes.out.selection, editor.in.file, { map: (id) => id ? id + '.qbit' : '' });
  }
  if (tree && tree.out && tree.out.selection && editor && editor.in && editor.in.file) {
    wire(tree.out.selection, editor.in.file);
  }
  if (editor && editor.out && editor.out.cursor && goal && goal.in && goal.in.cursor) {
    wire(editor.out.cursor, goal.in.cursor);
  }
  if (editor && editor.out && editor.out.cursor && lean && lean.in && lean.in.file) {
    wire(editor.out.cursor, lean.in.file, { map: (c) => c && c.path ? c.path : '' });
  }
  if (editor && editor.out && editor.out.buffer) {
    const targets = [];
    if (goal && goal.in && goal.in.buffer) targets.push(goal.in.buffer);
    if (lean && lean.in && lean.in.buffer) targets.push(lean.in.buffer);
    if (ir && ir.in && ir.in.buffer) targets.push(ir.in.buffer);
    if (render && render.in && render.in.buffer) targets.push(render.in.buffer);
    for (const dst of targets) wire(editor.out.buffer, dst);
  }
  if (ir && ir.out && ir.out.applied && editor && editor.call && editor.call.goto) {
    /* the map runs on EVERY value the port publishes, and a pane that has not produced one yet
       publishes null — so the map answers null rather than reading through it (this threw as
       soon as the wiring actually delivered anything, which is what the instance manager's port
       binding had been preventing) */
    wire(ir.out.applied, editor.call.goto, { map: (r) => (r && r.path ? { path: r.path, line: 1, col: 0 } : null) });
  }
  if (diag && diag.out && diag.out.jump && editor && editor.in && editor.in.goto) {
    wire(diag.out.jump, editor.in.goto);
  }
  if (editor && editor.out && editor.out.cursor && logo && logo.in && logo.in.file) {
    wire(editor.out.cursor, logo.in.file, { map: (c) => c && c.path ? c.path : '' });
  }
  if (nodes && nodes.out && nodes.out.selection && graph && graph.in && graph.in.highlight) {
    wire(nodes.out.selection, graph.in.highlight);
  }
  if (graph && graph.out && graph.out.selection && nodes && nodes.in && nodes.in.highlight) {
    wire(graph.out.selection, nodes.in.highlight);
  }
  if (goal && goal.out && goal.out.goalAt && nodes && nodes.in && nodes.in.highlight) {
    wire(goal.out.goalAt, nodes.in.highlight, { map: (g) => g && g.label ? g.label : '' });
  }
}

export function flowsCheckBuild(ws) {
  if (!ws || !ws.wire || !ws.pane) return;
  const { wire, pane, on, emit, endpoint } = ws;
  const editor = pane('editor');
  const output = pane('output');
  const diag = pane('diag');
  let timer = null;

  if (editor && editor.out && editor.out.buffer && endpoint) {
    wire(editor.out.buffer, null, {
      map: (buf) => {
        if (timer) clearTimeout(timer);
        timer = setTimeout(async () => {
          try {
            const ep = endpoint('ws.check');
            if (ep && ep.call && ep.call.send && ep.call.send.invoke) await ep.call.send.invoke({ path: '', content: buf });
          } catch {}
        }, 700);
      },
    });
  }

  if (on) {
    on('ui:build', async () => {
      try {
        const ep = endpoint('ws.build');
        if (ep && ep.call && ep.call.send && ep.call.send.invoke) await ep.call.send.invoke('build');
        emit('build:settled');
      } catch {}
    });
    on('build:settled', async () => {
      try { if (diag && diag.call && diag.call.rerun) await diag.call.rerun.invoke({}); } catch {}
    });
  }

  if (editor && editor.out && editor.out.saved && diag && diag.call && diag.call.rerun) {
    wire(editor.out.saved, null, { map: async (path) => { try { await diag.call.rerun.invoke({ path }); } catch {} } });
  }
}

export function flowsChrome(ws) {
  if (!ws || !ws.on || !ws.emit || !ws.pane) return;
  const modalMap = { 'ui:graph': 'graph', 'ui:slate': 'slate', 'ui:symbol': 'symbol-add', 'ui:settings': 'settings-theme', 'ui:handwrite': 'handwrite' };
  for (const [action, kind] of Object.entries(modalMap)) {
    ws.on(action, () => {
      const p = ws.pane(kind);
      if (p && p.call && p.call.open) p.call.open.invoke({});
      else ws.emit('modal:' + kind, {});
    });
  }
  const render = ws.pane('render');
  if (render && render.call && render.call.pdf && ws.on) {
    ws.on('ui:pdf', async () => { try { await render.call.pdf.invoke({}); } catch {} });
  }
  const newNode = ws.pane('new-node');
  if (newNode && newNode.call && newNode.call.open && ws.on) {
    ws.on('ui:new-node', async () => { try { await newNode.call.open.invoke({}); } catch {} });
  }
}

export function flowsData(ws) {
  if (!ws || !ws.wire || !ws.pane) return;
  const { wire, pane, on, endpoint } = ws;
  const tree = pane('tree');
  const nodes = pane('nodes');
  const graph = pane('graph');
  const stats = pane('stats');
  const editor = pane('editor');
  const newNode = pane('new-node');
  const symbolAdd = pane('symbol-add');

  async function refreshData() {
    if (!endpoint) return;
    try {
      let fileCount = 0;
      // files.list -> corpus file paths (used by the tree pane)
      const filesEp = endpoint('files.list');
      if (filesEp && filesEp.call && filesEp.call.invoke && filesEp.call.invoke.invoke) {
        const files = await filesEp.call.invoke.invoke({});
        const list = files && Array.isArray(files.files) ? files.files : (Array.isArray(files) ? files : []);
        fileCount = list.length;
        if (tree && tree.in && tree.in.files && tree.in.files.set) tree.in.files.set(list);
      }
      // graph.get -> structured nodes {id,type,stratum,label}; the nodes pane
      // expects {data:{id,label,type,era,level}}.
      const graphEp = endpoint('graph.get');
      if (graphEp && graphEp.call && graphEp.call.invoke && graphEp.call.invoke.invoke) {
        const g = await graphEp.call.invoke.invoke({});
        const gdata = g && g.graph ? g.graph : (g || {});
        const rawNodes = Array.isArray(gdata.nodes) ? gdata.nodes : [];
        const corpusNodes = rawNodes.map((n, i) => {
          const id = n && (n.id ?? n.path ?? (String(fileCount ? i + 1 : i)));
          return {
            data: {
              id: String(id),
              label: n && n.label ? n.label : String(id),
              type: n && n.type ? n.type : '',
              file: n && n.file ? n.file : '',
              era: n && n.stratum !== undefined ? String(n.stratum) : '',
              level: n && n.level ? String(n.level) : '',
            },
          };
        });
        if (nodes && nodes.in && nodes.in.corpus && nodes.in.corpus.set) {
          nodes.in.corpus.set({ nodes: corpusNodes, edges: [] });
        }
        if (graph && graph.in && graph.in.graphData && graph.in.graphData.set) {
          graph.in.graphData.set({ nodes: corpusNodes, edges: Array.isArray(gdata.edges) ? gdata.edges : [] });
        }
      }
      const statsEp = endpoint('stats.get');
      if (statsEp && statsEp.call && statsEp.call.invoke && statsEp.call.invoke.invoke) {
        const raw = await statsEp.call.invoke.invoke({});
        const s = raw && raw.stats ? raw.stats : (raw || {});
        const norm = { nodes: (fileCount || s.nodes) ?? '–', axioms: s.axioms ?? '–', theorems: s.theorems ?? '–', deps: s.deps ?? (s.operations ?? '–') };
        if (stats && stats.in && stats.in.stats && stats.in.stats.set) stats.in.stats.set(norm);
      }
    } catch {}
  }

  refreshData();
  if (on) on('build:settled', refreshData);
  if (editor && editor.out && editor.out.saved && on) wire(editor.out.saved, null, { map: refreshData });
  if (newNode && newNode.out && newNode.out.created && on) on('new-node.created', refreshData);
  if (symbolAdd && symbolAdd.out && symbolAdd.out.committed && on) on('symbol-add.committed', refreshData);
}