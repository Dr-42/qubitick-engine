/**
 * root-handles.js — Root move/resize handles (M3.02).
 *
 * Unlocked only. Each .root gets:
 *   - .root-handle: 20px strip above the root's top edge showing root id
 *   - 8 .root-grip elements (n, s, e, w, ne, nw, se, sw) for resize
 *
 * Dragging a handle moves the root (root.move / anchor.offset).
 * Dragging a grip resizes the root (root.resize). Delta ÷ z.
 * All created only when lock.get() === false; removed when locked.
 */

import { resizeRoot } from '../layout/resize-root.js';

const GRIP_EDGES = ['n', 's', 'e', 'w', 'ne', 'nw', 'se', 'sw'];

/**
 * Create and install root handles and resize grips.
 * @param {object} opts
 * @param {HTMLElement} opts.root - Root DOM element (#root)
 * @param {object} opts.store - Layout store
 * @param {object} opts.lock - Lock port {get:()=>boolean}
 * @param {function} opts.getZoom - () => current camera zoom
 * @param {function} opts.onRootRects - () => Map<rootId, {x,y,w,h}> resolved root rects
 * @returns {{destroy:()=>void, refresh:(rects)=>void}}
 */
export function createRootHandles({ root, store, lock, getZoom, onRootRects }) {
  const handles = new Map(); // rootId => {handleEl, grips: Map<edge, el>}

  function isUnlocked() {
    return lock && lock.get() === false;
  }

  function cursorFor(edge) {
    const m = {
      n: 'ns-resize', s: 'ns-resize',
      e: 'ew-resize', w: 'ew-resize',
      ne: 'nesw-resize', nw: 'nwse-resize',
      se: 'nesw-resize', sw: 'nwse-resize',
    };
    return m[edge] || 'default';
  }

  function gripPos(edge, x, y, w, h, g) {
    const half = g / 2;
    switch (edge) {
      case 'n': return { x: x + w / 2 - half, y: y - half, w: g, h: g };
      case 's': return { x: x + w / 2 - half, y: y + h - half, w: g, h: g };
      case 'e': return { x: x + w - half, y: y + h / 2 - half, w: g, h: g };
      case 'w': return { x: x - half, y: y + h / 2 - half, w: g, h: g };
      case 'ne': return { x: x + w - half, y: y - half, w: g, h: g };
      case 'nw': return { x: x - half, y: y - half, w: g, h: g };
      case 'se': return { x: x + w - half, y: y + h - half, w: g, h: g };
      case 'sw': return { x: x - half, y: y + h - half, w: g, h: g };
      default: return { x, y, w: g, h: g };
    }
  }

  function createHandleEl(rootId) {
    const el = document.createElement('div');
    el.className = 'root-handle';
    el.textContent = rootId;
    el.style.cssText =
      'position:absolute; display:flex; align-items:center; justify-content:center;'
      + 'width:100%; height:20px; background:#1c2130; border:1px solid #3b82f6;'
      + 'border-bottom:none; border-radius:4px 4px 0 0; color:#d6e2ee;'
      + 'font-size:10px; z-index:1300; cursor:pointer; user-select:none;';
    return el;
  }

  function createGripEl(edge) {
    const el = document.createElement('div');
    el.className = 'root-grip root-grip-' + edge;
    el.setAttribute('data-edge', edge);
    const g = 12;
    const half = g / 2;
    el.style.cssText =
      'position:absolute; z-index:1350; background:transparent;'
      + 'cursor:' + cursorFor(edge) + '; box-sizing:border-box;';
    return el;
  }

  function createRootUI(rootId, rect) {
    const { x, y, w, h } = rect;
    const handleEl = createHandleEl(rootId);
    const grips = new Map();

    GRIP_EDGES.forEach(edge => {
      const el = createGripEl(edge);
      const pos = gripPos(edge, x, y, w, h, 12);
      el.style.left = pos.x + 'px';
      el.style.top = pos.y + 'px';
      el.style.width = pos.w + 'px';
      el.style.height = pos.h + 'px';
      el.addEventListener('pointerdown', (e) => onGripDown(e, rootId, edge));
      grips.set(edge, el);
      handleEl.appendChild(el);
    });

    handleEl.style.left = x + 'px';
    handleEl.style.top = (y - 20) + 'px';
    handleEl.style.width = w + 'px';
    root.appendChild(handleEl);
    handles.set(rootId, { handleEl, grips });
  }

  function removeRootUI(rootId) {
    const entry = handles.get(rootId);
    if (entry) {
      entry.handleEl.remove();
      handles.delete(rootId);
    }
  }

  function positionAll(rects) {
    rects.forEach((rect, id) => {
      const entry = handles.get(id);
      if (!entry) return;
      const { x, y, w, h } = rect;
      entry.handleEl.style.left = x + 'px';
      entry.handleEl.style.top = (y - 20) + 'px';
      entry.handleEl.style.width = w + 'px';
      entry.grips.forEach((el, edge) => {
        const pos = gripPos(edge, x, y, w, h, 12);
        el.style.left = pos.x + 'px';
        el.style.top = pos.y + 'px';
        el.style.width = pos.w + 'px';
        el.style.height = pos.h + 'px';
      });
    });
  }

  // --- resize grip drag ---
  let dragStartX = 0, dragStartY = 0;
  let dragStartRect = null;
  let dragEdge = null;
  let dragRootId = null;

  function onGripDown(e, rootId, edge) {
    if (!isUnlocked()) return;
    e.preventDefault();
    e.stopPropagation();

    const rects = onRootRects ? onRootRects() : new Map();
    const rect = rects.get(rootId);
    if (!rect) return;

    dragRootId = rootId;
    dragEdge = edge;
    dragStartX = e.clientX;
    dragStartY = e.clientY;
    dragStartRect = { ...rect };

    const z = Math.max(getZoom ? getZoom() : 1, 0.01);

    if (store && typeof store.beginGesture === 'function') store.beginGesture();

    const move = (ev) => {
      const dxClient = ev.clientX - dragStartX;
      const dyClient = ev.clientY - dragStartY;
      const dxLayout = dxClient / z;
      const dyLayout = dyClient / z;
      const next = resizeRoot(dragStartRect, dragEdge, dxLayout, dyLayout);
      if (store && typeof store.dispatch === 'function') {
        store.dispatch({ type: 'root.resize', id: rootId, w: next.w, h: next.h });
        if (next.x !== dragStartRect.x || next.y !== dragStartRect.y) {
          store.dispatch({ type: 'root.move', id: rootId, x: next.x, y: next.y });
        }
      }
    };

    const up = (ev) => {
      ev.stopPropagation();
      if (store && typeof store.endGesture === 'function') store.endGesture();
      dragRootId = null;
      dragEdge = null;
      dragStartRect = null;
      document.removeEventListener('pointermove', move);
      document.removeEventListener('pointerup', up);
      document.removeEventListener('pointercancel', up);
    };

    document.addEventListener('pointermove', move);
    document.addEventListener('pointerup', up);
    document.addEventListener('pointercancel', up);
  }

  function refresh(rects) {
    if (!isUnlocked()) {
      handles.forEach((_, id) => removeRootUI(id));
      handles.clear();
      return;
    }
    // Create missing
    rects.forEach((rect, id) => {
      if (!handles.has(id)) createRootUI(id, rect);
    });
    // Remove stale
    handles.forEach((_, id) => {
      if (!rects.has(id)) removeRootUI(id);
    });
    // Position all
    positionAll(rects);
  }

  // Listen for lock changes
  const unsub = lock ? lock.subscribe(() => {
    const rects = onRootRects ? onRootRects() : new Map();
    refresh(rects);
  }) : () => {};

  // Initial
  const initialRects = onRootRects ? onRootRects() : new Map();
  refresh(initialRects);

  return {
    destroy() {
      unsub();
      handles.forEach((_, id) => removeRootUI(id));
      handles.clear();
    },
    refresh,
  };
}
