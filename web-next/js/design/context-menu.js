/**
 * context-menu.js — Reusable screen-space context menu (M3.03).
 *
 * Opens on right-click (or long-press 500 ms on touch) positioned in
 * screen space. Closes on outside pointerdown or Esc.
 *
 * Items are specified as {label, action, disabled, rootOnly} where
 * rootOnly items only appear for pane roots.
 *
 * @example
 *   const menu = createContextMenu({ root, onAction });
 *   menu.open({ x, y, items, rootOnly });
 */

import { clamp } from '../lib/math.js';

const MENU_CSS =
  'position:fixed; z-index:2000; background:#1c2130; border:1px solid #3b465e;'
  + 'border-radius:4px; box-shadow:0 4px 12px rgba(0,0,0,0.4);'
  + 'font-size:11px; color:#c8d2ee; min-width:160px;';

const ITEM_CSS =
  'display:flex; align-items:center; gap:8px; padding:4px 10px;'
  + 'cursor:pointer; border:none; background:transparent; text-align:left;'
  + 'color:#c8d2ee; width:100%;';

const ITEM_HOVER_CSS =
  'background:#2a2f3e;';

const ITEM_DISABLED_CSS =
  'color:#6b7280; cursor:not-allowed;';

/**
 * Create a context menu.
 * @param {object} opts
 * @param {HTMLElement} opts.root - root element to attach menu to
 * @param {function} opts.onAction - (action: string) => void
 * @returns {{open:(items, x, y, rootOnly)=>void, close:()=>void, destroy:()=>void}}
 */
export function createContextMenu({ root, onAction }) {
  let menuEl = null;
  let isOpen = false;

  function createMenu() {
    const el = document.createElement('div');
    el.className = 'context-menu';
    el.style.cssText = MENU_CSS;
    el.style.display = 'none';
    root.appendChild(el);
    return el;
  }

  function ensureMenu() {
    if (!menuEl) menuEl = createMenu();
    return menuEl;
  }

  function close() {
    if (menuEl) {
      menuEl.style.display = 'none';
      menuEl.innerHTML = '';
    }
    isOpen = false;
  }

  function open(items, x, y, rootOnly) {
    const el = ensureMenu();
    el.innerHTML = '';

    let hasVisible = false;
    for (const item of items) {
      if (item.rootOnly && !rootOnly) continue;
      if (item.separator) {
        const sep = document.createElement('div');
        sep.style.cssText = 'height:1px; background:#3b465e; margin:4px 0;';
        el.appendChild(sep);
        continue;
      }
      if (item.disabled) continue;

      const btn = document.createElement('button');
      btn.type = 'button';
      btn.textContent = item.label;
      btn.style.cssText = ITEM_CSS;
      btn.addEventListener('click', (e) => {
        e.stopPropagation();
        e.preventDefault();
        close();
        if (item.action) onAction({ action: item.action, clientX: e.clientX, clientY: e.clientY });
      });
      btn.addEventListener('mouseenter', () => {
        btn.style.cssText = ITEM_CSS + ITEM_HOVER_CSS;
      });
      btn.addEventListener('mouseleave', () => {
        btn.style.cssText = ITEM_CSS;
      });
      el.appendChild(btn);
      hasVisible = true;
    }

    if (!hasVisible) {
      close();
      return;
    }

    // Clamp to viewport
    const rect = el.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    let left = clamp(x, 0, vw - rect.width - 8);
    let top = clamp(y, 0, vh - rect.height - 8);
    el.style.left = left + 'px';
    el.style.top = top + 'px';
    el.style.display = 'block';
    isOpen = true;
  }

  function onOutsidePointerDown(e) {
    if (isOpen && menuEl && !menuEl.contains(e.target)) {
      close();
    }
  }

  function onKeyDown(e) {
    if (isOpen && e.key === 'Escape') {
      close();
    }
  }

  document.addEventListener('pointerdown', onOutsidePointerDown, true);
  document.addEventListener('keydown', onKeyDown);

  return {
    open,
    close,
    destroy() {
      close();
      document.removeEventListener('pointerdown', onOutsidePointerDown, true);
      document.removeEventListener('keydown', onKeyDown);
      if (menuEl) menuEl.remove();
      menuEl = null;
    },
  };
}

