/**
 * panes/settings-layout.js — Settings Layout pane (M4.27).
 *
 * Layout preference editor (sidebar width, pane gap, compact mode).
 *
 * In:  {}
 * Out: `prefs` (value `{sidebarWidth, paneGap, compact}`)
 * Call: {}
 */

import { valuePort, eventPort, callPort } from '../core/ports.js';
import { resetScrollConfig, DEFAULT_SCROLL_CONFIG } from '../defaults/scroll-config.js';

export default {
  kind: 'settings-layout',
  title: 'Layout',
  ports: {
    in: {},
    out: { prefs: 'value' },
    call: {},
  },
  create(ctx) {
    const { ws, event } = ctx;
    let el = null;
    let container = null;
    let widthSelect = null;
    let gapSelect = null;
    let compactCb = null;

    // Ports
    const portPrefs = valuePort({ mode: 'standard', screen: 'desktop', scrollNotchPx: 96 });

    function emitPrefs() {
      const prev = portPrefs.get() || {};
      const prefs = {
        mode: prev.mode ?? 'standard',
        screen: prev.screen ?? 'desktop',
        scrollNotchPx: prev.scrollNotchPx ?? 96,
        sidebarWidth: widthSelect ? widthSelect.value : '280px',
        paneGap: gapSelect ? gapSelect.value : '8px',
        compact: compactCb ? compactCb.checked : false,
      };
      portPrefs.set(prefs);
      ws.emit('prefs:layout', prefs);
    }

    function mount(containerEl) {
      container = containerEl;
      el = document.createElement('div');
      el.className = 'settings-layout-pane';
      el.style.cssText = `display: flex; flex-direction: column; height: 100%; font-size: 11px; font-family: ui-sans-serif, system-ui, sans-serif;`;

      const header = document.createElement('div');
      header.style.cssText = `padding: 8px 12px; border-bottom: 1px solid var(--line); background: var(--bg-inset);`;
      header.textContent = 'Layout';
      el.appendChild(header);

      const body = document.createElement('div');
      body.style.cssText = `padding: 12px; display: flex; flex-direction: column; gap: 10px; overflow-y: auto;`;

      const row = (label, elem) => {
        const r = document.createElement('div');
        r.style.cssText = 'display: flex; align-items: center; gap: 8px;';
        r.textContent = label + ':';
        elem.style.cssText = 'padding: 4px 8px; font-size: 11px; background: var(--bg-editor); border: 1px solid var(--line); border-radius: 4px; color: var(--ink); flex: 1;';
        r.appendChild(elem);
        body.appendChild(r);
      };

      widthSelect = document.createElement('select');
      widthSelect.innerHTML = '<option value="240px">240px</option><option value="280px">280px</option><option value="320px">320px</option><option value="360px">360px</option>';
      widthSelect.value = portPrefs.get()?.sidebarWidth || '280px';
      widthSelect.addEventListener('change', emitPrefs);
      row('Sidebar width', widthSelect);

      gapSelect = document.createElement('select');
      gapSelect.innerHTML = '<option value="4px">4px</option><option value="8px">8px</option><option value="12px">12px</option><option value="16px">16px</option>';
      gapSelect.value = portPrefs.get()?.paneGap || '8px';
      gapSelect.addEventListener('change', emitPrefs);
      row('Pane gap', gapSelect);

      const compactRow = document.createElement('div');
      compactRow.style.cssText = 'display: flex; align-items: center; gap: 8px;';
      compactRow.textContent = 'Compact mode:';
      compactCb = document.createElement('input');
      compactCb.type = 'checkbox';
      compactCb.checked = portPrefs.get()?.compact || false;
      compactCb.style.cssText = 'width: 16px; height: 16px;';
      compactCb.addEventListener('change', emitPrefs);
      compactRow.appendChild(compactCb);
      body.appendChild(compactRow);

      // Reset scroll defaults button
      const resetRow = document.createElement('div');
      resetRow.style.cssText = 'display: flex; align-items: center; gap: 8px; border-top: 1px solid var(--line); padding-top: 8px;';
      resetRow.textContent = 'Reset scroll defaults:';
      const resetBtn = document.createElement('button');
      resetBtn.textContent = 'Reset';
      resetBtn.title = 'Reset scroll tuning to defaults';
      resetBtn.style.cssText = 'padding: 4px 12px; font-size: 11px; background: var(--bg-inset); border: 1px solid var(--line); border-radius: 4px; color: var(--ink); cursor: pointer;';
      resetBtn.addEventListener('click', function() {
        resetScrollConfig(ws.storage);
        portPrefs.set({ ...portPrefs.get(), scrollNotchPx: DEFAULT_SCROLL_CONFIG.notchPx });
        emitPrefs();
      });
      resetRow.appendChild(resetBtn);
      body.appendChild(resetRow);

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
        widthSelect = null;
        gapSelect = null;
        compactCb = null;
      },
      in: {},
      out: {
        prefs: portPrefs,
      },
      call: {},
    };
  },
};
