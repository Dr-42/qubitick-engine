/**
 * anchor-ui.js — Anchor context menu + dashed lines (M3.06).
 *
 * Root context menu items:
 *   - Anchor to… — lists other roots whose anchoring would not cause a cycle;
 *     uses suggestAnchor → anchor.set.
 *   - Clear anchor — anchor.clear, keeps current x,y.
 *
 * A thin dashed line in design mode (SVG in .world, pointer-events:none)
 * joins each anchored root to its parent's anchor point.
 *
 * @param {object} opts
 * @param {HTMLElement} opts.root - root element to attach SVG to
 * @param {object} opts.store - layout store {dispatch(cmd), get()}
 * @param {function} opts.getRootRects - () => Map<rootId, {x,y,w,h}>
 * @returns {{destroy:()=>void, setContextMenu:(menu)=>void, drawLines:()=>void}}
 */

import { suggestAnchor } from '../layout/suggest-anchor.js';
import { detectCycle } from '../layout/anchors.js';

const EDGES = ['left', 'right', 'top', 'bottom'];

/**
 * Check if adding anchor childId -> parentId would create a cycle.
 * @param {Array} anchors - current anchor list
 * @param {string} childId
 * @param {string} parentId
 * @returns {boolean} true if safe (no cycle)
 */
export function detectAnchorCycle(anchors, childId, parentId) {
  const testAnchors = [
    ...anchors.filter(a => a.root !== childId),
    { root: childId, to: parentId, edge: 'left', toEdge: 'right', dx: 0, dy: 0 },
  ];
  return detectCycle(testAnchors) !== null;
}

/**
 * Create the anchor UI.
 */
export function createAnchorUI({ root, store, getRootRects }) {
  // SVG overlay for dashed anchor lines
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  /* `className` on an SVG element is a read-only SVGAnimatedString: the class goes on with
     setAttribute (this threw the first time the module was ever constructed). */
  svg.setAttribute('class', 'anchor-lines');
  svg.style.cssText =
    'position:absolute; inset:0; pointer-events:none; z-index:100; display:block;';
  root.appendChild(svg);

  let contextMenu = null;

  function drawLines() {
    svg.innerHTML = '';
    const layout = store.get();
    const anchors = layout.anchors || [];
    const rootRects = getRootRects ? getRootRects() : new Map();

    for (const a of anchors) {
      const parentRect = rootRects.get(a.to);
      const childRect = rootRects.get(a.root);
      if (!parentRect || !childRect) continue;

      // Reference point P on parent
      const P = referencePoint(parentRect, a.toEdge);

      const line = document.createElementNS('http://www.w3.org/2000/svg', 'line');
      line.setAttribute('x1', Math.round(P.x));
      line.setAttribute('y1', Math.round(P.y));
      line.setAttribute('x2', Math.round(childRect.x + childRect.w / 2));
      line.setAttribute('y2', Math.round(childRect.y + childRect.h / 2));
      line.setAttribute('stroke', '#4c8cff');
      line.setAttribute('stroke-width', '1');
      line.setAttribute('stroke-dasharray', '4,4');
      line.setAttribute('opacity', '0.7');
      svg.appendChild(line);

      // Small dot at parent anchor point
      const dot = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
      dot.setAttribute('cx', Math.round(P.x));
      dot.setAttribute('cy', Math.round(P.y));
      dot.setAttribute('r', '3');
      dot.setAttribute('fill', '#4c8cff');
      svg.appendChild(dot);
    }
  }

  function referencePoint(parent, toEdge) {
    switch (toEdge) {
      case 'left':
      case 'top':
        return { x: parent.x, y: parent.y };
      case 'right':
        return { x: parent.x + parent.w, y: parent.y };
      case 'bottom':
        return { x: parent.x, y: parent.y + parent.h };
      default:
        return { x: parent.x, y: parent.y };
    }
  }

  function openAnchorSubmenu(x, y, childId) {
    const layout = store.get();
    const anchors = layout.anchors || [];
    const roots = layout.roots || [];
    const rootRects = getRootRects ? getRootRects() : new Map();

    const validTargets = roots.filter(r => {
      if (r.id === childId) return false;
      if (anchors.some(a => a.root === childId && a.to === r.id)) return false; // already anchored
      return !detectAnchorCycle(anchors, childId, r.id);
    });

    if (validTargets.length === 0) return;

    const items = validTargets.map(r => {
      const parentRect = rootRects.get(r.id);
      const childRect = rootRects.get(childId);
      let suggestion = { edge: 'left', toEdge: 'right', dx: 0, dy: 0 };
      if (parentRect && childRect) {
        suggestion = suggestAnchor(parentRect, childRect);
      }
      return {
        label: r.id,
        action: `anchor-set:${childId}:${r.id}:${suggestion.edge}:${suggestion.toEdge}:${suggestion.dx}:${suggestion.dy}`,
      };
    });

    if (contextMenu) {
      contextMenu.open(items, x, y, false);
    }
  }

  function openClearAnchor(x, y, childId) {
    const anchors = (store.get().anchors || []);
    if (!anchors.some(a => a.root === childId)) return;

    if (contextMenu) {
      contextMenu.open([
        {
          label: 'Clear anchor',
          action: `clear-anchor:${childId}`,
        },
      ], x, y, false);
    }
  }

  /* The renderer's pane menu says WHICH root was chosen and hands over the click: the event
     name carries the pane id (`ui:anchor-to:<paneId>`) and a listener cannot match on a
     prefix, so the two sides agreed on nothing and this menu never opened. The event is the
     bare name now with the pane id in its detail, and this is the listener for it. */
  function paneIdFrom(e, name) {
    const detail = e.detail || {};
    if (detail.paneId) return detail.paneId;
    // an event named `ui:<name>:<paneId>` still reads (a caller that was not updated)
    const prefix = 'ui:' + name + ':';
    return e.type.startsWith(prefix) ? e.type.slice(prefix.length) : null;
  }

  function onAnchorTo(e) {
    const childId = paneIdFrom(e, 'anchor-to');
    if (!childId) return;
    e.preventDefault();
    e.stopPropagation();
    const d = e.detail || {};
    openAnchorSubmenu(d.clientX ?? e.clientX ?? 0, d.clientY ?? e.clientY ?? 0, childId);
  }

  function onClearAnchorEvent(e) {
    const childId = paneIdFrom(e, 'clear-anchor');
    if (!childId) return;
    e.preventDefault();
    e.stopPropagation();
    const d = e.detail || {};
    openClearAnchor(d.clientX ?? e.clientX ?? 0, d.clientY ?? e.clientY ?? 0, childId);
  }

  /* on the document: the renderer dispatches from the world element, which is a descendant of
     whatever this was attached to */
  document.addEventListener('ui:anchor-to', onAnchorTo);
  document.addEventListener('ui:clear-anchor', onClearAnchorEvent);

  /* AND THE LINES ARE DRAWN WHEN THE DOCUMENT CHANGES. `drawLines` was this module's own API and
     nothing called it — an anchor could be set (and the roots moved to match, by the store's own
     normalization) while the overlay stayed empty, because the one thing that knows about the
     new anchor is the document. */
  const unsubscribe = store && store.subscribe ? store.subscribe(() => drawLines()) : () => {};
  drawLines();

  // Listen for anchor-set and clear-anchor actions from context menu
  // These come as custom events too since context-menu uses onAction callback
  // We expose a method to set the context menu reference

  return {
    destroy() {
      document.removeEventListener('ui:anchor-to', onAnchorTo);
      document.removeEventListener('ui:clear-anchor', onClearAnchorEvent);
      unsubscribe();
      svg.remove();
    },
    setContextMenu(menu) {
      contextMenu = menu;
    },
    drawLines,
    openAnchorSubmenu,
    openClearAnchor,
  };
}

/**
 * Handle an anchor-set action from the context menu.
 * Dispatches anchor.set via the store.
 * @param {object} store
 * @param {string} action - e.g. 'anchor-set:childId:parentId:left:right:252:-350'
 * @returns {boolean} true if dispatched
 */
export function handleAnchorSet(store, action) {
  const parts = action.split(':');
  // anchor-set:childId:parentId:edge:toEdge:dx:dy
  if (parts.length < 7) return false;
  const [, childId, parentId, edge, toEdge, dxStr, dyStr] = parts;
  const dx = parseFloat(dxStr);
  const dy = parseFloat(dyStr);
  if (!childId || !parentId || !edge || !toEdge || Number.isNaN(dx) || Number.isNaN(dy)) return false;
  const result = store.dispatch({
    type: 'anchor.set',
    root: childId,
    to: parentId,
    edge,
    toEdge,
    dx,
    dy,
  });
  return result.ok;
}

/**
 * Handle a clear-anchor action from the context menu.
 * @param {object} store
 * @param {string} action - e.g. 'clear-anchor:childId'
 * @returns {boolean} true if dispatched
 */
export function handleClearAnchor(store, action) {
  const parts = action.split(':');
  // clear-anchor:childId
  if (parts.length < 2) return false;
  const [, childId] = parts;
  const result = store.dispatch({ type: 'anchor.clear', root: childId });
  return result.ok;
}

