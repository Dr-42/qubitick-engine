/**
 * panes/settings-theme.js — Settings Theme pane (M4.25).
 *
 * Theme preference editor. Emits theme prefs on `prefs` out port.
 *
 * In:  {}
 * Out: `prefs` (value `{theme: string}`)
 * Call: {}
 */

import { valuePort, eventPort, callPort } from '../core/ports.js';

export default {
  kind: 'settings-theme',
  title: 'Theme',
  ports: {
    in: {},
    out: { prefs: 'value' },
    call: {},
  },
  create(ctx) {
    const { ws, event } = ctx;
    let el = null;
    let container = null;
    let selectEl = null;

    // Ports
    const portPrefs = valuePort({ theme: 'dark', palette: 'default' });

    function emitPrefs(theme) {
      const prev = portPrefs.get() || {};
      const prefs = { theme, palette: prev.palette || 'default' };
      portPrefs.set(prefs);
      ws.emit('prefs:theme', prefs);
    }

    function mount(containerEl) {
      container = containerEl;
      el = document.createElement('div');
      el.className = 'settings-theme-pane';
      el.style.cssText = `display: flex; flex-direction: column; height: 100%; font-size: 11px; font-family: ui-sans-serif, system-ui, sans-serif;`;

      const header = document.createElement('div');
      header.style.cssText = `padding: 8px 12px; border-bottom: 1px solid var(--line); background: var(--bg-inset);`;
      header.textContent = 'Theme';
      el.appendChild(header);

      const body = document.createElement('div');
      body.style.cssText = `padding: 12px; display: flex; flex-direction: column; gap: 8px; overflow-y: auto;`;

      const row = document.createElement('div');
      row.style.cssText = 'display: flex; align-items: center; gap: 8px;';
      row.textContent = 'Theme:';

      selectEl = document.createElement('select');
      selectEl.innerHTML = '<option value="dark">Dark</option><option value="light">Light</option><option value="high-contrast">High Contrast</option>';
      selectEl.value = portPrefs.get()?.theme || 'dark';
      selectEl.style.cssText = 'padding: 4px 8px; font-size: 11px; background: var(--bg-editor); border: 1px solid var(--line); border-radius: 4px; color: var(--ink);';
      selectEl.addEventListener('change', () => emitPrefs(selectEl.value));

      row.append(selectEl);
      body.appendChild(row);
      el.appendChild(body);

      container.appendChild(el);
      emitPrefs(selectEl.value);
    }

    return {
      mount,
      unmount() {
        if (el && el.parentNode) el.parentNode.removeChild(el);
        el = null;
        container = null;
        selectEl = null;
      },
      in: {},
      out: {
        prefs: portPrefs,
      },
      call: {},
    };
  },
};
