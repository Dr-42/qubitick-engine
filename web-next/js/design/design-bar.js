/**
 * design-bar.js — Design-mode chrome bar (M3.01).
 *
 * Creates a screen-space `.design-bar` (fixed bottom-centre, above HUD;
 * safe-area aware) with buttons:
 *   lock/unlock (🔒/🔓, aria-pressed), Undo, Redo (disabled when
 *   !canUndo/!canRedo), Palette, Panic Reset, +HUD, +Modal.
 *
 * Visible only when unlocked or when hovering the bottom-left 24 px
 * "design handle" (so locked UI stays clean). The lock button is always
 * reachable via Alt+D (ui:design-toggle). Sets body[data-design].
 * All buttons ≥ 44 px on (pointer: coarse).
 */

/**
 * Create and install the design bar.
 * @param {object} opts
 * @param {HTMLElement} opts.root - Root DOM element to attach to
 * @param {object} opts.ws - Workspace API (ws.lock, ws.store)
 * @returns {{destroy: ()=>void}}
 */
export function createDesignBar({ root, ws }) {
  const { lock, store } = ws;

  // Create the design bar
  const bar = document.createElement('div');
  bar.className = 'design-bar';
  bar.style.cssText =
    'position:fixed; left:0; right:0; bottom:0;'
    + 'display:flex; align-items:center; gap:6px; padding:4px 8px;'
    + 'background:#1c2130; border-top:1px solid #2a2f3e;'
    + 'z-index:1500; opacity:0; pointer-events:none; transition:opacity 0.2s;'
    + 'padding-bottom:env(safe-area-inset-bottom, 0);';

  // Design handle — bottom-left 24px strip that reveals the bar on hover
  const handle = document.createElement('div');
  handle.className = 'design-handle';
  handle.style.cssText =
    'position:fixed; left:0; bottom:0; width:24px; height:24px;'
    + 'cursor:pointer; z-index:1600;';

  // Button helper
  function button(label, title, onClick, ariaPressed) {
    const b = document.createElement('button');
    b.type = 'button';
    b.textContent = label;
    b.title = title;
    b.style.cssText =
      'width:44px; height:44px; min-width:44px; min-height:44px;'
      + 'background:#2a2f3e; border:1px solid #3b465e; border-radius:6px;'
      + 'color:#d6e2ee; font-size:16px; cursor:pointer; display:flex;'
      + 'align-items:center; justify-content:center; flex-shrink:0;';
    if (ariaPressed !== undefined) b.setAttribute('aria-pressed', String(ariaPressed));
    b.addEventListener('click', (e) => { e.stopPropagation(); onClick(); });
    return b;
  }

  // Lock / unlock button
  const lockBtn = button(
    lock.get() ? '🔒' : '🔓',
    lock.get() ? 'Unlock (Alt+D)' : 'Lock (Alt+D)',
    () => {
      lock.toggle();
      update();
    },
    !lock.get()
  );

  // Undo button
  const undoBtn = button('↩', 'Undo', () => {
    if (store && typeof store.undo === 'function') store.undo();
  }, false);
  undoBtn.disabled = !(store && typeof store.canUndo === 'function' && store.canUndo());

  // Redo button
  const redoBtn = button('↪', 'Redo', () => {
    if (store && typeof store.redo === 'function') store.redo();
  }, false);
  redoBtn.disabled = !(store && typeof store.canRedo === 'function' && store.canRedo());

  // Palette button (M3.04)
  const paletteBtn = button('🎨', 'Palette (Alt+P)', () => {
    // Palette opened by M3.04
    const evt = new CustomEvent('ui:palette-toggle', { bubbles: true });
    root.dispatchEvent(evt);
  }, false);

  // Panic Reset button
  const panicBtn = button('⚠', 'Panic Reset', () => {
    const evt = new CustomEvent('ui:panic-reset', { bubbles: true });
    root.dispatchEvent(evt);
  }, false);

  // +HUD button (M3.05)
  const hudBtn = button('+HUD', 'Add HUD root', () => {
    const evt = new CustomEvent('ui:add-hud', { bubbles: true });
    root.dispatchEvent(evt);
  }, false);

  // +Modal button (M3.05)
  const modalBtn = button('+Modal', 'Add modal root', () => {
    const evt = new CustomEvent('ui:add-modal', { bubbles: true });
    root.dispatchEvent(evt);
  }, false);

  bar.appendChild(lockBtn);
  bar.appendChild(undoBtn);
  bar.appendChild(redoBtn);
  bar.appendChild(paletteBtn);
  bar.appendChild(panicBtn);
  bar.appendChild(hudBtn);
  bar.appendChild(modalBtn);

  root.appendChild(handle);
  root.appendChild(bar);

  function update() {
    const unlocked = !lock.get();
    document.documentElement.setAttribute('data-design', unlocked ? '1' : '0');
    document.body.setAttribute('data-design', unlocked ? '1' : '0');

    lockBtn.textContent = unlocked ? '🔓' : '🔒';
    lockBtn.title = unlocked ? 'Lock (Alt+D)' : 'Unlock (Alt+D)';
    lockBtn.setAttribute('aria-pressed', String(unlocked));

    // Show bar when unlocked or handle hovered
    const show = unlocked || bar.matches(':hover') || handle.matches(':hover');
    bar.style.opacity = show ? '1' : '0';
    bar.style.pointerEvents = show ? 'auto' : 'none';

    // Update undo/redo enabled state
    if (store) {
      if (typeof store.canUndo === 'function') {
        undoBtn.disabled = !store.canUndo();
      }
      if (typeof store.canRedo === 'function') {
        redoBtn.disabled = !store.canRedo();
      }
    }
  }

  // Subscribe to lock changes
  const unsubLock = lock.subscribe(update);

  // Subscribe to store changes for undo/redo state
  let unsubStore = () => {};
  if (store && typeof store.subscribe === 'function') {
    unsubStore = store.subscribe(() => {
      updateUndoRedo();
    });
  }

  function updateUndoRedo() {
    if (store) {
      if (typeof store.canUndo === 'function') undoBtn.disabled = !store.canUndo();
      if (typeof store.canRedo === 'function') redoBtn.disabled = !store.canRedo();
    }
  }

  // Alt+D toggles lock
  function onKeyDown(e) {
    if (e.altKey && e.key.toLowerCase() === 'd') {
      e.preventDefault();
      lock.toggle();
      update();
    }
  }
  document.addEventListener('keydown', onKeyDown);

  // Handle hover
  handle.addEventListener('mouseenter', update);
  handle.addEventListener('mouseleave', update);
  bar.addEventListener('mouseenter', update);
  bar.addEventListener('mouseleave', update);

  // Initial update
  update();

  return {
    destroy() {
      unsubLock();
      unsubStore();
      document.removeEventListener('keydown', onKeyDown);
      handle.remove();
      bar.remove();
    },
  };
}
