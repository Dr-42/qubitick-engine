/**
 * panes/settings-typography.js — Settings Typography pane (M4.26).
 *
 * Typography preference editor (font family, size, line height).
 *
 * In:  {}
 * Out: `prefs` (value `{fontFamily, fontSize, lineHeight}`)
 * Call: {}
 */

import { valuePort, eventPort, callPort } from '../core/ports.js';

export default {
  kind: 'settings-typography',
  title: 'Typography',
  ports: {
    in: {},
    out: { prefs: 'value' },
    call: {},
  },
  create(ctx) {
    const { ws, event } = ctx;
    let el = null;
    let container = null;
    let fontSelect = null;
    let sizeSelect = null;

    // Ports
    const portPrefs = valuePort({ fontFamily: 'ui-monospace', fontSize: 13, lineHeight: 1.5 });

    function emitPrefs() {
      const prev = portPrefs.get() || {};
      const prefs = {
        fontFamily: fontSelect ? fontSelect.value : 'mono',
        fontSize: sizeSelect ? sizeSelect.value : '11px',
        lineHeight: prev.lineHeight ?? 1.5,
      };
      portPrefs.set(prefs);
      ws.emit('prefs:typography', prefs);
    }

    function mount(containerEl) {
      container = containerEl;
      el = document.createElement('div');
      el.className = 'settings-typography-pane';
      el.style.cssText = `display: flex; flex-direction: column; height: 100%; font-size: 11px; font-family: ui-sans-serif, system-ui, sans-serif;`;

      const header = document.createElement('div');
      header.style.cssText = `padding: 8px 12px; border-bottom: 1px solid var(--line); background: var(--bg-inset);`;
      header.textContent = 'Typography';
      el.appendChild(header);

      const body = document.createElement('div');
      body.style.cssText = `padding: 12px; display: flex; flex-direction: column; gap: 8px; overflow-y: auto;`;

      const row = (label, elem) => {
        const r = document.createElement('div');
        r.style.cssText = 'display: flex; align-items: center; gap: 8px;';
        r.textContent = label + ':';
        elem.style.cssText = 'padding: 4px 8px; font-size: 11px; background: var(--bg-editor); border: 1px solid var(--line); border-radius: 4px; color: var(--ink); flex: 1;';
        r.appendChild(elem);
        body.appendChild(r);
      };

      fontSelect = document.createElement('select');
      fontSelect.innerHTML = '<option value="mono">Monospace</option><option value="sans">Sans Serif</option><option value="serif">Serif</option>';
      fontSelect.value = portPrefs.get()?.fontFamily || 'mono';
      fontSelect.addEventListener('change', emitPrefs);
      row('Font family', fontSelect);

      sizeSelect = document.createElement('select');
      sizeSelect.innerHTML = '<option value="10px">10px</option><option value="11px">11px</option><option value="12px">12px</option><option value="14px">14px</option>';
      sizeSelect.value = portPrefs.get()?.fontSize || '11px';
      sizeSelect.addEventListener('change', emitPrefs);
      row('Font size', sizeSelect);

      el.appendChild(body);

      container.appendChild(el);
      emitPrefs();
    }

    return {
      mount,
      unmount() {
        if (el && el.parentNode) el.parentNode.removeChild(el);
        el = null;
        container = null;
        fontSelect = null;
        sizeSelect = null;
      },
      in: {},
      out: {
        prefs: portPrefs,
      },
      call: {},
    };
  },
};
