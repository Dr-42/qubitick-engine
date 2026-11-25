/**
 * live/wire/aim.js — where a wire gesture lands.
 *
 * `connect.js` owns the port dots and paints the ghost; this module answers the
 * two questions the gesture asks about them, and it asks nothing else:
 *
 *   sourceFor(panes, target, x, y)      a press on a NODE draws from that node
 *   nearestLegal(panes, from, …)        a release near a dot counts as on it
 *   legalTargets(panes, from, …)        every dot that would accept the wire
 *
 * A press anywhere in a pane's body means "draw a wire from this node", not
 * only on the 9 px dot that advertises it. Chrome that has its own pointer
 * meaning (the head that moves the tile, tabs, resize grips, sashes, root
 * handles, palette items) and hosts that own their own drag (form fields, a
 * canvas being drawn on, the editor's buffer) are never sources.
 *
 * The geometry is plain arithmetic over `getBoundingClientRect`, so the module
 * has no DOM of its own and no state: `panes` is the map `connect.js` keeps,
 * `paneId -> { el, dots: Map<portId, dotEl> }`, and every hit is measured from
 * a client point. That is what makes it testable without a browser.
 */

/** The element a press has to be inside to mean "this node is the source". */
export const NODE_SEL = '.pane[data-pane]';

/**
 * Inside a node, these are NOT sources: chrome with its own pointer meaning
 * first, then hosts whose own drag would fight this one. A `select` or an
 * `input` is a control to operate, and the editor's buffer is a place to
 * select text — a wire from those panes is drawn from the dot on the edge.
 */
export const NOT_A_SOURCE = [
  '.pane-head', '.qb-tab', '.grip', '.sash', '.root-handle', '.palette-item',
  'input', 'textarea', 'select', 'canvas', '[contenteditable=""]',
  '[contenteditable="true"]', '.cm-editor',
].join(', ');

/** Every port of one pane, as the same shape the drag carries. */
export function portsOf(panes, paneId, dir) {
  const entry = panes.get(paneId);
  if (!entry) return [];
  const list = [];
  for (const [port, el] of entry.dots) {
    const p = portOf(panes, paneId, port, el);
    if (!dir || p.dir === dir) list.push(p);
  }
  return list;
}

/** One dot as a port, including the element the ghost is measured from. */
export function portOf(panes, paneId, port, el) {
  const dot = el || panes.get(paneId)?.dots.get(port);
  return {
    pane: paneId,
    port,
    dir: dot?.dataset.dir,
    kind: dot?.dataset.kind,
    type: dot?.dataset.type,
    el: dot,
  };
}

/** The dot nearest a client point on one side of a pane, with its distance. */
export function nearestPort(panes, paneId, dir, x, y) {
  let best = null;
  for (const p of portsOf(panes, paneId, dir)) {
    const d = distTo(p.el, x, y);
    if (!best || d < best.d) best = { ...p, d };
  }
  return best;
}

/**
 * The source a press on `target` draws from, or null when this press is not a
 * wire. A node with no out port is not a source: there is nothing to draw.
 */
export function sourceFor(panes, target, x, y) {
  const host = target?.closest?.(NODE_SEL);
  if (!host || host.closest?.(NOT_A_SOURCE) || target.closest?.(NOT_A_SOURCE)) return null;
  return nearestPort(panes, host.dataset.pane, 'out', x, y);
}

/** Every dot that would accept a wire from `from`. */
export function legalTargets(panes, from, canConnect) {
  const els = new Set();
  for (const [paneId, entry] of panes) {
    for (const [port, el] of entry.dots) {
      if (el === from.el) continue;
      if (canConnect(from, portOf(panes, paneId, port, el)).ok) els.add(el);
    }
  }
  return els;
}

/**
 * The legal dot nearest a release point, within `px` screen px — a release near
 * a dot counts as a release on it. Aiming at a 9 px target under a camera is
 * the whole difficulty of drawing a wire, and the drop is the moment the
 * pointer is least likely to be exact.
 *
 * Only *legal* dots are considered: a release beside an incompatible port is a
 * refusal that must be reported, not a silent snap to something unrelated.
 * When nothing is near, the caller falls back to the dot under the cursor.
 */
export function nearestLegal(panes, from, x, y, { px = 0, canConnect } = {}) {
  let best = null;
  for (const [paneId, entry] of panes) {
    for (const [port, el] of entry.dots) {
      if (el === from.el) continue;
      const cand = portOf(panes, paneId, port, el);
      if (!canConnect(from, cand).ok) continue;
      const d = distTo(el, x, y);
      if (d <= px && (!best || d < best.d)) best = { ...cand, d };
    }
  }
  return best;
}

function distTo(el, x, y) {
  if (!el || !el.getBoundingClientRect) return Infinity;
  const r = el.getBoundingClientRect();
  return Math.hypot(r.left + r.width / 2 - x, r.top + r.height / 2 - y);
}
