/**
 * live/graph.js — the dependency graph, drawn as a real DAG.
 *
 * Two layouts, one drawing model:
 *
 *   LAYERED  dagre (vendored) computes a layered layout. Several candidates
 *            (rank direction × rank spread) are computed once and the one whose
 *            aspect can best fill the tile wins; leftover slack goes into the
 *            cross axis only, so rank-mates spread further apart instead of the
 *            circles turning into ellipses.
 *   FORCE    a small Fruchterman–Reingold spring solver, normalised to the tile.
 *            It fills any aspect, which is what the layered drawing cannot do
 *            when the graph is one long chain.
 *
 * On a very elongated layered drawing the pane falls back to FORCE by itself;
 * the switch in the legend overrides that.
 *
 * The drawing is SVG (crisp at any camera zoom) and node radius/label size are
 * set in *screen* pixels, so a dot is the same size in any tile.
 *
 *   mountGraph(host, { nodes, edges, onPick, selected })
 *     -> { setSelected, highlight, repaint, mode }
 */

let dagreReady = null;
function loadDagre() {
  if (window.dagre) return Promise.resolve(window.dagre);
  if (dagreReady) return dagreReady;
  dagreReady = new Promise((res, rej) => {
    const s = document.createElement('script');
    s.src = '/vendor/dagre.min.js';
    s.onload = () => res(window.dagre);
    s.onerror = () => rej(new Error('dagre unavailable'));
    document.head.append(s);
  });
  return dagreReady;
}

const KIND_COLOR = {
  axiom: 'var(--green)',
  definition: 'var(--cyan)',
  theorem: 'var(--violet)',
  calibration: 'var(--amber)',
};
export const kindColor = (t) => KIND_COLOR[String(t || '').toLowerCase()] || 'var(--t2)';

const SVGNS = 'http://www.w3.org/2000/svg';
const el = (n, attrs = {}) => {
  const e = document.createElementNS(SVGNS, n);
  for (const k in attrs) e.setAttribute(k, attrs[k]);
  return e;
};
const clamp = (v, a, b) => Math.max(a, Math.min(b, v));

const RANK_SPREAD = [1, 1.8, 3];
const MAX_STRETCH = 1.6;
const MIN_FILL = 0.62; // below this a layered drawing reads as a thread

/* ── force layout ─────────────────────────────────────────────────────────
   Positions are normalised to 0..1 in both axes, so a resize is only a
   rescale — the simulation never has to run twice. Deterministic seed, so
   the same corpus always draws the same picture.                        */
function forcePositions(ids, links, seed = 7) {
  const n = ids.length;
  const px = new Float64Array(n);
  const py = new Float64Array(n);
  let s = seed >>> 0 || 1;
  const rnd = () => ((s = (s * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff);
  for (let i = 0; i < n; i++) {
    const a = (i / n) * Math.PI * 2 + rnd() * 0.8;
    const r = 0.2 + 0.8 * Math.sqrt(rnd());
    px[i] = Math.cos(a) * r;
    py[i] = Math.sin(a) * r;
  }
  const ix = new Map(ids.map((id, i) => [id, i]));
  const E = links.map((l) => [ix.get(l.from), ix.get(l.to)]).filter((e) => e[0] != null && e[1] != null);
  const k = Math.sqrt(1 / Math.max(1, n)) * 1.15; // ideal edge length in unit area
  let t = 0.11;
  const dx = new Float64Array(n);
  const dy = new Float64Array(n);
  for (let it = 0; it < 380; it++) {
    dx.fill(0);
    dy.fill(0);
    for (let i = 0; i < n; i++) {
      for (let j = i + 1; j < n; j++) {
        let ax = px[i] - px[j], ay = py[i] - py[j];
        let d = Math.hypot(ax, ay) || 1e-5;
        const f = ((k * k) / d) * 0.06;
        ax /= d; ay /= d;
        dx[i] += ax * f; dy[i] += ay * f;
        dx[j] -= ax * f; dy[j] -= ay * f;
      }
    }
    for (const [a, b] of E) {
      let ax = px[a] - px[b], ay = py[a] - py[b];
      let d = Math.hypot(ax, ay) || 1e-5;
      const f = ((d * d) / k) * 0.06;
      ax /= d; ay /= d;
      dx[a] -= ax * f; dy[a] -= ay * f;
      dx[b] += ax * f; dy[b] += ay * f;
    }
    for (let i = 0; i < n; i++) {
      // weak gravity keeps weakly connected components from flying away
      dx[i] -= px[i] * 0.012;
      dy[i] -= py[i] * 0.012;
      const len = Math.hypot(dx[i], dy[i]) || 1e-5;
      const step = Math.min(len, t);
      px[i] += (dx[i] / len) * step;
      py[i] += (dy[i] / len) * step;
    }
    t = Math.max(0.005, t * 0.994);
  }
  /* Anchor the drawing on the median and measure it by the 85th percentile of
     the distance to that centre. Both axes share one scale, so a spring layout
     keeps its proportions; the tail beyond the percentile stays in the data and
     is only *softened* at paint time (tanh), never clipped to a corner. */
  const med = (arr) => {
    const a = [...arr].sort((u, v) => u - v);
    return a[Math.floor((a.length - 1) / 2)];
  };
  const cx = med(px), cy = med(py);
  const rad = ids.map((_, i) => Math.hypot(px[i] - cx, py[i] - cy)).sort((u, v) => u - v);
  const R = rad[Math.min(rad.length - 1, Math.floor(0.85 * (rad.length - 1)))] || 1;
  const out = new Map();
  ids.forEach((id, i) => out.set(id, {
    nx: (px[i] - cx) / R,
    ny: (py[i] - cy) / R,
  }));
  return out;
}

export async function mountGraph(host, { nodes, edges, onPick, selected }) {
  const dagre = await loadDagre();

  if (!nodes.length) {
    host.innerHTML = '<div class="empty">no graph yet</div>';
    return { setSelected() {}, highlight() {}, repaint() {}, mode: () => 'layered' };
  }

  const norm = nodes.map((n, i) => ({
    id: String(n.id ?? n.name ?? n.godel ?? i),
    label: String(n.label ?? n.name ?? n.id ?? n.godel ?? i),
    kind: String(n.kind ?? n.type ?? '').toLowerCase(),
  }));
  const ids = new Set(norm.map((n) => n.id));
  const links = edges
    .map((e) => ({ from: String(e.source), to: String(e.target) }))
    .filter((e) => ids.has(e.from) && ids.has(e.to) && e.from !== e.to);

  const deg = new Map(norm.map((n) => [n.id, 0]));
  for (const e of links) {
    deg.set(e.from, (deg.get(e.from) || 0) + 1);
    deg.set(e.to, (deg.get(e.to) || 0) + 1);
  }
  const byId = new Map(norm.map((n) => [n.id, n]));
  const linked = norm.filter((n) => (deg.get(n.id) || 0) > 0);
  const loose = norm.filter((n) => (deg.get(n.id) || 0) === 0);
  const linkedIds = linked.map((n) => n.id);
  const weight = (id) => clamp(15 + Math.sqrt(deg.get(id) || 0) * 4.6, 15, 30);

  const neighbours = new Map();
  for (const l of links) {
    if (!neighbours.has(l.from)) neighbours.set(l.from, new Set());
    if (!neighbours.has(l.to)) neighbours.set(l.to, new Set());
    neighbours.get(l.from).add(l.to);
    neighbours.get(l.to).add(l.from);
  }
  const hubIds = [...linked]
    .sort((a, b) => (deg.get(b.id) || 0) - (deg.get(a.id) || 0))
    .slice(0, 5)
    .map((n) => n.id);
  const hubs = new Set(hubIds);

  /* ── layered candidates ───────────────────────────────────────────────── */
  function build(rankdir, spread) {
    const g = new dagre.graphlib.Graph({ multigraph: false });
    g.setGraph({
      rankdir,
      nodesep: 9 * spread,
      ranksep: rankdir === 'LR' ? 46 : 34,
      marginx: 4,
      marginy: 4,
      ranker: 'network-simplex',
    });
    g.setDefaultEdgeLabel(() => ({}));
    for (const n of linked) {
      const s = weight(n.id);
      g.setNode(n.id, { width: s, height: s, size: s });
    }
    for (const e of links) if (g.hasNode(e.from) && g.hasNode(e.to)) g.setEdge(e.from, e.to);
    dagre.layout(g);
    const m = g.graph() || {};
    const w = Number.isFinite(m.width) && m.width > 1 ? m.width : 120;
    const h = Number.isFinite(m.height) && m.height > 1 ? m.height : 80;
    return { rankdir, spread, g, w, h, aspect: w / h };
  }
  const cands = [];
  for (const rd of ['LR', 'TB']) for (const sp of RANK_SPREAD) cands.push(build(rd, sp));

  let forceCache = null;
  const force = () => {
    if (!forceCache) forceCache = forcePositions(linkedIds, links);
    return forceCache;
  };

  /* Geometry for one candidate, including the cross-axis stretch. `fill` is
     the fraction of the tile the drawing would occupy on its *worse* axis:
     1.0 means it lands edge to edge in both directions. */
  function geo(i) {
    const pw = Math.max(40, plot.clientWidth || host.clientWidth || 400);
    const ph = Math.max(40, plot.clientHeight || host.clientHeight || 300);
    const c = cands[i];
    const sc0 = Math.min(pw / c.w, ph / c.h);
    const slack = c.rankdir === 'LR' ? ph / (c.h * sc0) : pw / (c.w * sc0);
    const k = clamp(slack, 1, MAX_STRETCH);
    const sx = c.rankdir === 'LR' ? 1 : k;
    const sy = c.rankdir === 'LR' ? k : 1;
    const W = c.w * sx;
    const H = c.h * sy;
    const sc = Math.min(pw / W, ph / H);
    return { c, pw, ph, sx, sy, W, H, sc, fill: Math.min((W * sc) / pw, (H * sc) / ph) };
  }
  function bestIndex() {
    let best = 0, bf = -1;
    cands.forEach((c, i) => {
      const g = geo(i);
      // fill decides; a mild nudge keeps left-to-right reading on ties
      const score = g.fill - (c.rankdir === 'LR' ? 0 : 0.01);
      if (score > bf) { bf = score; best = i; }
    });
    return best;
  }

  /* ── state ─────────────────────────────────────────────────────────────── */
  const state = { mode: 'auto' }; // 'auto' | 'layered' | 'force'
  let selectedId = selected ? String(selected) : null;
  let hoverId = null;
  let svg = null;
  let edgeEls = new Map();
  let nodeEls = new Map();
  let paintedMode = null;
  let lastKey = '';

  /* ── shell ─────────────────────────────────────────────────────────────── */
  host.innerHTML = '';
  const wrap = document.createElement('div');
  wrap.className = 'gw';
  const plot = document.createElement('div');
  plot.className = 'g-plot';
  wrap.append(plot);

  const legend = document.createElement('div');
  legend.className = 'g-legend';
  const kinds = [...new Set(norm.map((n) => n.kind).filter(Boolean))];
  legend.innerHTML = kinds
    .map((k) => `<span class="g-key" style="--kc:${kindColor(k)}">${k}</span>`)
    .join('');

  const seg = document.createElement('span');
  seg.className = 'g-seg';
  const mkBtn = (m, label, title) => {
    const b = document.createElement('button');
    b.className = 'g-mbtn';
    b.dataset.mode = m;
    b.textContent = label;
    b.title = title;
    b.addEventListener('click', () => {
      state.mode = m;
      repaint(true);
    });
    seg.append(b);
  };
  mkBtn('layered', 'layered', 'ranked dependency layers');
  mkBtn('force', 'force', 'spring layout — fills the tile, shows clusters');
  legend.append(seg);

  const hover = document.createElement('span');
  hover.className = 'g-hl';
  legend.append(hover);

  const meta = document.createElement('span');
  meta.className = 'g-meta';
  legend.append(meta);
  /* the footer is one line in any pane: keep a short form for narrow tiles */
  let metaL = { full: '', short: '' };
  let density = 0;
  function setMeta(full, short) { metaL = { full, short }; applyMeta(); }
  function applyMeta() {
    meta.textContent = density >= 1 ? metaL.short : metaL.full;
    meta.title = metaL.full;
  }

  if (loose.length) {
    const toggle = document.createElement('button');
    toggle.className = 'g-toggle';
    toggle.textContent = `+${loose.length} loose`;
    toggle.title = 'isolated nodes carry no dependency edge';
    const strip = document.createElement('div');
    strip.className = 'g-loose-row';
    strip.innerHTML = loose
      .map((n) => `<i style="--kc:${kindColor(n.kind)}" title="${n.label.replace(/"/g, '')}"></i>`)
      .join('');
    toggle.addEventListener('click', () => {
      const on = !strip.classList.contains('on');
      strip.classList.toggle('on', on);
      toggle.classList.toggle('on', on);
    });
    legend.insertBefore(toggle, meta);
    wrap.append(strip);
  }
  wrap.append(legend);
  host.append(wrap);

  /* ── scale compensation ─────────────────────────────────────────────────
     The world is CSS-scaled by the camera and publishes that zoom as --z, so a
     dot or a name sized in drawing units would shrink with the view. Sizes are
     therefore asked for in *screen* pixels and converted back through u (units
     per screen pixel), which keeps a label readable at 60% and at 200%.      */
  function zoomNow() {
    // the camera writes a translate+scale matrix on an ancestor, so the nearest
    // transform is the world zoom — no coupling to the engine, no polling
    for (let e = host; e && e !== document.body; e = e.parentElement) {
      const tr = getComputedStyle(e).transform;
      if (!tr || tr === 'none') continue;
      const m = tr.match(/matrix\(([^)]+)\)/);
      if (!m) continue;
      const parts = m[1].split(',').map(Number);
      const a = parts[0], d = parts[3];
      // panes carry their own hover translate; only an honest scale counts
      if (!Number.isFinite(a) || !Number.isFinite(d) || a <= 0.02) continue;
      if (Math.abs(a - 1) > 1e-3 || Math.abs(d - 1) > 1e-3) return a;
    }
    // no transform yet (first paint): fall back to the laid-out/drawn ratio
    const laid = host.offsetWidth || 0;
    const drawn = host.getBoundingClientRect().width;
    const ratio = laid > 8 && drawn > 0 ? drawn / laid : 1;
    return Number.isFinite(ratio) && ratio > 0.02 ? ratio : 1;
  }
  // softened on purpose: a zoomed-out overview earns slightly bigger names,
  // not a 1:1 copy of a zoomed-in one
  const labelScreenPx = (z) => 9.5 * clamp(Math.sqrt(z), 0.85, 1.4);
  const nodeScreenPx = (n) => clamp(3.4 + Math.sqrt(deg.get(n.id) || 1) * 1.5, 3.4, 10);

  /* ── drawing helpers ───────────────────────────────────────────────────── */
  function nodeElFor(n, x, y, r, fs, W, side = 1) {
    const hub = hubs.has(n.id);
    const c = el('circle', {
      cx: x, cy: y, r,
      class: 'g-node' + (hub ? ' hub' : ''),
      fill: kindColor(n.kind),
      'data-id': n.id,
      'vector-effect': 'non-scaling-stroke',
    });
    const tip = el('title');
    tip.textContent = `${n.label} · ${n.kind || 'node'} · ${deg.get(n.id) || 0} links`;
    c.append(tip);
    // side 0 means "draw the dot only": the label budget is spent
    if (hub && side !== 0) {
      const text = n.label.length > 22 ? n.label.slice(0, 21) + '…' : n.label;
      const pad = r + 6;
      const t = el('text', {
        x: side > 0 ? x + pad : x - pad,
        y,
        class: 'g-label',
        'font-size': fs.toFixed(2),
        'text-anchor': side > 0 ? 'start' : 'end',
        style: `--kc:${kindColor(n.kind)}`,
      });
      t.textContent = text;
      return { c, label: t };
    }
    return { c, label: null };
  }

  function edgeElFor(from, to, d) {
    const path = el('path', {
      d,
      class: 'g-edge',
      fill: 'none',
      'vector-effect': 'non-scaling-stroke',
      stroke: kindColor(byId.get(to)?.kind),
      'data-from': from,
      'data-to': to,
    });
    return path;
  }

  function commit(s) {
    if (svg) svg.remove();
    plot.prepend(s);
    svg = s;
    for (const [id, c] of nodeEls) {
      c.addEventListener('pointerenter', () => { hoverId = id; highlight(null); });
      c.addEventListener('pointerleave', () => { hoverId = null; highlight(null); });
      c.addEventListener('click', (ev) => {
        ev.stopPropagation();
        setSelected(id);
        onPick && onPick(byId.get(id));
      });
    }
    highlight(null);
  }

  /* Labels are ranked by degree and the ones that would collide are dropped:
     a graph reads better with five names than with fifteen overlapping ones. */
  function labelClash(taken, box) {
    return taken.some((o) => box[0] < o[0] + o[2] && box[0] + box[2] > o[0]
      && box[1] < o[1] + o[3] && box[1] + box[3] > o[1]);
  }

  /* One placement pass for both painters: positions arrive in drawing units,
     radii and label sizes arrive in screen pixels, and labels that would land
     on one already placed are dropped (highest degree wins the space). */
  function placeNodes(gNodes, gLabels, at, rPxOf, fsPx, W, u) {
    nodeEls = new Map();
    const taken = [];
    const fs = fsPx * u;
    // a small tile earns fewer names: the five biggest hubs in a big pane, one
    // or two in a postage stamp. Measured in *screen* area, because the world
    // camera shrinks what the reader actually gets.
    const zNow = zoomNow();
    const screenArea = host.clientWidth * host.clientHeight * zNow * zNow;
    let budget = clamp(Math.round(screenArea / 30000), 1, 5);
    const spots = linked.map((n) => ({ n, q: at(n) })).filter((e) => e.q)
      .sort((a, b) => (deg.get(b.n.id) || 0) - (deg.get(a.n.id) || 0));
    for (const { n, q } of spots) {
      const r = rPxOf(n) * u;
      const x = q.x, y = q.y;
      // try the roomy side first, then the other one, then give the name up
      let placed = null;
      for (const side of (hubs.has(n.id) && budget > 0 ? (x + fs * 20 > W ? [-1, 1] : [1, -1]) : [])) {
        const out = nodeElFor(n, x, y, r, fs, W, side);
        const tw = out.label.textContent.length * fs * 0.58 + 8 * u;
        const pad = r + 6 * u;
        const bx = side > 0 ? x + pad : x - pad - tw;
        // a full line of vertical clearance: ranks sit close together once a
        // wide drawing is fitted into the tile
        const box = [bx, y - fs, tw, fs * 2];
        // a name printed on top of another dot — or another name — is unreadable
        const overNode = spots.some((o) => o.q !== q
          && o.q.x > box[0] - u && o.q.x < box[0] + box[2] + u
          && Math.abs(o.q.y - y) < fs * 0.62);
        if (overNode || labelClash(taken, box)) continue;
        taken.push(box);
        placed = out;
        budget -= 1;
        break;
      }
      const out = placed || nodeElFor(n, x, y, r, fs, W, 0);
      gNodes.append(out.c);
      if (out.label) gLabels.append(out.label);
      nodeEls.set(n.id, out.c);
    }
  }

  /* ── layered paint ─────────────────────────────────────────────────────── */
  function paintLayered(i) {
    const { c, sx, sy, W, H, sc } = geo(i);
    const { g } = c;
    const mx = c.w / 2, my = c.h / 2;
    const X = (x) => mx + (x - mx) * sx;
    const Y = (y) => my + (y - my) * sy;

    const s = el('svg', { viewBox: `0 0 ${W} ${H}`, preserveAspectRatio: 'xMidYMid meet', class: 'ggraph' });
    const gEdges = el('g', { class: 'g-edges' });
    const gNodes = el('g', { class: 'g-nodes' });
    const gLabels = el('g', { class: 'g-labels' });
    s.append(gEdges, gNodes, gLabels);

    edgeEls = new Map();
    for (const l of links) {
      const e = g.edge(l.from, l.to);
      const pts = (e && e.points) || [];
      if (!pts.length) continue;
      const d = pts.reduce((a, p, k2) => a + (k2 ? ' L' : 'M') + X(p.x).toFixed(1) + ' ' + Y(p.y).toFixed(1), '');
      const path = edgeElFor(l.from, l.to, d);
      gEdges.append(path);
      edgeEls.set(l.from + '>' + l.to, path);
    }

    const z = zoomNow();
    const u = 1 / (sc * z || 1);
    // dagre sized the nodes in drawing units; keep that, but never below a
    // legible dot once the whole wide drawing is fitted
    placeNodes(gNodes, gLabels,
      (n) => { const p = g.node(n.id); return p ? { x: X(p.x), y: Y(p.y) } : null; },
      (n) => { const p = g.node(n.id); return Math.max(((p && p.width) || 15) / 2 * (sc * z), 2.2); },
      labelScreenPx(z), W, u);
    commit(s);
    paintedMode = 'layered';
    setMeta(`${linked.length} linked · ${links.length} edges`, `${linked.length}·${links.length}`);
  }

  /* ── force paint ───────────────────────────────────────────────────────── */
  /* tanh squashes the long tail of a spring layout into the frame without
     clipping it: points inside the 85% core stay put, runaways compress. */
  const squash = (v) => Math.tanh(v * 1.25);
  const MAX_ANISO = 1.35; // a blob stays round: never stretch the picture more

  function forceFrame(pw, ph, pos) {
    // side padding leaves room for hub labels; the top/bottom needs less
    const padX = clamp(pw * 0.16, 24, 86);
    const padY = clamp(ph * 0.09, 16, 40);
    const aw = Math.max(20, pw - padX * 2);
    const ah = Math.max(20, ph - padY * 2);
    let ax = 1e-4, ay = 1e-4;
    for (const p of pos.values()) {
      ax = Math.max(ax, Math.abs(squash(p.nx)));
      ay = Math.max(ay, Math.abs(squash(p.ny)));
    }
    const scale = Math.min(aw / (2 * ax), ah / (2 * ay));
    // whichever axis is under-filled may stretch a little to use the tile
    const fx = Math.min(MAX_ANISO, aw / (2 * ax * scale));
    const fy = Math.min(MAX_ANISO, ah / (2 * ay * scale));
    return { cx: pw / 2, cy: ph / 2, sx: scale * fx, sy: scale * fy };
  }

  function paintForce() {
    const pw = Math.max(40, plot.clientWidth || host.clientWidth || 400);
    const ph = Math.max(40, plot.clientHeight || host.clientHeight || 300);
    const pos = force();
    const fr = forceFrame(pw, ph, pos);
    const X = (n) => fr.cx + squash(n) * fr.sx;
    const Y = (n) => fr.cy + squash(n) * fr.sy;

    const s = el('svg', { viewBox: `0 0 ${pw} ${ph}`, preserveAspectRatio: 'xMidYMid meet', class: 'ggraph force' });
    const gEdges = el('g', { class: 'g-edges' });
    const gNodes = el('g', { class: 'g-nodes' });
    const gLabels = el('g', { class: 'g-labels' });
    s.append(gEdges, gNodes, gLabels);

    edgeEls = new Map();
    for (const l of links) {
      const a = pos.get(l.from), b = pos.get(l.to);
      if (!a || !b) continue;
      const x1 = X(a.nx), y1 = Y(a.ny), x2 = X(b.nx), y2 = Y(b.ny);
      // a slight bow keeps parallel edges readable
      const cx = (x1 + x2) / 2 + (y2 - y1) * 0.08;
      const cy = (y1 + y2) / 2 - (x2 - x1) * 0.08;
      const path = edgeElFor(l.from, l.to, `M${x1.toFixed(1)} ${y1.toFixed(1)} Q${cx.toFixed(1)} ${cy.toFixed(1)} ${x2.toFixed(1)} ${y2.toFixed(1)}`);
      gEdges.append(path);
      edgeEls.set(l.from + '>' + l.to, path);
    }

    const z = zoomNow();
    placeNodes(gNodes, gLabels,
      (n) => { const p = pos.get(n.id); return p ? { x: X(p.nx), y: Y(p.ny) } : null; },
      nodeScreenPx, labelScreenPx(z), pw, 1 / z);
    commit(s);
    paintedMode = 'force';
    setMeta(`${linked.length} linked · ${links.length} edges · spring`, `${linked.length}·${links.length}`);
  }

  /* ── highlight / selection ─────────────────────────────────────────────── */
  function highlight(id) {
    const focus = id || hoverId || selectedId;
    const near = focus ? neighbours.get(focus) || new Set() : null;
    if (svg) {
      svg.classList.toggle('dim', !!focus);
      svg.classList.toggle('hover', !!hoverId);
    }
    for (const [eid, path] of edgeEls) {
      const [a, b] = eid.split('>');
      path.classList.toggle('lit', !!focus && (a === focus || b === focus));
    }
    for (const [nid, c] of nodeEls) {
      const ok = !focus || nid === focus || (near && near.has(nid));
      c.classList.toggle('near', !!focus && ok);
      c.classList.toggle('self', nid === focus);
    }
    const shown = hoverId ? byId.get(hoverId) : null;
    hover.textContent = shown ? `${shown.label} · ${deg.get(shown.id) || 0} links` : '';
    hover.dataset.empty = shown ? '' : '1';
  }

  function setSelected(id) {
    selectedId = id ? String(id) : null;
    highlight(null);
    host.dispatchEvent(new CustomEvent('graph:select', { detail: selectedId, bubbles: true }));
  }

  /* ── paint dispatch ────────────────────────────────────────────────────── */
  function repaint(forceDraw) {
    const i = bestIndex();
    const auto = geo(i).fill < MIN_FILL ? 'force' : 'layered';
    const mode = state.mode === 'auto' ? auto : state.mode;
    const z = zoomNow();
    const key = [mode, i, Math.round(host.clientWidth / 8), Math.round(host.clientHeight / 8),
      Math.round(z * 25)].join(':')
    if (!forceDraw && key === lastKey) return;
    lastKey = key;
    host.dataset.layout = mode;
    host.dataset.dbg = JSON.stringify({
      fill: +geo(i).fill.toFixed(2), i,
      aspects: cands.map((c) => +c.aspect.toFixed(1)),
      boxes: cands.map((c, k) => [+c.w.toFixed(0), +c.h.toFixed(0)]),
    });
    for (const b of seg.children) b.classList.toggle('on', b.dataset.mode === mode);
    fitLegend();
    if (mode === 'force') paintForce();
    else paintLayered(i);
    fitLegend();
  }

  /* The footer has to stay on one line at any pane width, so it sheds weight in
     steps — short counts, then the species keys, then the counts — until what it
     needs fits what it has. A fixed width threshold would be wrong: the pane
     lives inside a scaled world, and `scrollWidth` on the row lies while a child
     clips its own overflow, so each child reports its own natural width. */
  const gapPx = () => parseFloat(getComputedStyle(legend).columnGap || '0') || 0;
  function legendNeed() {
    // the hover read-out grows to fill the row and truncates, so it never limits
    const kids = [...legend.children].filter((c) =>
      getComputedStyle(c).display !== 'none' && !c.classList.contains('g-hl'));
    const sum = kids.reduce((a, c) => a + Math.max(c.scrollWidth, c.offsetWidth), 0);
    return sum + gapPx() * Math.max(0, kids.length - 1);
  }
  function fitLegend() {
    for (density = 0; density <= 3; density++) {
      wrap.dataset.density = String(density);
      applyMeta();
      if (legendNeed() <= legend.clientWidth + 1) return;
    }
    density = 3;
    wrap.dataset.density = '3';
    applyMeta();
  }

  repaint(true);

  /* A camera zoom changes every drawing unit's on-screen size without resizing
     the pane, so the obvious gestures re-check the zoom once they settle. */
  let lastZoom = zoomNow();
  let zoomTimer = 0;
  const syncZoom = () => {
    if (!host.isConnected) return;
    const z = zoomNow();
    if (Math.abs(z - lastZoom) > 0.015) { lastZoom = z; repaint(true); }
  };
  const onGesture = () => { clearTimeout(zoomTimer); zoomTimer = setTimeout(syncZoom, 110); };
  for (const ev of ['wheel', 'keydown', 'pointerup', 'dblclick']) {
    window.addEventListener(ev, onGesture, { passive: true });
  }
  setInterval(syncZoom, 1500); // camera moves can also come from code (fit, presets)

  if (typeof ResizeObserver === 'function') {
    let t = 0;
    const ro = new ResizeObserver(() => {
      clearTimeout(t);
      t = setTimeout(() => {
        if (!host.isConnected) { ro.disconnect(); return; }
        repaint(false);
      }, 120);
    });
    ro.observe(host);
  }

  return { setSelected, highlight, repaint, mode: () => paintedMode };
}