/**
 * drag.js — Drag and drop engine (M2.16).
 *
 * Handles header/tab drag with Pointer Events.
 * Only active when lock.get() === false (design mode).
 * Dispatches tree.move on drop.
 */

import { CONFIG } from '../core/config.js';
import { targetAt } from '../layout/dropzone.js';
import { showDropOverlay, hideDropOverlay } from './overlay.js';

let dragState = null;
let ghost = null;
let pointerId = null;

/**
 * Check if dragging is allowed (design mode).
 * @param {object} ws - Workspace API
 * @returns {boolean}
 */
function isDragAllowed(ws) {
  return ws.lock && ws.lock.get() === false;
}

/**
 * Create the drag ghost element.
 * @param {string} title - Pane title
 * @returns {HTMLElement}
 */
function createGhost(title) {
  const el = document.createElement('div');
  el.className = 'drag-ghost';
  el.style.cssText = `
    position: fixed;
    pointer-events: none;
    z-index: 9999;
    padding: 4px 12px;
    background: #1c2130;
    border: 1px solid #3b82f6;
    border-radius: 4px;
    color: #c8d2e0;
    font-size: 12px;
    font-weight: 500;
    white-space: nowrap;
    box-shadow: 0 4px 12px rgba(0,0,0,0.4);
    transform: translate(-50%, -100%);
    opacity: 0;
    transition: opacity 100ms ease;
  `;
  el.textContent = title;
  document.body.appendChild(el);
  return el;
}

/**
 * Remove the ghost element.
 */
function removeGhost() {
  if (ghost) {
    ghost.remove();
    ghost = null;
  }
}

/**
 * Handle pointer down on a draggable element.
 * @param {PointerEvent} e
 * @param {object} ctx - {ws, store, instances, camera, worldEl}
 * @param {object} dragInfo - {paneId, type: 'header'|'tab'|'root-handle', tabIndex?}
 */
function onPointerDown(e, ctx, dragInfo) {
  const { ws, store, instances, camera, worldEl } = ctx;
  
  // Only allow drag in design mode
  if (!isDragAllowed(ws)) return;
  
  // Only left button
  if (e.button !== 0) return;
  
  // Set up drag state
  dragState = {
    paneId: dragInfo.paneId,
    type: dragInfo.type,
    tabIndex: dragInfo.tabIndex,
    startX: e.clientX,
    startY: e.clientY,
    currentX: e.clientX,
    currentY: e.clientY,
    hasMoved: false,
    ctx,
    pointerId: e.pointerId
  };
  
  pointerId = e.pointerId;
  
  // Capture pointer
  e.target.setPointerCapture(e.pointerId);
  
  // Listen for move and up
  document.addEventListener('pointermove', onPointerMove);
  document.addEventListener('pointerup', onPointerUp);
  document.addEventListener('pointercancel', onPointerCancel);
  
  // Prevent default browser behavior
  e.preventDefault();
}

/**
 * Handle pointer move.
 * @param {PointerEvent} e
 */
function onPointerMove(e) {
  if (!dragState || e.pointerId !== pointerId) return;
  
  const dx = e.clientX - dragState.startX;
  const dy = e.clientY - dragState.startY;
  const distance = Math.sqrt(dx * dx + dy * dy);
  
  // Check if we've moved enough to start dragging (4px threshold)
  if (!dragState.hasMoved && distance < 4) return;
  
  if (!dragState.hasMoved) {
    dragState.hasMoved = true;
    
    // Create ghost
    const pane = dragState.ctx.instances.get(dragState.paneId);
    const title = pane?.paneInstance?.title || 'Pane';
    ghost = createGhost(pane?.paneInstance?.title || 'Pane');
    
    // Start gesture
    dragState.ctx.store.beginGesture?.();
  }
  
  dragState.currentX = e.clientX;
  dragState.currentY = e.clientY;
  
  // Update ghost position
  if (ghost) {
    ghost.style.left = e.clientX + 'px';
    ghost.style.top = (e.clientY - 20) + 'px';
    ghost.style.opacity = '1';
  }
  
  // Update drop overlay
  const { store, camera, worldEl } = dragState.ctx;
  const layout = store.get();
  if (layout) {
    // Convert screen point to world coordinates
    const cam = camera.get();
    const worldX = (e.clientX - cam.x) / cam.z;
    const worldY = (e.clientY - cam.y) / cam.z;
    
    // Compute layout rects for drop target
    const layoutRects = computeLayoutRects(store.get(), camera.get());
    
    // Find drop target
    const target = targetAt(layoutRects, { x: worldX, y: worldY }, CONFIG.layout.dropEdgeFraction);
    
    if (target) {
      showDropOverlay(worldEl, target.zone, layoutRects.find(r => r.paneId === target.paneId)?.rect, camera.get().z);
    } else {
      hideDropOverlay();
    }
  }
}

/**
 * Handle pointer up.
 * @param {PointerEvent} e
 */
function onPointerUp(e) {
  if (!dragState || e.pointerId !== pointerId) return;
  
  // Clean up
  document.removeEventListener('pointermove', onPointerMove);
  document.removeEventListener('pointerup', onPointerUp);
  document.removeEventListener('pointercancel', onPointerCancel);
  
  if (e.target.releasePointerCapture) {
    e.target.releasePointerCapture(e.pointerId);
  }
  
  if (!dragState.hasMoved) {
    // Click, not drag
    removeGhost();
    hideDropOverlay();
    dragState.ctx.store.endGesture?.();
    dragState = null;
    pointerId = null;
    return;
  }
  
  // Compute drop target
  const { store, camera, worldEl } = dragState.ctx;
  const cam = camera.get();
  const worldX = (dragState.currentX - cam.x) / cam.z;
  const worldY = (dragState.currentY - cam.y) / cam.z;
  
  const layoutRects = computeLayoutRects(store.get(), camera.get());
  const target = targetAt(layoutRects, { x: worldX, y: worldY }, CONFIG.layout.dropEdgeFraction);
  
  hideDropOverlay();
  removeGhost();
  
  if (target) {
    // Build move command
    let to;
    switch (target.zone) {
      case 'left':
      case 'right':
      case 'top':
      case 'bottom':
        to = { beside: target.paneId, side: target.zone };
        break;
      case 'center':
        to = { tabInto: target.paneId };
        break;
      default:
        // New root
        const cam = camera.get();
        const defaultSize = CONFIG.layout.defaultNewRoot || { w: 520, h: 380 };
        to = {
          newRoot: {
            x: (dragState.currentX - cam.x) / cam.z - defaultSize.w / 2,
            y: (dragState.currentY - cam.y) / cam.z - defaultSize.h / 2
          }
        };
    }
    
    // Dispatch move command
    const { store: s } = dragState.ctx;
    s.dispatch({
      type: 'tree.move',
      id: dragState.paneId,
      to
    });
  }
  
  dragState.ctx.store.endGesture?.();
  dragState = null;
  pointerId = null;
}

/**
 * Handle pointer cancel.
 * @param {PointerEvent} e
 */
function onPointerCancel(e) {
  if (!dragState || e.pointerId !== pointerId) return;
  
  document.removeEventListener('pointermove', onPointerMove);
  document.removeEventListener('pointerup', onPointerUp);
  document.removeEventListener('pointercancel', onPointerCancel);
  
  hideDropOverlay();
  removeGhost();
  dragState.ctx.store.endGesture?.();
  dragState = null;
  pointerId = null;
}


/**
 * Handle palette pointer move.
 * @param {PointerEvent} e
 */
function onPalettePointerMove(e) {
  if (!dragState || dragState.source !== 'palette' || e.pointerId !== pointerId) return;

  const dx = e.clientX - dragState.startX;
  const dy = e.clientY - dragState.startY;
  const distance = Math.sqrt(dx * dx + dy * dy);

  if (!dragState.hasMoved && distance < 4) return;

  if (!dragState.hasMoved) {
    dragState.hasMoved = true;
    dragState.ctx.store.beginGesture?.();
  }

  dragState.currentX = e.clientX;
  dragState.currentY = e.clientY;

  const { store, camera, worldEl } = dragState.ctx;
  const layout = store.get();
  if (layout) {
    const cam = camera.get();
    const worldX = (e.clientX - cam.x) / cam.z;
    const worldY = (e.clientY - cam.y) / cam.z;

    const layoutRects = computeLayoutRects(store.get(), camera.get());
    const target = targetAt(layoutRects, { x: worldX, y: worldY }, CONFIG.layout.dropEdgeFraction);

    if (target) {
      showDropOverlay(worldEl, target.zone, layoutRects.find(r => r.paneId === target.paneId)?.rect, camera.get().z);
    } else {
      hideDropOverlay();
    }
  }
}


/**
 * Compute layout rects for all panes in world coordinates.
 * @param {object} layout
 * @param {object} camera
 * @returns {Array} [{paneId, rect}]
 */
function computeLayoutRects(layout, camera) {
  const result = [];
  const cam = camera || { x: 0, y: 0, z: 1 };
  
  if (!layout || !layout.roots) return [];
  
  const z = cam.z || 1;
  const worldToScreen = (x, y) => ({ x: x * z + cam.x, y: y * z + cam.y });
  
  function processRoot(root) {
    if (root && root.tree) {
      const rootId = root.id || 'main';
      const rootRect = { x: root.x || 0, y: root.y || 0, w: root.w || 0, h: root.h || 0 };
      processNode(rootId, root.tree, rootRect);
    }
  }
  
  function processNode(paneId, node, parentRect) {
    if (!node) return;
    
    // Compute node rect (simplified)
    const rect = computeNodeRect(node, parentRect);
    if (rect && rect.w > 0 && rect.h > 0) {
      // Convert to world coords
      result.push({ paneId, rect: { ...rect } });
    }
    
    // Recurse
    if (node.tabs) {
      node.tabs.forEach((child, i) => {
        const childRect = { ...parentRect, y: parentRect.y + 28, h: Math.max(0, parentRect.h - 28) };
        processNode(`${paneId}.tabs.${i}`, child, childRect);
      });
    } else if (node.split) {
      const children = node.children || [];
      const isH = node.split === 'h';
      const sizes = node.sizes || children.map(() => 1 / children.length);
      let pos = isH ? parentRect.x : parentRect.y;
      children.forEach((child, i) => {
        const childRect = { ...parentRect };
        if (isH) {
          childRect.x = pos;
          childRect.w = parentRect.w * (sizes[i] || 1 / children.length);
        } else {
          childRect.y = pos;
          childRect.h = parentRect.h * (sizes[i] || 1 / children.length);
        }
        pos += isH ? childRect.w : childRect.h;
        processNode(`${paneId}.split.${i}`, child, childRect);
      });
    } else if (node.group) {
      processNode(`${paneId}.group`, node.group, parentRect);
    }
  }
  
  layout.roots.forEach(processRoot);
  return result;
}

function computeNodeRect(node, parentRect) {
  // Simplified - in reality this uses layout/computeRects
  // For now, return the parent rect as placeholder
  return parentRect ? { ...parentRect } : null;
}

/**
 * Install drag handlers on the world element.
 * @param {object} ctx - {ws, store, instances, camera, worldEl}
 */
export function installDrag(ctx) {
  const { worldEl } = ctx;
  
  // Delegate pointerdown on draggable elements within worldEl
  worldEl.addEventListener('pointerdown', (e) => {
    // Check if target is a draggable header/tab/root-handle
    const header = e.target.closest('.qb-pane-head');
    const tab = e.target.closest('.qb-tab');
    const rootHandle = e.target.closest('.root-handle');
    
    if (header) {
      const paneEl = header.closest('[data-pane]');
      const paneId = paneEl?.dataset.pane;
      if (paneId) {
        onPointerDown(e, { ws: dragState?.ctx?.ws }, { paneId, type: 'header' });
      }
    } else if (tab) {
      const tabsEl = tab.closest('.qb-tabs');
      const paneEl = tabsEl?.closest('[data-pane]');
      const paneId = paneEl?.dataset.pane;
      const tabs = Array.from(tabsEl?.querySelectorAll('.qb-tab') || []);
      const tabIndex = tabs.indexOf(tab);
      if (paneId && tabIndex >= 0) {
        onPointerDown(e, { ws: dragState?.ctx?.ws }, { paneId, type: 'tab', tabIndex });
      }
    } else if (rootHandle) {
      const rootEl = rootHandle.closest('[data-root]');
      const rootId = rootEl?.dataset.root;
      if (rootId) {
        onPointerDown(e, { ws: dragState?.ctx?.ws }, { paneId: rootId, type: 'root-handle' });
      }
    }
  });
  
  
  return {
    destroy() {
      // Event listeners are on worldEl, would need to be removed properly
    }
  };
}
