/**
 * overlay.js — Drop overlay visual feedback (M2.16).
 *
 * Shows a highlighted zone indicator while dragging.
 */

const OVERLAY_CLASS = 'qb-drop-overlay';
const ZONE_CLASS_PREFIX = 'qb-zone-';

let overlay = null;

/**
 * Create the drop overlay element.
 * @returns {HTMLElement}
 */
function createOverlayElement() {
  const el = document.createElement('div');
  el.className = OVERLAY_CLASS;
  el.style.cssText = `
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 3000;
    display: none;
  `;
  
  // Create zone indicators
  const zones = ['left', 'right', 'top', 'bottom', 'center', 'newroot'];
  zones.forEach(zone => {
    const zoneEl = document.createElement('div');
    zoneEl.className = `${ZONE_CLASS_PREFIX}${zone}`;
    zoneEl.style.cssText = `
      position: absolute;
      pointer-events: none;
      opacity: 0;
      transition: opacity 100ms ease;
    `;
    el.appendChild(zoneEl);
  });
  
  return el;
}

/**
 * Show the overlay with a specific zone highlighted.
 * @param {HTMLElement} worldEl - The world element
 * @param {string} zone - Zone name: 'left'|'right'|'top'|'bottom'|'center'|'newroot'
 * @param {object} rect - {x, y, w, h} in world coordinates
 * @param {number} zoom - Camera zoom level
 */
export function showDropOverlay(worldEl, zone, rect, zoom = 1) {
  if (!overlay) {
    overlay = createOverlayElement();
    overlay.style.display = 'block';
    document.querySelector('.world')?.appendChild(overlay);
  }
  
  // Hide all zones first
  overlay.querySelectorAll('[class^="qb-zone-"]').forEach(el => {
    el.style.opacity = '0';
  });
  
  // Show the target zone
  const zoneEl = overlay.querySelector(`.qb-zone-${zone}`);
  if (zoneEl && rect) {
    // Convert world coordinates to screen coordinates
    const screenX = rect.x * zoom;
    const screenY = rect.y * zoom;
    const screenW = rect.w * zoom;
    const screenH = rect.h * zoom;
    
    let zoneStyle = '';
    switch (zone) {
      case 'left':
        zoneStyle = `left: ${rect.x}px; top: 0; width: ${rect.w * 0.25}px; height: 100%;`;
        break;
      case 'right':
        zoneStyle = `right: ${rect.w * 0.25}px; top: 0; width: ${rect.w * 0.25}px; height: 100%;`;
        break;
      case 'top':
        zoneStyle = `top: 0; left: 0; width: 100%; height: ${rect.h * 0.25}px;`;
        break;
      case 'bottom':
        zoneStyle = `bottom: ${rect.h * 0.25}px; left: 0; width: 100%; height: ${rect.h * 0.25}px;`;
        break;
      case 'center':
        zoneStyle = `left: ${rect.w * 0.25}px; top: ${rect.h * 0.25}px; width: ${rect.w * 0.5}px; height: ${rect.h * 0.5}px;`;
        break;
      case 'newroot':
        zoneStyle = `left: ${rect.x}px; top: ${rect.y}px; width: ${rect.w}px; height: ${rect.h}px;`;
        break;
    }
    
    zoneEl.style.cssText = `
      position: absolute;
      pointer-events: none;
      background: rgba(59, 130, 246, 0.3);
      border: 2px solid #3b82f6;
      border-radius: 4px;
      ${zoneStyle}
      opacity: 1;
      transition: opacity 100ms ease;
    `;
    overlay.style.display = 'block';
  }
}

/**
 * Hide the drop overlay.
 */
export function hideDropOverlay() {
  if (overlay) {
    overlay.style.display = 'none';
    overlay.querySelectorAll('[class^="qb-zone-"]').forEach(el => {
      el.style.opacity = '0';
    });
  }
}

/**
 * Update overlay position during drag.
 * @param {object} rect - {x, y, w, h} in world coordinates
 * @param {number} zoom - Camera zoom level
 */
export function updateDropOverlay(rect, zoom = 1) {
  if (!overlay) return;
  
  // Update all visible zone elements
  overlay.querySelectorAll('[class^="qb-zone-"]').forEach(el => {
    if (el.style.opacity !== '0') {
      // Recalculate positions based on current rect
      // This is handled by showDropOverlay when zone changes
    }
  });
}
