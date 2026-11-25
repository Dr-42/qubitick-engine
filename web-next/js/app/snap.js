/*
 * snap.js — where a dragged or resized tile is allowed to land.
 *
 * Extracted from the inline script in web-next/index.html. This file is arithmetic:
 * given the tile, the rects it may align to, and the tolerances, it answers where the
 * tile goes and which lines to draw. It reads no DOM and holds no state, so the page
 * keeps the job it is good at (collecting the other rects from the live panes and
 * turning guides into elements) and a node test can walk the rules directly.
 *
 * THE RULE THAT MATTERS. A candidate is an edge of another tile plus the position that
 * would put this tile's own edge on it. Taken alone that has no sense of place: the
 * candidate set is every other tile on the canvas, so a tile dragged into empty space
 * takes the edge of a tile thousands of pixels away and the guide is drawn from one to
 * the other. So an edge only offers a snap if the two tiles are within a band along the
 * OTHER axis (`snapBand`), which also bounds how long a guide can be. The band is a
 * world distance on purpose — not divided by the zoom — so the same two tiles attract
 * each other the same way at every scale.
 *
 * AND THE ONE THAT IS SUBTLE. Two candidates can be within tolerance of the same
 * pointer: a flush edge and the same edge one gutter further out. "Nearest wins" makes
 * the tile flip between touching and 2px short as the pointer crosses the midpoint, so
 * a gutter candidate carries a `bias` and loses any tie against the plainer one: within
 * tolerance the flush position always wins, and a gutter only catches a tile that is
 * already past contact.
 */

/** The defaults, and the single place they are written down. */
export const SNAP = { on: true, tolPx: 7, grid: 20, gutter: 2, bandPx: 240 };

/** How far apart two tiles may be along an edge and still attract each other: the
 *  dragged tile's own extent (so a guide is never much longer than the thing being
 *  dragged), with a floor for small tiles. */
export const snapBand = (extent) => Math.max(SNAP.bandPx, extent * 1.5);

/** The gap between two spans along one axis: 0 when they touch or overlap. */
export const spanGap = (a0, a1, b0, b1) => Math.max(0, Math.max(a0, b0) - Math.min(a1, b1));

/** Every position for `x` at which this tile's left or right edge meets an edge of
 *  `r`, with the line each would draw and the bias that makes a gutter lose a tie. */
function across(r, w, gutter) {
  return [
    { pos: r.x,           line: r.x },                     // left edges together
    { pos: r.x + r.w,     line: r.x + r.w },               // right of it
    { pos: r.x + r.w + gutter, line: r.x + r.w, bias: gutter },  // right of it, one gutter
    { pos: r.x - w,       line: r.x },                     // left of it
    { pos: r.x - w - gutter, line: r.x, bias: gutter },    // left of it, one gutter
    { pos: r.x + r.w - w, line: r.x + r.w },               // right edges together
  ];
}

/** The same, down `y`. */
function down(r, h, gutter) {
  return [
    { pos: r.y,           line: r.y },
    { pos: r.y + r.h,     line: r.y + r.h },
    { pos: r.y + r.h + gutter, line: r.y + r.h, bias: gutter },
    { pos: r.y - h,       line: r.y },
    { pos: r.y - h - gutter, line: r.y, bias: gutter },
    { pos: r.y + r.h - h, line: r.y + r.h },
  ];
}

/** Walk a list of candidates and keep the one with the lowest biased distance. */
function nearest(cands, value, tol) {
  let best = null;
  for (const c of cands) {
    const d = Math.abs(c.pos - value);
    const score = d + (c.bias || 0);
    if (d <= tol && (!best || score < best.d)) best = { ...c, d: score };
  }
  return best;
}

/**
 * Where a dragged tile lands, and the lines to draw.
 *
 * `others` are plain rects ({x,y,w,h}) — the page collects them from the live panes and
 * the cluster frames; `tol` is in the same units as the rects (world px), which is why
 * the page divides its screen tolerance by the zoom before calling.
 * Returns {x, y, snapped, guides}; a guide is {dir:'v'|'h', at, from, to} in world px.
 */
export function snapMove({ x, y, w, h, others = [], tol, gutter = SNAP.gutter, grid = 0, band }) {
  const bandX = band != null ? band : snapBand(w);
  const bandY = band != null ? band : snapBand(h);
  let bx = null, by = null;
  for (const r of others) {
    /* the x-candidates are only offered by a neighbour this tile can see along Y */
    if (spanGap(y, y + h, r.y, r.y + r.h) <= bandY) {
      const hit = nearest(across(r, w, gutter), x, tol);
      if (hit && (!bx || hit.d < bx.d)) bx = { ...hit, r };
    }
    /* ...and the y-candidates by a neighbour it can see along X */
    if (spanGap(x, x + w, r.x, r.x + r.w) <= bandX) {
      const hit = nearest(down(r, h, gutter), y, tol);
      if (hit && (!by || hit.d < by.d)) by = { ...hit, r };
    }
  }

  let nx = x, ny = y;
  if (bx) nx = bx.pos; else if (grid) nx = Math.round(x / grid) * grid;
  if (by) ny = by.pos; else if (grid) ny = Math.round(y / grid) * grid;

  const guides = [];
  if (bx) guides.push({ dir: 'v', at: bx.line,
                        from: Math.min(ny, bx.r.y), to: Math.max(ny + h, bx.r.y + bx.r.h) });
  if (by) guides.push({ dir: 'h', at: by.line,
                        from: Math.min(nx, by.r.x), to: Math.max(nx + w, by.r.x + by.r.w) });
  /* a grid snap is a snap too: a caller asking "did it land on something?" means the
     tile either, not only the edges */
  return { x: nx, y: ny, snapped: !!(bx || by) || !!(grid && (nx !== x || ny !== y)), guides };
}

/**
 * Snap the edge a resize is moving onto a neighbour's edge.
 *
 * `rect` is the proposed rect, already clamped to the minimum; only the moving edge is
 * changed, so the far side stays put. Returns {rect, guides} with a new rect — the
 * argument is not modified.
 */
export function snapEdge({ rect, edge, others = [], tol, grid = 0, band }) {
  const r = { ...rect };
  const growX = edge.includes('e'), growY = edge.includes('s');
  const bandX = band != null ? band : snapBand(r.w);
  const bandY = band != null ? band : snapBand(r.h);
  const xs = [], ys = [];
  /* the same gate a dragged tile gets: an edge is a candidate only if this tile can see
     the neighbour along that edge */
  for (const o of others) {
    if (spanGap(r.y, r.y + r.h, o.y, o.y + o.h) <= bandY) xs.push(o.x, o.x + o.w);
    if (spanGap(r.x, r.x + r.w, o.x, o.x + o.w) <= bandX) ys.push(o.y, o.y + o.h);
  }
  const near = (list, v) => {
    let best = null;
    for (const at of list) { const d = Math.abs(at - v); if (d <= tol && (!best || d < best.d)) best = { at, d }; }
    return best;
  };
  const guides = [];

  if (edge.includes('e') || edge.includes('w')) {
    const hit = near(xs, growX ? r.x + r.w : r.x);
    if (hit) {
      if (growX) r.w = hit.at - r.x;
      else { r.w = r.x + r.w - hit.at; r.x = hit.at; }
      guides.push({ dir: 'v', at: hit.at, from: r.y, to: r.y + r.h });
    } else if (grid) {
      if (growX) r.w = Math.round((r.x + r.w) / grid) * grid - r.x;
      else { const nx = Math.round(r.x / grid) * grid; r.w = r.x + r.w - nx; r.x = nx; }
    }
  }
  if (edge.includes('s') || edge.includes('n')) {
    const hit = near(ys, growY ? r.y + r.h : r.y);
    if (hit) {
      if (growY) r.h = hit.at - r.y;
      else { r.h = r.y + r.h - hit.at; r.y = hit.at; }
      guides.push({ dir: 'h', at: hit.at, from: r.x, to: r.x + r.w });
    } else if (grid) {
      if (growY) r.h = Math.round((r.y + r.h) / grid) * grid - r.y;
      else { const ny = Math.round(r.y / grid) * grid; r.h = r.y + r.h - ny; r.y = ny; }
    }
  }
  return { rect: r, guides };
}