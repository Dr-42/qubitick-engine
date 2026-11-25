/**
 * lod.js — Pure LOD (Level of Detail) calculations (M2.03).
 *
 * Hysteresis table from §4:
 *   live  <-> frozen: m=190 (down=0.9 → 171, up=1.12 → 212.8)
 *   frozen <-> card: m=110 (down=0.9 → 99, up=1.12 → 123.2)
 *   card  <-> culled: viewport culling with margin
 *
 * Levels: 'live' | 'frozen' | 'card' | 'culled'
 */

import { CONFIG } from '../core/config.js';

const { cardBelow, frozenBelow, zoomRef, up, down, cullMarginPx } = CONFIG.lod;

// Hysteresis thresholds
const LIVE_TO_FROZEN = frozenBelow * down;    // 190 * 0.9 = 171 (zooming OUT)
const FROZEN_TO_LIVE = frozenBelow * up;      // 190 * 1.12 = 212.8 (zooming IN)
const FROZEN_TO_CARD = cardBelow * down;      // 110 * 0.9 = 99 (zooming OUT)
const CARD_TO_FROZEN = cardBelow * up;        // 110 * 1.12 = 123.2 (zooming IN)

/**
 * LOD metric = min(screenW, screenH, z * zoomRef)
 * @param {number} screenW
 * @param {number} screenH
 * @param {number} z
 * @returns {number}
 */
export function lodMetric(screenW, screenH, z) {
  return Math.min(screenW, screenH, z * zoomRef);
}

/**
 * Determine next LOD level with hysteresis.
 * @param {'live'|'frozen'|'card'|'culled'} prev
 * @param {number} m - lodMetric value
 * @returns {'live'|'frozen'|'card'|'culled'}
 */
export function nextLevel(prev, m) {
  // Culled is handled separately by isCulled
  if (prev === 'culled') return 'culled';
  
  switch (prev) {
    case 'live':
      // Zooming out: live -> frozen when m drops below 171
      if (m < LIVE_TO_FROZEN) return 'frozen';
      return 'live';
    
    case 'frozen':
      // Zooming in: frozen -> live when m rises above 212.8
      if (m >= FROZEN_TO_LIVE) return 'live';
      // Zooming out: frozen -> card when m drops below 99
      if (m < FROZEN_TO_CARD) return 'card';
      return 'frozen';
    
    case 'card':
      // Zooming in: card -> frozen when m rises above 123.2
      if (m >= CARD_TO_FROZEN) return 'frozen';
      return 'card';
    
    default:
      return 'card';
  }
}

/**
 * Check if rect is culled (fully outside viewport + margin).
 * @param {{x:number,y:number,w:number,h:number}} screenRect
 * @param {{x:number,y:number,w:number,h:number}} viewportRect
 * @param {number} [margin=cullMarginPx]
 * @returns {boolean}
 */
export function isCulled(screenRect, viewportRect, margin = cullMarginPx) {
  if (!screenRect || !viewportRect) return false;
  
  // Rect is culled if entirely outside viewport with margin
  // Left: rect right edge + margin < viewport left
  // Right: rect left edge - margin > viewport right
  // Top: rect bottom edge + margin < viewport top
  // Bottom: rect top edge - margin > viewport bottom
  return screenRect.x + screenRect.w + margin < viewportRect.x ||
         screenRect.x - margin > viewportRect.x + viewportRect.w ||
         screenRect.y + screenRect.h + margin < viewportRect.y ||
         screenRect.y - margin > viewportRect.y + viewportRect.h;
}

/**
 * Determine LOD level combining metric and culling.
 * @param {object} opts
 * @param {'live'|'frozen'|'card'|'culled'} [opts.prev] - previous non-culled level
 * @param {{x:number,y:number,w:number,h:number}} opts.rect - screen rect
 * @param {{x:number,y:number,z:number}} opts.cam - camera state
 * @param {{w:number,h:number}} opts.viewport - viewport size
 * @returns {{level: 'live'|'frozen'|'card'|'culled', culled: boolean}}
 */
export function levelFor({ prev, rect, cam, viewport }) {
  // Check culling first
  const viewportRect = { x: 0, y: 0, w: viewport.w, h: viewport.h };
  const culled = isCulled(rect, viewportRect);
  
  if (culled) {
    return { level: 'culled', culled: true };
  }
  
  // Compute metric
  const m = lodMetric(viewport.w, viewport.h, cam.z);
  
  // If coming back from culled, use 'live' as previous
  const effectivePrev = prev === 'culled' || !prev ? 'live' : prev;
  const level = nextLevel(effectivePrev, m);
  
  return { level, culled: false };
}
