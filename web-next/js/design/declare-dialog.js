// declare-dialog.js — Declare HUD / modal roots (M3.05)
//
// Small chrome dialog (not a pane):
//   +HUD: fields id, dock (select), size (number) → hud.set with an empty tree
//   +Modal: fields id, open events (comma list), close events, persistent → modal.set
// Shows the store's error text inline on rejection (duplicate id, bad dock).
// Remove buttons dispatch hud.remove / modal.remove.

const DOCK_OPTIONS = ["top", "bottom", "left", "right"];

/**
 * Create the declare dialog for HUD/modal roots.
 * @param {object} opts
 * @param {HTMLElement} opts.root - Root DOM element to attach to
 * @param {object} opts.store - Layout store {dispatch(cmd), lastError()}
 * @returns {{destroy:()=>void, open:()=>void, close:()=>void, isOpen:()=>boolean}}
 */
export function createDeclareDialog(opts) {
  const { root, store } = opts;

  const overlay = document.createElement('div');
  overlay.className = 'declare-dialog-overlay';
  overlay.style.cssText =
    'position:fixed; top:0; left:0; right:0; bottom:0;'
    + 'background:rgba(0,0,0,0.5); z-index:2000;'
    + 'display:flex; align-items:center; justify-content:center;';

  const dialog = document.createElement('div');
  dialog.className = 'declare-dialog';
  dialog.style.cssText =
    'background:#1c2130; border:1px solid #3b465e; border-radius:6px;'
    + 'padding:16px; width:400px; box-sizing:border-box;'
    + 'color:#d6e2ee; font-family:sans-serif;';

  overlay.appendChild(dialog);
  root.appendChild(overlay);

  let open = false;
  let mode = 'hud'; // 'hud' | 'modal'

  // Title
  const title = document.createElement('h3');
  title.textContent = 'Declare HUD Root';
  title.style.cssText = 'margin:0 0 12px 0; font-size:16px; color:#ffffff;';
  dialog.appendChild(title);

  // Mode selector
  const modeLabel = document.createElement('label');
  modeLabel.textContent = 'Type:';
  modeLabel.style.cssText = 'display:block; margin-bottom:6px; font-size:12px; color:#9aa5b8;';
  const modeSelect = document.createElement('select');
  modeSelect.style.cssText = 'width:100%; margin-bottom:12px;';
  const hudOption = document.createElement('option');
  hudOption.value = 'hud';
  hudOption.textContent = '+HUD — docked root';
  const modalOption = document.createElement('option');
  modalOption.value = 'modal';
  modalOption.textContent = '+Modal — event-driven root';
  modeSelect.appendChild(hudOption);
  modeSelect.appendChild(modalOption);
  dialog.appendChild(modeLabel);
  dialog.appendChild(modeSelect);

  // id field
  const idLabel = document.createElement('label');
  idLabel.textContent = 'id:';
  idLabel.style.cssText = 'display:block; margin-bottom:4px; font-size:12px; color:#9aa5b8;';
  const idInput = document.createElement('input');
  idInput.type = 'text';
  idInput.placeholder = 'e.g. my-hud';
  idInput.style.cssText = 'width:100%; margin-bottom:12px;';
  dialog.appendChild(idLabel);
  dialog.appendChild(idInput);

  // dock field (HUD only)
  const dockLabel = document.createElement('label');
  dockLabel.textContent = 'dock:';
  dockLabel.style.cssText = 'display:block; margin-bottom:4px; font-size:12px; color:#9aa5b8;';
  const dockSelect = document.createElement('select');
  dockSelect.style.cssText = 'width:100%; margin-bottom:12px;';
  for (const d of DOCK_OPTIONS) {
    const o = document.createElement('option');
    o.value = d;
    o.textContent = d;
    dockSelect.appendChild(o);
  }
  dialog.appendChild(dockLabel);
  dialog.appendChild(dockSelect);

  // size field (HUD only)
  const sizeLabel = document.createElement('label');
  sizeLabel.textContent = 'size (px):';
  sizeLabel.style.cssText = 'display:block; margin-bottom:4px; font-size:12px; color:#9aa5b8;';
  const sizeInput = document.createElement('input');
  sizeInput.type = 'number';
  sizeInput.placeholder = '80';
  sizeInput.style.cssText = 'width:100%; margin-bottom:12px;';
  dialog.appendChild(sizeLabel);
  dialog.appendChild(sizeInput);

  // open events field (Modal only)
  const openLabel = document.createElement('label');
  openLabel.textContent = 'open events (comma list):';
  openLabel.style.cssText = 'display:block; margin-bottom:4px; font-size:12px; color:#9aa5b8;';
  const openInput = document.createElement('input');
  openInput.type = 'text';
  openInput.placeholder = 'e.g. show:modal';
  openInput.style.cssText = 'width:100%; margin-bottom:12px;';
  dialog.appendChild(openLabel);
  dialog.appendChild(openInput);

  // close events field (Modal only)
  const closeLabel = document.createElement('label');
  closeLabel.textContent = 'close events (comma list):';
  closeLabel.style.cssText = 'display:block; margin-bottom:4px; font-size:12px; color:#9aa5b8;';
  const closeInput = document.createElement('input');
  closeInput.type = 'text';
  closeInput.placeholder = 'e.g. hide:modal';
  closeInput.style.cssText = 'width:100%; margin-bottom:12px;';
  dialog.appendChild(closeLabel);
  dialog.appendChild(closeInput);

  // persistent field (Modal only)
  const persistentLabel = document.createElement('label');
  persistentLabel.textContent = 'persistent:';
  persistentLabel.style.cssText = 'display:block; margin-bottom:4px; font-size:12px; color:#9aa5b8;';
  const persistentInput = document.createElement('input');
  persistentInput.type = 'checkbox';
  persistentInput.style.cssText = 'margin-bottom:12px;';
  dialog.appendChild(persistentLabel);
  dialog.appendChild(persistentInput);

  // Error text (shown inline on rejection)
  const errorDiv = document.createElement('div');
  errorDiv.style.cssText =
    'color:#ff6b6b; font-size:11px; margin-bottom:12px; min-height:14px; display:none;';
  dialog.appendChild(errorDiv);

  // Buttons
  const btnBar = document.createElement('div');
  btnBar.style.cssText = 'display:flex; gap:8px; justify-content:flex-end;';

  const cancelBtn = document.createElement('button');
  cancelBtn.type = 'button';
  cancelBtn.textContent = 'Cancel';
  cancelBtn.style.cssText = 'padding:6px 12px;';

  const createBtn = document.createElement('button');
  createBtn.type = 'button';
  createBtn.textContent = 'Create';
  createBtn.style.cssText = 'padding:6px 12px; background:#4c8cff; color:#fff; border:none; border-radius:4px;';

  btnBar.appendChild(cancelBtn);
  btnBar.appendChild(createBtn);
  dialog.appendChild(btnBar);

  function showError(msg) {
    errorDiv.textContent = msg || '';
    errorDiv.style.display = msg ? 'block' : 'none';
  }

  function setMode(m) {
    mode = m;
    const isHud = m === 'hud';
    title.textContent = isHud ? 'Declare HUD Root' : 'Declare Modal Root';
    dockLabel.style.display = isHud ? 'block' : 'none';
    dockSelect.style.display = isHud ? 'block' : 'none';
    sizeLabel.style.display = isHud ? 'block' : 'none';
    sizeInput.style.display = isHud ? 'block' : 'none';
    openLabel.style.display = isHud ? 'none' : 'block';
    openInput.style.display = isHud ? 'none' : 'block';
    closeLabel.style.display = isHud ? 'none' : 'block';
    closeInput.style.display = isHud ? 'none' : 'block';
    persistentLabel.style.display = isHud ? 'none' : 'block';
    persistentInput.style.display = isHud ? 'none' : 'block';
    createBtn.textContent = isHud ? 'Create HUD' : 'Create Modal';
    showError('');
  }

  modeSelect.addEventListener('change', (e) => setMode(e.target.value));

  function close() {
    open = false;
    overlay.style.display = 'none';
    showError('');
  }

  cancelBtn.addEventListener('click', close);

  overlay.addEventListener('click', (e) => {
    if (e.target === overlay) close();
  });

  createBtn.addEventListener('click', () => {
    showError('');
    const id = idInput.value.trim();
    if (!id) { showError('id is required'); return; }

    if (mode === 'hud') {
      const dock = dockSelect.value;
      const size = Number(sizeInput.value);
      const result = store.dispatch({ type: 'hud.set', id, dock, size });
      if (!result.ok) { showError(result.error || 'failed'); return; }
    } else {
      const openEvents = openInput.value.split(',').map(s => s.trim()).filter(Boolean);
      const closeEvents = closeInput.value.split(',').map(s => s.trim()).filter(Boolean);
      const result = store.dispatch({
        type: 'modal.set',
        id,
        open: openEvents,
        close: closeEvents,
        persistent: persistentInput.checked,
      });
      if (!result.ok) { showError(result.error || 'failed'); return; }
    }

    idInput.value = '';
    openInput.value = '';
    closeInput.value = '';
    persistentInput.checked = false;
    close();
  });

  function openDialog() {
    open = true;
    overlay.style.display = 'flex';
    idInput.focus();
    setMode(mode);
  }

  function isOpen() { return open; }

  setMode('hud');
  overlay.style.display = 'none';

  return {
    destroy() {
      overlay.remove();
    },
    open: openDialog,
    close,
    isOpen,
  };
}

export function hudRemove(store, id) {
  if (!store || !id) return false;
  const result = store.dispatch({ type: 'hud.remove', id });
  return result.ok;
}

export function modalRemove(store, id) {
  if (!store || !id) return false;
  const result = store.dispatch({ type: 'modal.remove', id });
  return result.ok;
}

