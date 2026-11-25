/**
 * panes/script-editor.js — Script editor pane (M4.30).
 *
 * CM6 JS editor for wiring scripts and custom pane definitions.
 * Targets: `wiring:<name>` (minimal, standard, power) and `customPane:<kind>`.
 * Loads from storage or /js/defaults/wiring/<name>.js, saves via storage.put
 * then loader.reload(name). Syntax-checked via import() of Blob URL.
 *
 * In:  `layout` (value)
 * Out: {}
 * Call: {}
 */

import { valuePort, eventPort, callPort } from '../core/ports.js';

let CM = null;

async function loadCM() {
  if (CM) return CM;
  CM = {};
  try {
    const viewMod = await import('@codemirror/view');
    const stateMod = await import('@codemirror/state');
    const langMod = await import('@codemirror/language');
    CM.EditorView = viewMod.EditorView;
    CM.EditorState = stateMod.EditorState;
    CM.keymap = viewMod.keymap;
    CM.lineNumbers = langMod.lineNumbers;
    CM.highlightActiveLineGutter = langMod.highlightActiveLineGutter;
    CM.highlightSpecialChars = langMod.highlightSpecialChars;
    CM.history = langMod.history;
    CM.foldGutter = langMod.foldGutter;
    CM.indentOnInput = langMod.indentOnInput;
    CM.bracketMatching = langMod.bracketMatching;
    CM.closeBrackets = langMod.closeBrackets;
    CM.closeBracketsKeymap = langMod.closeBracketsKeymap;
    CM.defaultKeymap = langMod.defaultKeymap;
    CM.historyKeymap = langMod.historyKeymap;
    CM.searchKeymap = langMod.searchKeymap;
    CM.foldKeymap = langMod.foldKeymap;
    CM.indentWithTab = langMod.indentWithTab;
  } catch (err) {
    console.warn('CM6 not available, script-editor will use fallback:', err.message);
  }
  return CM;
}

export default {
  kind: 'script-editor',
  title: 'Script Editor',
  ports: {
    in: { layout: 'value' },
    out: {},
    call: {},
  },
  create(ctx) {
    const { ws, event } = ctx;
    let el = null;
    let container = null;
    let view = null;
    let targetSelect = null;
    let statusEl = null;
    let errorBanner = null;
    let currentTarget = 'wiring:standard';
    let currentCode = '';
    let isSafe = false;

    // Ports
    const portLayout = valuePort(null);

    portLayout.subscribe((layout) => {
      if (!layout) return;
      // Layout change could reload the editor
    });

    function esc(s) {
      return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
    }

    async function loadTarget(target) {
      currentTarget = target;
      if (targetSelect) targetSelect.value = target;
      if (errorBanner) errorBanner.style.display = 'none';

      // Try storage first
      let code = null;
      try {
        if (ws && ws.storage && typeof ws.storage.get === 'function') {
          code = await ws.storage.get(target);
        }
      } catch {}

      if (code == null) {
        // Fetch shipped source
        const name = target.replace(/^wiring:/, '');
        try {
          const res = await fetch('/js/defaults/wiring/' + name + '.js');
          if (res.ok) code = await res.text();
          else code = '// No shipped source for ' + target + '\n'; // eslint-disable-line no-useless-escape
        } catch {
          code = '// Failed to load ' + target + '\n';
        }
      }

      currentCode = code || '';
      if (view && view.state.doc.toString() !== currentCode) {
        view.dispatch({
          changes: { from: 0, to: view.state.doc.length, insert: currentCode },
        });
      }
      updateStatus();
    }

    async function saveTarget() {
      if (!view) return { ok: false, error: 'no editor' };
      const code = view.state.doc.toString();

      // Syntax check via import() of Blob URL
      try {
        const blob = new Blob([code], { type: 'application/javascript' });
        const url = URL.createObjectURL(blob);
        try {
          await import(url);
        } finally {
          URL.revokeObjectURL(url);
        }
      } catch (e) {
        if (errorBanner) {
          errorBanner.textContent = 'Syntax error: ' + esc(e.message);
          errorBanner.style.display = 'block';
        }
        return { ok: false, error: e.message };
      }

      if (errorBanner) errorBanner.style.display = 'none';

      // Save to storage
      try {
        if (ws && ws.storage && typeof ws.storage.put === 'function') {
          await ws.storage.put(currentTarget, code);
        }
      } catch {}

      currentCode = code;
      updateStatus();

      // Reload via loader if available
      if (ws && typeof ws.loader === 'object' && ws.loader.reload) {
        try {
          await ws.loader.reload(currentTarget);
        } catch {}
      }

      ws.emit('script:saved', { target: currentTarget });
      return { ok: true };
    }

    async function revertTarget() {
      if (!view) return { ok: false, error: 'no editor' };
      await loadTarget(currentTarget);
      return { ok: true };
    }

    function updateStatus() {
      if (!statusEl) return;
      statusEl.textContent = 'Script · ' + currentTarget + ' · ' + currentCode.length + ' chars';
    }

    function showSafeBanner() {
      if (!errorBanner) return;
      errorBanner.textContent = 'Safe mode: custom panes and wiring reloads are disabled.';
      errorBanner.style.display = 'block';
      errorBanner.style.background = 'rgba(var(--amber-rgb, 255, 165, 0), 0.1)';
      errorBanner.style.border = '1px solid var(--amber)';
      errorBanner.style.color = 'var(--amber)';
    }

    function mount(containerEl) {
      container = containerEl;
      el = document.createElement('div');
      el.className = 'script-editor-pane';
      el.style.cssText = `display: flex; flex-direction: column; height: 100%; font-size: 11px; font-family: monospace;`;

      // Header
      const header = document.createElement('div');
      header.className = 'script-header';
      header.style.cssText = `
        display: flex;
        align-items: center;
        gap: 6px;
        padding: 4px 8px;
        border-bottom: 1px solid var(--line);
      `;

      const title = document.createElement('div');
      title.textContent = 'Script Editor';
      title.style.cssText = 'font-weight: 600; font-size: 12px; color: var(--ink);';

      targetSelect = document.createElement('select');
      targetSelect.style.cssText = `margin-left: auto; padding: 2px 8px; font-size: 11px; background: var(--bg-editor); border: 1px solid var(--line); border-radius: 4px; color: var(--ink);`;
      targetSelect.innerHTML = `
        <option value="wiring:minimal">wiring:minimal</option>
        <option value="wiring:standard">wiring:standard</option>
        <option value="wiring:power">wiring:power</option>
        <option value="customPane:x-clock">customPane:x-clock</option>
      `;
      targetSelect.value = currentTarget;
      targetSelect.addEventListener('change', () => { loadTarget(targetSelect.value); });

      const saveBtn = document.createElement('button');
      saveBtn.textContent = 'Save';
      saveBtn.title = 'Save script and reload';
      saveBtn.style.cssText = `padding: 2px 10px; font-size: 11px; background: var(--cyan); border: none; border-radius: 4px; color: var(--ink); cursor: pointer; font-weight: 600;`;
      saveBtn.addEventListener('click', saveTarget);

      const revertBtn = document.createElement('button');
      revertBtn.textContent = 'Revert';
      revertBtn.title = 'Revert to shipped source';
      revertBtn.style.cssText = `padding: 2px 10px; font-size: 11px; background: var(--bg-inset); border: 1px solid var(--line); border-radius: 4px; color: var(--ink); cursor: pointer;`;
      revertBtn.addEventListener('click', revertTarget);

      header.append(title, targetSelect, saveBtn, revertBtn);
      el.appendChild(header);

      // Error banner
      errorBanner = document.createElement('div');
      errorBanner.className = 'script-error';
      errorBanner.style.cssText = `
        padding: 4px 8px;
        font-size: 10px;
        display: none;
      `;
      el.appendChild(errorBanner);

      // CM6 editor area
      const editorWrap = document.createElement('div');
      editorWrap.className = 'script-editor-wrap';
      editorWrap.style.cssText = `flex: 1; overflow: hidden; position: relative;`;
      el.appendChild(editorWrap);

      // Status bar
      statusEl = document.createElement('div');
      statusEl.className = 'script-status';
      statusEl.style.cssText = `
        display: flex;
        align-items: center;
        gap: 8px;
        padding: 2px 8px;
        font-size: 10px;
        font-family: monospace;
        border-top: 1px solid var(--line);
        background: var(--bg-panel);
      `;
      el.appendChild(statusEl);

      container.appendChild(el);

      // Safe mode check
      try {
        const params = new URLSearchParams(typeof location !== 'undefined' ? location.search : '');
        if (params.get('safe')) {
          isSafe = true;
          showSafeBanner();
        }
      } catch {}

      // Load initial target
      loadTarget(currentTarget);
    }

    return {
      mount,
      unmount() {
        if (view) {
          view.destroy();
          view = null;
        }
        if (el && el.parentNode) el.parentNode.removeChild(el);
        el = null;
        container = null;
        view = null;
        targetSelect = null;
        statusEl = null;
        errorBanner = null;
      },
      in: {
        layout: portLayout,
      },
      out: {},
      call: {},
    };
  },
};
