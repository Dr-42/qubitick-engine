/**
 * pane-chrome.js — Pane chrome buttons for design mode (M3.03).
 *
 * When unlocked, adds buttons to pane heads:
 *   ⌖ Fit pane    — fly camera so the pane fills the viewport
 *   ⧉ Detach      — tree.move to a new root to the right
 *   ✕ Remove      — tree.remove + pane.remove (fully deletes the pane)
 *
 * Also adds ✕ buttons to tabs when unlocked.
 *
 * All buttons are hidden when locked.
 */

const BTN_CSS =
  'width:20px; height:20px; min-width:20px; min-height:20px;'
  + 'background:#2a2f3e; border:1px solid #3b465e; border-radius:4px;'
  + 'color:#c8d2ee; font-size:12px; cursor:pointer; display:inline-flex;'
  + 'align-items:center; justify-content:center; flex-shrink:0; margin-left:4px;';

const TAB_BTN_CSS =
  'width:16px; height:16px; min-width:16px; min-height:16px;'
  + 'background:#2a2f3e; border:1px solid #3b465e; border-radius:4px;'
  + 'color:#c8d2ee; font-size:10px; cursor:pointer; display:inline-flex;'
  + 'align-items:center; justify-content:center; flex-shrink:0; margin-left:4px;';

/**
 * Add chrome buttons to a pane head element.
 * Only adds buttons when lock.get() === false.
 * @param {HTMLElement} paneHeadEl - the .pane-head element
 * @param {string} paneId - pane id
 * @param {object} opts
 * @param {object} opts.lock - lock port {get:()=>boolean}
 * @param {object} opts.store - layout store {dispatch(cmd)}
 * @param {function} opts.onFit - (paneId) => void
 * @param {function} opts.onDetach - (paneId) => void
 * @param {function} opts.onRemove - (paneId) => void
 * @returns {{remove:()=>void}}
 */
export function addPaneChrome(paneHeadEl, paneId, opts) {
  const { lock, store, onFit, onDetach, onRemove } = opts;

  // Remove any existing chrome
  const existing = paneHeadEl.querySelector('.pane-chrome');
  if (existing) existing.remove();

  if (!lock || lock.get() !== false) return { remove: () => {} };

  const chrome = document.createElement('div');
  chrome.className = 'pane-chrome';
  chrome.style.cssText =
    'display:flex; align-items:center; gap:2px; margin-left:auto;';

  // Fit button
  const fitBtn = document.createElement('button');
  fitBtn.type = 'button';
  fitBtn.textContent = '⌖';
  fitBtn.title = 'Fit pane';
  fitBtn.style.cssText = BTN_CSS;
  fitBtn.addEventListener('click', (e) => {
    e.stopPropagation();
    if (onFit) onFit(paneId);
  });
  chrome.appendChild(fitBtn);

  // Detach button
  const detachBtn = document.createElement('button');
  detachBtn.type = 'button';
  detachBtn.textContent = '⧉';
  detachBtn.title = 'Detach to new root';
  detachBtn.style.cssText = BTN_CSS;
  detachBtn.addEventListener('click', (e) => {
    e.stopPropagation();
    if (onDetach) onDetach(paneId);
  });
  chrome.appendChild(detachBtn);

  // Remove button
  const removeBtn = document.createElement('button');
  removeBtn.type = 'button';
  removeBtn.textContent = '✕';
  removeBtn.title = 'Remove pane';
  removeBtn.style.cssText = BTN_CSS;
  removeBtn.addEventListener('click', (e) => {
    e.stopPropagation();
    if (onRemove) onRemove(paneId);
  });
  chrome.appendChild(removeBtn);

  paneHeadEl.appendChild(chrome);

  return {
    remove() {
      chrome.remove();
    },
  };
}

/**
 * Add a remove button to a tab element.
 * @param {HTMLElement} tabEl - the .qb-tab element
 * @param {string} paneId - pane id
 * @param {object} opts
 * @param {object} opts.lock - lock port
 * @param {function} opts.onRemove - (paneId) => void
 * @returns {{remove:()=>void}}
 */
export function addTabChrome(tabEl, paneId, opts) {
  const { lock, onRemove } = opts;

  if (!lock || lock.get() !== false) return { remove: () => {} };

  const existing = tabEl.querySelector('.tab-chrome');
  if (existing) existing.remove();

  const removeBtn = document.createElement('button');
  removeBtn.type = 'button';
  removeBtn.textContent = '✕';
  removeBtn.title = 'Remove tab';
  removeBtn.style.cssText = TAB_BTN_CSS;
  removeBtn.addEventListener('click', (e) => {
    e.stopPropagation();
    if (onRemove) onRemove(paneId);
  });
  tabEl.appendChild(removeBtn);

  return {
    remove() {
      removeBtn.remove();
    },
  };
}
