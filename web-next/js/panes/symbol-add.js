/**
 * panes/symbol-add.js — Symbol-add pane (M4.24).
 *
 * Modal pane for creating new symbols. Provides an input field and
 * emits the created symbol on its `symbol` out port.
 *
 * In:  {}
 * Out: `symbol` (value `{name, kind, tex}`)
 * Call: `open()`
 */

import { valuePort, eventPort, callPort } from '../core/ports.js';

export default {
  kind: 'symbol-add',
  title: 'Add Symbol',
  ports: {
    in: {},
    out: { symbol: 'value' },
    call: { open: 'call' },
  },
  create(ctx) {
    const { ws, event } = ctx;
    let el = null;
    let container = null;
    let inputEl = null;
    let kindSelect = null;
    let texInput = null;
    let statusEl = null;

    // Ports
    const portSymbol = valuePort(null);
    const callOpen = callPort(doOpen);

    function esc(s) {
      return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
    }

    function doOpen() {
      if (!el) return { ok: false, error: 'not mounted' };
      el.style.display = 'flex';
      if (inputEl) inputEl.focus();
      return { ok: true };
    }

    function doClose() {
      if (!el) return;
      el.style.display = 'none';
      if (inputEl) inputEl.value = '';
      if (kindSelect) kindSelect.value = '';
      if (texInput) texInput.value = '';
      if (statusEl) statusEl.textContent = '';
    }

    function doCreate() {
      const name = inputEl ? inputEl.value.trim() : '';
      const kind = kindSelect ? kindSelect.value : '';
      const tex = texInput ? texInput.value.trim() : '';
      if (!name) {
        if (statusEl) statusEl.textContent = 'Name is required';
        return;
      }
      const symbol = { name, kind, tex };
      portSymbol.set(symbol);
      ws.emit('symbol:created', symbol);
      doClose();
    }

    function mount(containerEl) {
      container = containerEl;
      el = document.createElement('div');
      el.className = 'symbol-add-pane';
      el.style.cssText = `
        display: none;
        position: fixed;
        top: 50%; left: 50%;
        transform: translate(-50%, -50%);
        width: 420px;
        background: var(--bg-panel);
        border: 1px solid var(--line);
        border-radius: 8px;
        box-shadow: 0 4px 24px rgba(0,0,0,0.4);
        z-index: 1000;
        flex-direction: column;
        font-size: 11px;
        font-family: ui-sans-serif, system-ui, sans-serif;
      `;

      // Header
      const header = document.createElement('div');
      header.className = 'symbol-add-header';
      header.style.cssText = `
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 8px 12px;
        border-bottom: 1px solid var(--line);
        background: var(--bg-inset);
      `;
      const hTitle = document.createElement('div');
      hTitle.textContent = 'Add Symbol';
      hTitle.style.cssText = 'font-weight: 600; color: var(--ink);';
      const hClose = document.createElement('button');
      hClose.textContent = '✕';
      hClose.style.cssText = `background: none; border: none; color: var(--muted); cursor: pointer; font-size: 16px;`;
      hClose.addEventListener('click', doClose);
      header.append(hTitle, hClose);
      el.appendChild(header);

      // Form
      const form = document.createElement('div');
      form.className = 'symbol-add-form';
      form.style.cssText = 'padding: 12px; display: flex; flex-direction: column; gap: 8px;';

      const field = (label, elem) => {
        const row = document.createElement('div');
        row.style.cssText = 'display: flex; flex-direction: column; gap: 2px;';
        const lab = document.createElement('label');
        lab.textContent = label;
        lab.style.cssText = 'font-size: 10px; color: var(--muted);';
        elem.style.cssText = 'padding: 4px 6px; font-size: 11px; background: var(--bg-editor); border: 1px solid var(--line); border-radius: 4px; color: var(--ink);';
        row.append(lab, elem);
        form.appendChild(row);
      };

      inputEl = document.createElement('input');
      inputEl.type = 'text';
      inputEl.placeholder = 'e.g. ⊢'; // eslint-disable-line no-undef
      field('Symbol', inputEl);

      kindSelect = document.createElement('select');
      kindSelect.innerHTML = `<option value="">kind</option><option value="infix">infix</option><option value="prefix">prefix</option><option value="postfix">postfix</option><option value="binder">binder</option>`;
      field('Kind', kindSelect);

      texInput = document.createElement('input');
      texInput.type = 'text';
      texInput.placeholder = 'e.g. \\vdash'; // eslint-disable-line no-undef
      field('TeX', texInput);

      statusEl = document.createElement('div');
      statusEl.style.cssText = 'font-size: 9px; color: var(--red); height: 12px;';
      form.appendChild(statusEl);

      const btns = document.createElement('div');
      btns.style.cssText = 'display: flex; gap: 6px; margin-top: 8px;';

      const createBtn = document.createElement('button');
      createBtn.textContent = 'Create';
      createBtn.style.cssText = `padding: 4px 12px; background: var(--cyan); border: none; border-radius: 4px; color: var(--ink); cursor: pointer; font-weight: 600;`;
      createBtn.addEventListener('click', doCreate);

      const cancelBtn = document.createElement('button');
      cancelBtn.textContent = 'Cancel';
      cancelBtn.style.cssText = `padding: 4px 12px; background: var(--bg-inset); border: 1px solid var(--line); border-radius: 4px; color: var(--ink); cursor: pointer;`;
      cancelBtn.addEventListener('click', doClose);

      btns.append(createBtn, cancelBtn);
      form.appendChild(btns);

      el.appendChild(form);

      // Click outside to close
      el.addEventListener('click', (e) => {
        if (e.target === el) doClose();
      });

      container.appendChild(el);
    }

    return {
      mount,
      unmount() {
        if (el && el.parentNode) el.parentNode.removeChild(el);
        el = null;
        container = null;
        inputEl = null;
        kindSelect = null;
        texInput = null;
        statusEl = null;
      },
      in: {},
      out: {
        symbol: portSymbol,
      },
      call: {
        open: callOpen,
      },
    };
  },
};
