/**
 * live/wire/layer.js — the wire layer: one canvas, screen space.
 *
 * Wires are drawn, not laid out. Ports live in world coordinates (they belong
 * to panes), but every wire is projected to the screen and stroked there, so a
 * 1 px wire stays 1 px at 8% zoom and no SVG tree has to be rebuilt when the
 * camera moves. That is also what makes the per-kind dashes and the flow
 * particles affordable: it is one path per wire, once per frame, no DOM.
 *
 * The layer knows nothing about panes, layouts or the store. It is handed:
 *
 *   portWorld(pid, portId) -> { x, y } | null   world coords of a port
 *   camera()               -> { x, y, z }       the world -> screen transform
 *   rootRect(rootId)       -> { x, y, w, h }    only needed for the anchor overlay
 *
 * and a list of wires in the shape the store persists:
 *
 *   { id, from:{p,port}, to:{p,port}, kind, type, dead, warn }
 *
 * Port kind decides the line: value solid, event marching dashes, call
 * dash-dot. Illegal (cycled) wires are red and dashed; a selected wire is
 * brighter and thicker. Value is not decorative here — the dash is how you
 * read a graph without clicking anything.
 */

const KIND_DASH = {
  value: [],
  event: [7, 6],
  call: [12, 4, 2, 4],
};

/** cubic bezier with horizontal handles — the shape every patcher uses */
export function bez(a, b, c, d, t) {
  const u = 1 - t;
  return u * u * u * a + 3 * u * u * t * b + 3 * u * t * t * c + t * t * t * d;
}

/** Concrete colours for a canvas: custom-property tokens are resolved once. */
function makePalette() {
  let cache = null;
  function resolve() {
    if (typeof document === 'undefined') return { bad: '#ff5c74', dim: '#888', ink: '#eee' };
    const probe = document.createElement('span');
    probe.style.cssText = 'position:absolute;width:0;height:0;visibility:hidden';
    document.body.append(probe);
    const read = (token) => {
      probe.style.color = `var(${token}, #888)`;
      return getComputedStyle(probe).color;
    };
    const out = {
      cyan: read('--cyan'),
      green: read('--green'),
      amber: read('--amber'),
      violet: read('--violet'),
      bad: read('--bad'),
      t2: read('--t2'),
      ink: read('--ink'),
      panel: read('--panel'),
    };
    probe.remove();
    return out;
  }
  return {
    get() {
      if (!cache) cache = resolve();
      return cache;
    },
    invalidate() { cache = null; },
  };
}

export function createWireLayer({ canvas, portWorld, camera, rootRect, opts = {} }) {
  if (!canvas) throw new Error('wire layer needs a canvas');
  if (typeof portWorld !== 'function') throw new Error('wire layer needs portWorld()');
  if (typeof camera !== 'function') throw new Error('wire layer needs camera()');

  const palette = makePalette();
  const curve = opts.curve ?? 0.55;      // handle length as a fraction of |dx|
  const minHandle = opts.minHandle ?? 46;
  const glow = opts.glow ?? 9;
  const labelZoom = opts.labelZoom ?? 0.62;
  const cullPad = 80;

  const ctx = canvas.getContext('2d');
  let dpr = Math.min(opts.maxDpr ?? 2, (typeof devicePixelRatio === 'number' ? devicePixelRatio : 1));
  let W = 1, H = 1;

  function resize() {
    const w = Math.max(1, Math.round(canvas.clientWidth || canvas.parentElement?.clientWidth || 1));
    const h = Math.max(1, Math.round(canvas.clientHeight || canvas.parentElement?.clientHeight || 1));
    const next = Math.min(opts.maxDpr ?? 2, (typeof devicePixelRatio === 'number' ? devicePixelRatio : 1));
    if (w === W && h === H && next === dpr) return false;
    W = w; H = h; dpr = next;
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
    return true;
  }

  const w2s = (x, y) => {
    const c = camera();
    return { x: x * c.z + c.x, y: y * c.z + c.y };
  };

  /**
   * Handles for a pair of endpoints. Both ends are pinned to the pane that owns
   * them — a source leaves to the right, a sink arrives from the left — so the
   * handles run along each port's own outward normal, never "toward the other
   * dot". That is what keeps a wire drawn from a right-hand port readable even
   * when its target sits *behind* it: the curve loops out and comes back
   * instead of leaving through its own pane.
   *
   * A near-vertical pair is the one case where the normals are a detour, so the
   * handles turn vertical and the wire becomes a clean S down the gutter.
   */
  function handles(A, B) {
    const dx = B.x - A.x;
    const dy = B.y - A.y;
    if (Math.abs(dy) > Math.abs(dx) * 1.6) {
      const v = Math.max(minHandle, Math.abs(dy) * curve);
      const sy = dy === 0 ? 1 : Math.sign(dy);
      return [{ x: A.x, y: A.y + sy * v }, { x: B.x, y: B.y - sy * v }];
    }
    if (dx < 0) {
      // The target sits behind its source. Two horizontal handles would have to
      // overshoot the target and hook back through it; a vertical first handle
      // instead drops the wire into the gutter, and the arrival handle is short
      // enough that the hook it needs stays next to the port.
      const down = dy >= 0 ? 1 : -1;
      const v = Math.max(minHandle, Math.min(Math.abs(dy) * 0.6, 150)) * down;
      const h = Math.max(minHandle, Math.min(Math.abs(dx) * 0.25, 90));
      return [{ x: A.x, y: A.y + v }, { x: B.x - h, y: B.y }];
    }
    const h = Math.max(minHandle, Math.abs(dx) * curve);
    return [{ x: A.x + h, y: A.y }, { x: B.x - h, y: B.y }];
  }

  /** Screen-space geometry of one wire, or null when a port is missing. */
  function geometry(wire) {
    if (!wire || !wire.from || !wire.to) return null;
    const a = portWorld(wire.from.p, wire.from.port);
    const b = portWorld(wire.to.p, wire.to.port);
    if (!a || !b) return null;
    const A = w2s(a.x, a.y);
    const B = w2s(b.x, b.y);
    const [c1, c2] = handles(A, B);
    return { A, B, c1, c2 };
  }

  const pointAt = (g, t) => ({
    x: bez(g.A.x, g.c1.x, g.c2.x, g.B.x, t),
    y: bez(g.A.y, g.c1.y, g.c2.y, g.B.y, t),
  });

  function wireColor(wire, pal) {
    if (wire.dead) return pal.dim;
    if (wire.warn) return pal.amber;
    if (wire.kind === 'call') return pal.violet;
    if (wire.kind === 'event') return pal.amber;
    return pal.cyan;
  }

  /**
   * Draw every wire.
   *   t        seconds, drives the marching dashes and the flow particles
   *   dim      multiplier for work mode (wires recede) — 1 keeps them normal
   *   flow     >0 animates particles along live value/event wires
   *   selected / illegal   ids, as Set/array
   */
  function draw(wires, t = 0, view = {}) {
    resize();
    const pal = palette.get();
    const c = camera();
    const dim = view.dim ?? 1;
    const flow = view.flow ?? 1;
    const sel = view.selected;
    const isSel = (id) => (sel instanceof Set ? sel.has(id) : sel === id);
    const illegal = view.illegal instanceof Set ? view.illegal : new Set(view.illegal || []);

    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, W, H);

    for (const wire of wires || []) {
      if (wire.dead) continue;
      const g = geometry(wire);
      if (!g) continue;
      const { A, B, c1, c2 } = g;
      // off-screen in one direction: not worth a path
      if (A.x < -cullPad && B.x < -cullPad) continue;
      if (A.x > W + cullPad && B.x > W + cullPad) continue;
      if (A.y < -cullPad && B.y < -cullPad) continue;
      if (A.y > H + cullPad && B.y > H + cullPad) continue;

      const bad = illegal.has(wire.id);
      const selected = isSel(wire.id);
      const col = bad ? pal.bad : wireColor(wire, pal);

      ctx.strokeStyle = col;
      ctx.globalAlpha = (selected ? 1 : 0.8) * dim;
      ctx.lineWidth = Math.min(3.6, Math.max(0.9, 1.7 * c.z)) * (selected ? 1.7 : 1);
      ctx.shadowBlur = glow * (selected ? 1.6 : 1) * Math.min(1.3, Math.max(0.4, c.z + 0.35));
      ctx.shadowColor = col;

      if (bad) { ctx.setLineDash([5, 5]); ctx.lineDashOffset = 0; }
      else if (wire.kind === 'event') { ctx.setLineDash(KIND_DASH.event); ctx.lineDashOffset = -t * 46; }
      else if (wire.kind === 'call') { ctx.setLineDash(KIND_DASH.call); ctx.lineDashOffset = -t * 30; }
      else { ctx.setLineDash([]); ctx.lineDashOffset = 0; }

      ctx.beginPath();
      ctx.moveTo(A.x, A.y);
      ctx.bezierCurveTo(c1.x, c1.y, c2.x, c2.y, B.x, B.y);
      ctx.stroke();
      ctx.setLineDash([]);
      ctx.shadowBlur = 0;

      // a live value/event wire carries visible traffic
      if (flow > 0 && !bad && view.running !== false) {
        const n = wire.kind === 'event' ? 1 : 2;
        for (let i = 0; i < n; i++) {
          const tt = (t * 0.34 * flow + i / n) % 1;
          const { x, y } = pointAt(g, tt);
          ctx.globalAlpha = (0.35 + 0.65 * Math.sin(tt * Math.PI)) * dim;
          ctx.fillStyle = col;
          ctx.shadowBlur = 10;
          ctx.shadowColor = col;
          ctx.beginPath();
          ctx.arc(x, y, Math.min(3.4, Math.max(1.3, 2.4 * c.z)) * (1 + flow * 0.4), 0, 7);
          ctx.fill();
          ctx.shadowBlur = 0;
        }
      }

      // port names at the midpoint, only when they can be read
      if (view.labels !== false && c.z > labelZoom) {
        const mid = pointAt(g, 0.5);
        const mx = mid.x, my = mid.y;
        const label = `${wire.from.port} → ${wire.to.port}${wire.type ? ' :' + wire.type : ''}`;
        ctx.font = '9px ui-monospace, monospace';
        const tw = ctx.measureText(label).width;
        ctx.globalAlpha = 0.92 * dim;
        ctx.fillStyle = 'rgba(6,7,14,0.82)';
        ctx.fillRect(mx - tw / 2 - 4, my - 8, tw + 8, 14);
        ctx.globalAlpha = 0.35 * dim;
        ctx.strokeStyle = col;
        ctx.lineWidth = 1;
        ctx.strokeRect(mx - tw / 2 - 4, my - 8, tw + 8, 14);
        ctx.globalAlpha = dim;
        ctx.fillStyle = col;
        ctx.textAlign = 'center';
        ctx.fillText(label, mx, my + 2);
        ctx.textAlign = 'left';
      }

      // the wire being dragged from a port is drawn by connect.js
      ctx.globalAlpha = 1;
    }

    if (view.anchors && view.anchors.length && typeof rootRect === 'function') drawAnchors(view.anchors, pal, dim);
    ctx.globalAlpha = 1;
  }

  /** Dashed constraints between root centres (layout anchors). */
  function drawAnchors(anchors, pal, dim) {
    const show = pal.green;
    ctx.setLineDash([4, 5]);
    ctx.lineWidth = 1.2;
    ctx.strokeStyle = show;
    ctx.font = '8px ui-monospace, monospace';
    for (const an of anchors) {
      const ra = rootRect(an.a);
      const rb = rootRect(an.b);
      if (!ra || !rb) continue;
      const A = w2s(ra.x + ra.w / 2, ra.y + ra.h / 2);
      const B = w2s(rb.x + rb.w / 2, rb.y + rb.h / 2);
      ctx.globalAlpha = 0.5 * dim;
      ctx.beginPath();
      ctx.moveTo(A.x, A.y);
      ctx.lineTo(B.x, B.y);
      ctx.stroke();
      const mx = (A.x + B.x) / 2, my = (A.y + B.y) / 2;
      ctx.setLineDash([]);
      ctx.fillStyle = show;
      ctx.beginPath();
      ctx.arc(mx, my, 3, 0, 7);
      ctx.fill();
      if (an.edge) ctx.fillText(`${an.edge}${an.gap ?? ''}`, mx + 6, my - 4);
      ctx.setLineDash([4, 5]);
    }
    ctx.setLineDash([]);
  }

  /** Nearest wire under a screen point (bezier sampled 20×), or null. */
  function hitTest(wires, sx, sy, tolerance = 9) {
    let best = null;
    let bestD = tolerance;
    for (const wire of wires || []) {
      if (wire.dead) continue;
      const g = geometry(wire);
      if (!g) continue;
      for (let i = 0; i <= 20; i++) {
        const { x, y } = pointAt(g, i / 20);
        const d = Math.hypot(x - sx, y - sy);
        if (d < bestD) { bestD = d; best = wire; }
      }
    }
    return best;
  }

  /** The in-progress wire from a dragged port to the cursor. */
  function drawGhost(fromScreen, toScreen, { kind = 'value', valid = true, mirrored = false } = {}) {
    const pal = palette.get();
    const A = mirrored ? toScreen : fromScreen;
    const B = mirrored ? fromScreen : toScreen;
    const [c1, c2] = handles(A, B);
    ctx.globalAlpha = 0.95;
    ctx.strokeStyle = valid ? pal.cyan : pal.bad;
    ctx.lineWidth = 1.8;
    ctx.setLineDash(kind === 'event' ? KIND_DASH.event : []);
    ctx.shadowBlur = 12;
    ctx.shadowColor = ctx.strokeStyle;
    ctx.beginPath();
    ctx.moveTo(A.x, A.y);
    ctx.bezierCurveTo(c1.x, c1.y, c2.x, c2.y, B.x, B.y);
    ctx.stroke();
    ctx.setLineDash([]);
    ctx.shadowBlur = 0;
    ctx.globalAlpha = 1;
    ctx.fillStyle = valid ? pal.cyan : pal.bad;
    ctx.beginPath();
    ctx.arc(B.x, B.y, 3.4, 0, 7);
    ctx.fill();
  }

  function clear() {
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, W, H);
  }

  resize();
  return {
    canvas,
    toScreen: w2s,
    draw,
    drawGhost,
    clear,
    hitTest,
    geometry,
    resize,
    refreshPalette: () => palette.invalidate(),
    get size() { return { w: W, h: H, dpr }; },
  };
}