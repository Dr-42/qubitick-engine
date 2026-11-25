/**
 * live/wire/connect.js — ports you can see and drag from.
 *
 * The layer draws wires; this module owns the *ends* of them. Ports are DOM
 * dots pinned to a pane's left/right edge, so they are laid out by the same CSS
 * that places the pane — no geometry to recompute when a sash moves, and hit
 * testing is the browser's job. Their on-screen size is counter-scaled against
 * the camera (`--z` on #world), so a port stays a 9 px dot at 40% zoom.
 *
 * A node with ports is a leaf pane; the dot order follows the kind descriptor:
 *
 *     ┌───────────────┐
 *   ●─┤ highlight     ├─● selection
 *     └───────────────┘
 *
 * The dots are *not* children of the pane. A pane clips its own content
 * (`overflow:hidden`, for the rounded corners), and the 8 px sashes sit
 * directly on top of the pane edges, so a dot parented to the pane would be
 * half clipped and half covered — a 1 px target. They live in one flat
 * container inside the world instead, laid over each pane's body box, so they
 * paint above the sashes and are never clipped.
 *
 * A wire is drawn by pressing on a NODE and dragging to the port it should
 * reach. The press takes the pane's nearest out port as the source — or the dot
 * itself, when the press lands on one — the travel has to pass a few pixels
 * first, because a node's body is also its content: below that distance the
 * press is the click the pane was expecting. Every dot that would accept the
 * wire lights up as the drag starts, and the release connects; a release near a
 * dot counts as a release on it, since 9 px under a camera is not a target to
 * be surgical about. If the direction and types agree, `onConnect` is called
 * with `{ from, to }` in the same shape the store persists. Refusals are shown,
 * not silent: the ghost turns red, and a wrong-type drop reports why.
 *
 *   const ports = createPorts({ worldEl, camera, layer, onConnect, onSelect });
 *   ports.sync();                        // after a layout change
 *   ports.setWiring(true);               // show every port at once (wire mode)
 */

import { getKind, typeOk } from './kinds.js';
import { sourceFor, legalTargets, nearestLegal } from './aim.js';

export function createPorts({
  worldEl,
  camera,
  layer,
  onConnect,
  onSelect,
  onRefuse,
  portKinds = ['value', 'event'],
} = {}) {
  if (!worldEl) throw new Error('ports need the world element');
  if (typeof camera !== 'function') throw new Error('ports need camera()');

  /** paneId -> { el, dots: Map<portId, el> } */
  const panes = new Map();
  let wiring = false;
  let drag = null;
  let pending = null;       // a press on a node, until it travels far enough
  let lastDragEnd = 0;      // a click always follows a drag; it is not a pick

  const ARM_PX = 5;         // travel inside a node before a press is a drag, not a click
  const SNAP_PX = 26;       // how far a release may miss a dot and still land on it

  const toWorld = (clientX, clientY) => {
    const c = camera();
    const box = worldEl.getBoundingClientRect();
    // the world element sits at the camera transform's origin
    return { x: (clientX - box.left) / c.z, y: (clientY - box.top) / c.z };
  };

  /** Centre of a port dot in world coordinates — what the layer projects. */
  function portWorld(paneId, portId) {
    const dot = panes.get(paneId)?.dots.get(portId);
    if (!dot) return null;
    const r = dot.getBoundingClientRect();
    const c = camera();
    const box = worldEl.getBoundingClientRect();
    // rects are already transformed; undo the camera to get world units
    return {
      x: (r.left + r.width / 2 - box.left) / c.z,
      y: (r.top + r.height / 2 - box.top) / c.z,
    };
  }

  /**
   * The pane element for an id.
   *
   * Scoped to `.pane` on purpose: a port dot carries `data-pane` too, and a
   * tile added *after* boot lands after the port layer in the DOM, so a bare
   * `[data-pane]` lookup would resolve to the tile's own dot. Placing the host
   * over a dot is self-referential: the host never moves, and every wire from
   * that tile keeps pointing at where it used to be.
   */
  function paneElFor(paneId) {
    return worldEl.querySelector(`.pane[data-pane="${cssEscape(paneId)}"]`);
  }

  /** The flat container the dots live in: a world child, above panes and sashes. */
  let layerEl = null;
  function ensureLayer() {
    if (layerEl && layerEl.isConnected) return layerEl;
    layerEl = worldEl.querySelector(':scope > .wire-port-layer');
    if (!layerEl) {
      layerEl = document.createElement('div');
      layerEl.className = 'wire-port-layer';
      worldEl.append(layerEl);           // last child: above panes and sashes
    }
    return layerEl;
  }

  /** Lay one port host over its pane's body box (world units). */
  function placeOne(entry, paneEl, force) {
    const body = paneEl.querySelector('.pane-body') || paneEl;
    const b = body.getBoundingClientRect();
    const box = worldEl.getBoundingClientRect();
    const z = camera().z || 1;
    const geo = {
      x: (b.left - box.left) / z,
      y: (b.top - box.top) / z,
      w: b.width / z,
      h: b.height / z,
    };
    const key = `${geo.x.toFixed(1)}/${geo.y.toFixed(1)}/${geo.w.toFixed(1)}/${geo.h.toFixed(1)}`;
    if (!force && entry.key === key) return;
    entry.key = key;
    Object.assign(entry.el.style, {
      left: `${geo.x}px`, top: `${geo.y}px`,
      width: `${geo.w}px`, height: `${geo.h}px`,
    });
  }

  /**
   * Follow the panes: cheap no-op when nothing moved, so call it per frame.
   *
   * `keep` is asked about each pane first. Reading a bounding rect costs layout,
   * so a pane nobody can see is not worth the read: its dots already sit in world
   * space and stay where they were until it moves again, which is exactly what
   * should happen while it is off screen.
   */
  function place(force, keep) {
    for (const [paneId, entry] of panes) {
      if (keep && !keep(paneId)) continue;
      const el = paneElFor(paneId);
      if (el) placeOne(entry, el, force);
    }
  }

  /** Build/refresh the dots of one pane from its kind descriptor. */
  function buildPorts(paneId, hostEl) {
    let entry = panes.get(paneId);
    if (!entry) {
      entry = { el: document.createElement('div'), dots: new Map(), key: '' };
      entry.el.className = 'wire-ports';
      panes.set(paneId, entry);
    }
    const layer = ensureLayer();
    if (entry.el.parentElement !== layer) layer.append(entry.el);
    placeOne(entry, hostEl, true);

    // the pane's own kind, read off the element we were handed — a dot also
    // carries data-kind, but that is the *port* kind, not the pane's
    const kind = getKind(hostEl.dataset.kind || paneId);
    const ports = (kind?.ports || []).filter((p) => portKinds.includes(p.kind));
    entry.el.innerHTML = '';
    entry.dots.clear();

    for (const dir of ['in', 'out']) {
      const list = ports.filter((p) => p.dir === dir);
      list.forEach((p, i) => {
        const dot = document.createElement('i');
        dot.className = `wire-port wire-port--${dir} tone-${toneOf(p.type)} kind-${p.kind}`;
        dot.dataset.pane = paneId;
        dot.dataset.port = p.id;
        dot.dataset.dir = dir;
        dot.dataset.kind = p.kind;
        dot.dataset.type = p.type;
        dot.title = `${dir} · ${p.kind} · ${p.type}`;
        // the container is already inset below the pane head, so the spread is
        // a plain percentage of the body box
        dot.style.top = pct(i, list.length);
        dot.style[dir === 'in' ? 'left' : 'right'] = '0';
        entry.el.append(dot);
        entry.dots.set(p.id, dot);
      });
    }
  }

  /** Rescan the world: panes come and go with every layout change. */
  function sync() {
    const seen = new Set();
    for (const el of worldEl.querySelectorAll('.pane[data-pane]')) {
      const paneId = el.dataset.pane;
      if (!paneId) continue;
      seen.add(paneId);
      buildPorts(paneId, el);
    }
    for (const [id, entry] of panes) {
      if (seen.has(id)) continue;
      entry.el.remove();
      panes.delete(id);
    }
    place();
    return panes.size;
  }

  /* ── drag to connect ───────────────────────────────────────────────────── */

  function canConnect(a, b) {
    if (!a || !b) return { ok: false, why: 'no target' };
    if (a.dir === b.dir) return { ok: false, why: 'output connects to input' };
    if (a.pane === b.pane) return { ok: false, why: 'a pane cannot wire itself' };
    const from = a.dir === 'out' ? a : b;
    const to = a.dir === 'out' ? b : a;
    if (!typeOk(from.type, to.type)) {
      return { ok: false, why: `${from.type} does not fit ${to.type}` };
    }
    return { ok: true, from: { p: from.pane, port: from.port }, to: { p: to.pane, port: to.port } };
  }

  function dotFromEvent(target) {
    const dot = target?.closest?.('.wire-port');
    if (!dot) return null;
    return {
      pane: dot.dataset.pane,
      port: dot.dataset.port,
      dir: dot.dataset.dir,
      kind: dot.dataset.kind,
      type: dot.dataset.type,
      el: dot,
    };
  }

  /**
   * Port under the cursor, even when something else is painted on top.
   *
   * Sashes sit in the 8 px gutter between panes and are appended after them, so
   * they cover the half of a port dot that hangs over the pane edge. Hit testing
   * by stacking order alone would make those ports undraggable, so when the top
   * element is not a dot we look down the stack (`elementsFromPoint`) — but only
   * accept a dot the user can actually see, otherwise a hidden dot would steal
   * clicks from the sash underneath it.
   */
  function dotUnder(x, y) {
    const stack = document.elementsFromPoint ? document.elementsFromPoint(x, y) : [];
    for (const el of stack) {
      const dot = dotFromEvent(el);
      if (!dot) continue;
      if (!wiring && !drag && Number(getComputedStyle(dot.el).opacity) < 0.9) continue;
      return dot;
    }
    return null;
  }

  const dotAt = (x, y, fallbackTarget) => dotFromEvent(fallbackTarget) || dotUnder(x, y);

  function startDrag(ev, from) {
    endPending();
    drag = { from, valid: false, target: null, x: ev.clientX, y: ev.clientY };
    document.body.dataset.wiring = '1';
    window.addEventListener('pointermove', onPointerMove);
    window.addEventListener('pointerup', onPointerUp);
    window.addEventListener('keydown', onKeyDown, true);
    // where this wire may go, lit before the pointer has moved
    for (const el of legalTargets(panes, from, canConnect)) el.classList.add('hot');
    paintGhost();
  }

  function onPointerDown(ev) {
    if (ev.button !== 0) return;
    const from = dotAt(ev.clientX, ev.clientY, ev.target);
    if (from) {
      ev.preventDefault();
      ev.stopPropagation();
      startDrag(ev, from);
      return;
    }
    // not on a dot: a press on a node draws from it, once the pointer travels.
    // Until then nothing is consumed, so the pane keeps its own clicks and drags
    const src = sourceFor(panes, ev.target, ev.clientX, ev.clientY);
    if (!src) return;
    pending = { src, x: ev.clientX, y: ev.clientY };
    window.addEventListener('pointermove', onArmMove);
    window.addEventListener('pointerup', onArmUp, true);
  }

  /** The press has now travelled far enough inside its node to be a drag. */
  function onArmMove(ev) {
    if (!pending || drag) return;
    if (Math.hypot(ev.clientX - pending.x, ev.clientY - pending.y) < ARM_PX) return;
    startDrag(ev, pending.src);
    onPointerMove(ev);
  }

  /** A press that never travelled is the pane's own gesture, not a wire. */
  function onArmUp() { if (!drag) endPending(); }

  function endPending() {
    if (!pending) return;
    pending = null;
    window.removeEventListener('pointermove', onArmMove);
    window.removeEventListener('pointerup', onArmUp, true);
  }

  function onPointerMove(ev) {
    if (!drag) return;
    drag.x = ev.clientX;
    drag.y = ev.clientY;
    const over = dotAt(ev.clientX, ev.clientY, document.elementFromPoint(ev.clientX, ev.clientY));
    const other = over && over.el !== drag.from.el ? over : null;
    const verdict = other ? canConnect(drag.from, other) : { ok: false, why: null };
    drag.valid = verdict.ok;
    drag.target = other;
    drag.reason = verdict.why;
    // the legal set was lit at the start; only the dot under the pointer can be
    // wrong, and saying so before the release is the point
    const bad = other && !verdict.ok ? other.el : null;
    for (const entry of panes.values()) {
      for (const dot of entry.dots.values()) dot.classList.toggle('bad', dot === bad);
    }
    paintGhost();
  }

  function onPointerUp(ev) {
    if (!drag) return;
    const from = drag.from;
    // a release beside a dot counts as on it; the dot under the cursor wins
    const target = drag.target
      || nearestLegal(panes, from, ev.clientX, ev.clientY, { px: SNAP_PX, canConnect });
    const verdict = target ? canConnect(from, target) : { ok: false, why: 'dropped on nothing' };
    endDrag();
    if (verdict.ok) {
      onConnect && onConnect({ ...verdict, kind: from.kind, type: from.type === 'any' ? target.type : from.type });
    } else if (target && onRefuse) {
      onRefuse(verdict.why, { from, target, x: ev.clientX, y: ev.clientY });
    }
  }

  function onKeyDown(ev) {
    if (ev.key === 'Escape' && drag) {
      endDrag();
      ev.preventDefault();
    }
  }

  function endDrag() {
    if (drag) lastDragEnd = performance.now();
    drag = null;
    endPending();
    delete document.body.dataset.wiring;
    window.removeEventListener('pointermove', onPointerMove);
    window.removeEventListener('pointerup', onPointerUp);
    window.removeEventListener('keydown', onKeyDown, true);
    for (const entry of panes.values()) for (const dot of entry.dots.values()) dot.classList.remove('hot', 'bad');
    layer && layer.clear && layer.clear();
  }

  function paintGhost() {
    if (!layer || !drag) return;
    const a = portWorld(drag.from.pane, drag.from.port);
    if (!a) return;
    // both ends in canvas space: the layer owns that projection, so ask it
    const cv = layer.canvas || layerCanvas();
    const box = cv.getBoundingClientRect();
    const fromScreen = layer.toScreen ? layer.toScreen(a.x, a.y) : { x: a.x, y: a.y };
    const toScreen = { x: drag.x - box.left, y: drag.y - box.top };
    layer.drawGhost(fromScreen, toScreen, {
      kind: drag.from.kind,
      valid: drag.valid,
      mirrored: drag.from.dir === 'in',
    });
  }

  /* ── wire picking ──────────────────────────────────────────────────────── */

  function onClick(ev) {
    if (!layer || drag) return;
    // finishing a wire drag fires a click on the canvas as well; without this
    // every connection ended with a stray "no wire under cursor"
    if (performance.now() - lastDragEnd < 350) return;
    if (ev.target !== layerCanvas()) return;
    const box = layerCanvas().getBoundingClientRect();
    const hit = layer.hitTest(currentWires(), ev.clientX - box.left, ev.clientY - box.top);
    onSelect && onSelect(hit);
  }

  function layerCanvas() {
    return layer && layer.canvas ? layer.canvas : null;
  }

  // the owner keeps the wire list; this is only the reference the layer uses
  let wires = [];
  function setWires(list) { wires = list || []; }
  function currentWires() { return wires; }

  function attach() {
    // capture phase: a pane header stops pointerdown before it bubbles, and a
    // port drag must still start when the dot sits over a header
    document.addEventListener('pointerdown', onPointerDown, true);
    const cv = layerCanvas();
    if (cv) cv.addEventListener('click', onClick);
    return detach;
  }

  function detach() {
    document.removeEventListener('pointerdown', onPointerDown, true);
    const cv = layerCanvas();
    if (cv) cv.removeEventListener('click', onClick);
    endDrag();
    for (const entry of panes.values()) entry.el.remove();
    panes.clear();
  }

  function setWiring(on) {
    wiring = !!on;
    document.body.classList.toggle('wire-mode', wiring);
    // the canvas is pointer-transparent so panes keep their clicks; in wire
    // mode a click can only mean "pick a wire", so picking is switched on
    const cv = layerCanvas();
    if (cv) cv.classList.toggle('picking', wiring);
  }

  return { sync, place, attach, detach, portWorld, setWires, setWiring, canConnect, get wiring() { return wiring; }, get dragging() { return drag; } };
}

/* ── helpers ─────────────────────────────────────────────────────────────── */

const TONE_BY_TYPE = {
  NodeId: 'cyan',
  Path: 'amber',
  Cursor: 'violet',
  Text: 'green',
  CSS: 'violet',
  num: 'cyan',
  any: 't2',
};
const toneOf = (type) => TONE_BY_TYPE[type] || 't2';

function pct(i, n) {
  // spread the dots down the pane's body; never let them touch the edges
  const step = 100 / (n + 1);
  return `${(step * (i + 1)).toFixed(2)}%`;
}

function cssEscape(v) {
  if (typeof CSS !== 'undefined' && CSS.escape) return CSS.escape(v);
  return String(v).replace(/["\\]/g, '\\$&');
}