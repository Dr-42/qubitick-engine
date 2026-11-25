/**
 * renderer-modal.js — Modal rendering utilities (M2.07 Part A, B1-fixed).
 *
 * Modals are declared in layout.modals and are NOT visible on cold boot.
 * Each modal overlay is created (hidden) by the app's modal manager
 * (see main.js `createModalManager`); this renderer mounts the modal's pane
 * content inside that overlay. Visibility is controlled by the manager
 * (open/close/Esc/backdrop), never force-shown here.
 */

/**
 * Render modal content into an existing modal overlay.
 * @param {HTMLElement} worldEl
 * @param {object} modal
 * @param {object} layout
 * @param {Set} currentPaneIds
 * @param {function} renderTree
 * @returns {HTMLElement} the content wrapper (inside the overlay)
 */
export function renderModal(worldEl, modal, layout, currentPaneIds, renderTree) {
  const modalId = modal.id;

  // Reuse the overlay created by the modal manager (hidden by default).
  let overlay = null;
  const existing = document.querySelector('[data-root="modal"][data-modal="' + modalId + '"]')
    || document.querySelector('[data-root="modal"]');
  if (existing && existing.dataset.modal === modalId) {
    overlay = existing;
  } else {
    // Fallback: only used when the manager has not provided an overlay.
    overlay = existing || worldEl;
  }

  /* CLEAR THE WHOLE PREVIOUS WRAP, not just its content slot. Removing `.qb-modal-content`
     left the title bar and the wrap itself in place, so every render pass appended ANOTHER
     wrap: a modal with one pane held two copies of it after the first re-render, and a
     three-tab modal held six tabs. The modals are re-rendered on every pass like everything
     else, so this grew with every change. */
  overlay.querySelectorAll('.qb-modal-wrap').forEach((w) => w.remove());

  if (!modal.tree) return overlay;

  const wrap = document.createElement('div');
  wrap.className = 'qb-modal-wrap';
  wrap.style.cssText =
    'position:relative; width:min(90vw, 1000px); height:min(90vh, 700px);'
    + 'background:#12141e; border:1px solid #3b82f6; border-radius:8px;'
    + 'overflow:hidden; display:flex; flex-direction:column;';

  const titleBar = document.createElement('div');
  titleBar.className = 'qb-modal-title';
  titleBar.style.cssText =
    'padding:6px 10px; background:#1c2130; border-bottom:1px solid #2a2f3e;'
    + 'font-size:11px; color:#c8d2e0; flex-shrink:0; display:flex; justify-content:space-between; align-items:center;';
  titleBar.textContent = modal.title || ('modal: ' + modalId);
  const closeBtn = document.createElement('button');
  closeBtn.className = 'qb-modal-close';
  closeBtn.textContent = '✕';
  closeBtn.style.cssText =
    'background:none; border:none; color:#6b7280; font-size:16px; cursor:pointer; padding:0 4px;';
  closeBtn.addEventListener('click', (e) => {
    e.stopPropagation();
    document.dispatchEvent(new CustomEvent('ui:modal-close', { detail: { id: modalId } }));
  });
  titleBar.appendChild(closeBtn);
  wrap.appendChild(titleBar);

  const content = document.createElement('div');
  content.className = 'qb-modal-content';
  content.style.cssText = 'flex:1; overflow:hidden; position:relative;';
  wrap.appendChild(content);

  overlay.appendChild(wrap);
  renderTree(content, modal.tree, [{ modal: modal.id }], layout, currentPaneIds || new Set());
  return content;
}