/**
 * palette-drag.js — Palette item drag support (M3.12).
 * Self-installs document-level pointerdown for palette items.
 */

import { CONFIG } from '../core/config.js';
import { targetAt } from '../layout/dropzone.js';
import { showDropOverlay, hideDropOverlay } from '../engine/overlay.js';
import { paletteDrop } from './palette.js';

let dragState = null;
let pointerId = null;

/**
 * Compute layout rects for all panes in world coordinates.
 */
function computeLayoutRects(layout, camera) {
  const result = [];
  const cam = camera || { x: 0, y: 0, z: 1 };
  if (!layout || !layout.roots) return [];
  const z = cam.z || 1;
  function processRoot(root) {
    if (root && root.tree) {
      const rootRect = { x: root.x || 0, y: root.y || 0, w: root.w || 0, h: root.h || 0 };
      processNode(root.id || 'main', root.tree, rootRect);
    }
  }
  function processNode(paneId, node, parentRect) {
    if (!node) return;
    const rect = computeNodeRect(node, parentRect);
    if (rect && rect.w > 0 && rect.h > 0) {
      result.push({ paneId, rect: { ...rect } });
    }
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
  return parentRect ? { ...parentRect } : null;
}

/**
 * Handle palette pointer move.
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
 * Handle palette pointer up.
 */
function onPalettePointerUp(e) {
  if (!dragState || dragState.source !== 'palette' || e.pointerId !== pointerId) return;
  document.removeEventListener('pointermove', onPalettePointerMove);
  document.removeEventListener('pointerup', onPalettePointerUp);
  document.removeEventListener('pointercancel', onPalettePointerCancel);
  if (e.target.releasePointerCapture) e.target.releasePointerCapture(e.pointerId);
  hideDropOverlay();
  if (!dragState.hasMoved) {
    dragState.ctx.store.endGesture?.();
    dragState = null;
    pointerId = null;
    return;
  }
  const { store, camera } = dragState.ctx;
  const cam = camera.get();
  const worldX = (dragState.currentX - cam.x) / cam.z;
  const worldY = (dragState.currentY - cam.y) / cam.z;
  const layoutRects = computeLayoutRects(store.get(), camera.get());
  const target = targetAt(layoutRects, { x: worldX, y: worldY }, CONFIG.layout.dropEdgeFraction);
  let to;
  if (target) {
    switch (target.zone) {
      case 'left': case 'right': case 'top': case 'bottom':
        to = { beside: target.paneId, side: target.zone }; break;
      case 'center':
        to = { tabInto: target.paneId }; break;
      default:
        to = { newRoot: { x: worldX - 150, y: worldY - 100, w: 300, h: 200 } };
    }
  } else {
    to = { newRoot: { x: worldX - 150, y: worldY - 100, w: 300, h: 200 } };
  }
  paletteDrop(store, dragState.kind, to);
  dragState.ctx.store.endGesture?.();
  dragState = null;
  pointerId = null;
}

/**
 * Handle palette pointer cancel.
 */
function onPalettePointerCancel(e) {
  if (!dragState || dragState.source !== 'palette' || e.pointerId !== pointerId) return;
  document.removeEventListener('pointermove', onPalettePointerMove);
  document.removeEventListener('pointerup', onPalettePointerUp);
  document.removeEventListener('pointercancel', onPalettePointerCancel);
  hideDropOverlay();
  dragState.ctx.store.endGesture?.();
  dragState = null;
  pointerId = null;
}

/**
 * Install palette item drag listener.
 * @param {object} ctx - {ws, store, instances, camera, worldEl}
 */
export function installPaletteDrag(ctx) {
  document.addEventListener('pointerdown', (e) => {
    const paletteItem = e.target.closest('.palette-item');
    if (paletteItem) {
      const kind = paletteItem.dataset.kind;
      if (kind && e.button === 0) {
        dragState = {
          source: 'palette',
          kind,
          startX: e.clientX,
          startY: e.clientY,
          currentX: e.clientX,
          currentY: e.clientY,
          hasMoved: false,
          ctx,
          pointerId: e.pointerId
        };
        pointerId = e.pointerId;
        paletteItem.setPointerCapture(e.pointerId);
        document.addEventListener('pointermove', onPalettePointerMove);
        document.addEventListener('pointerup', onPalettePointerUp);
        document.addEventListener('pointercancel', onPalettePointerCancel);
        e.preventDefault();
      }
    }
  });
}
