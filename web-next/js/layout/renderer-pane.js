/**
 * renderer-pane.js — Pane rendering utilities (M2.07 Part A).
 *
 * Handles pane element creation, mounting, unmounting, and LOD management.
 */

// CSS class prefixes
const PANE_PREFIX = 'qb-pane-';
const PANE_HEAD_PREFIX = 'qb-pane-head-';
const PANE_BODY_PREFIX = 'qb-pane-body-';
const TAB_BAR_PX = 24;

/**
 * Create or get a pane element.
 * @param {string} paneId
 * @param {object} paneDef
 * @param {object} instances - instance manager
 * @param {HTMLElement} containerEl
 * @returns {object} { paneEl, paneHeadEl, paneBodyEl, isNew }
 */
export function getOrCreatePaneElement(paneId, paneDef, instances, containerEl) {
  // This will be called from the main renderer
  // For now, return null to indicate the main renderer should handle it
  return null;
}

/**
 * Create a new pane element with header and body.
 * @param {string} paneId
 * @param {object} paneDef
 * @returns {object} { paneEl, paneHeadEl, paneBodyEl }
 */
export function createPaneElement(paneId, paneDef) {
  const title = paneDef.title || paneDef.kind;

  // Create pane element
  const paneEl = document.createElement('div');
  paneEl.className = 'qb-pane-' + paneId;
  paneEl.setAttribute('data-pane', paneId);
  paneEl.style.cssText =
    'position:absolute; inset:0; overflow:hidden; display:flex; flex-direction:column; background:#12141e; border:1px solid #2a2f3e; border-radius:4px;';

  // Pane header
  const paneHeadEl = document.createElement('div');
  paneHeadEl.className = 'qb-pane-head-' + paneId;
  paneHeadEl.style.cssText =
    'height:24px; display:flex; align-items:center; padding:0 6px;'
    + 'background:#1c2130; border-bottom:1px solid #2a2f3e;'
    + 'font-size:11px; color:#c8d2e0; font-weight:500; flex-shrink:0;'
    + 'user-select:none;';
  paneHeadEl.textContent = title;

  // Pane body
  const paneBodyEl = document.createElement('div');
  paneBodyEl.className = 'qb-pane-body-' + paneId;
  paneBodyEl.style.cssText = 'flex:1; overflow:hidden; position:relative; width:100%; height:calc(100% - 24px);';

  paneEl.appendChild(paneHeadEl);
  paneEl.appendChild(paneBodyEl);

  return { paneEl, paneHeadEl, paneBodyEl };
}

/**
 * Update pane element LOD (level of detail).
 * @param {HTMLElement} paneEl
 * @param {string} lod - 'full' | 'card' | 'culled'
 */
export function updatePaneLOD(paneEl, lod) {
  if (!paneEl) return;
  paneEl.dataset.lod = lod;
  const bodyEl = paneEl.querySelector('[class^="qb-pane-body-"]');
  if (bodyEl) {
    if (lod === 'culled') {
      bodyEl.style.display = 'none';
    } else {
      bodyEl.style.display = '';
    }
  }
}

/**
 * Update pane element position and size.
 * @param {HTMLElement} paneEl
 * @param {object} rect - { left, top, width, height }
 */
export function updatePaneRect(paneEl, rect) {
  if (!paneEl) return;
  const { left, top, width, height } = rect;
  paneEl.style.left = Math.round(left) + 'px';
  paneEl.style.top = Math.round(top) + 'px';
  paneEl.style.width = Math.round(width) + 'px';
  paneEl.style.height = Math.round(height) + 'px';
}
