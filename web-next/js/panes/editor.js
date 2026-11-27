/**
 * panes/editor.js — Editor pane part A (M4.10).
 *
 * CM6-based code editor with per-file buffers, file tabs, dirty tracking,
 * save via Mod-s, and persistence.
 *
 * Ported from app.js 618-680, 753-878, 1570-1650, 1763-1770, 3029-3082.
 *
 * In:  `file` (value path), `goto` (event `{line,col}|{pos}`)
 * Out: `buffer` (value text), `cursor` (value `{path,line,col}`), `dirty` (value bool), `vimMode` (value bool), `saved` (event path)
 * Call: `save`, `goto({line,col}|{pos})`
 */

import { valuePort, eventPort, callPort } from '../core/ports.js';

const LOADING_SENTINEL = ';- loading corpus…';

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
    CM.drawSelection = viewMod.drawSelection;
    CM.dropCursor = viewMod.dropCursor;
    CM.rectangularSelection = viewMod.rectangularSelection;
    CM.crosshairCursor = viewMod.crosshairCursor;
    CM.highlightSelectionMatches = viewMod.highlightSelectionMatches;
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
    console.warn('CM6 not available, editor will use fallback:', err.message);
  }
  return CM;
}

export default {
  kind: 'editor',
  title: 'Editor',
  ports: {
    in: { file: 'value', goto: 'event' },
    out: { buffer: 'value', cursor: 'value', dirty: 'value', vimMode: 'value', saved: 'event' },
    call: { save: 'call', goto: 'call' },
  },
  create(ctx) {
    const { ws, event } = ctx;
    let el = null;
    let view = null;
    let container = null;
    let statusEl = null;
    let tabsEl = null;
    let currentPath = '';
    const buffers = new Map(); // path -> {content, dirty}
    const openTabs = [];
    let pendingSave = null;

    // Ports
    const portFile = valuePort('');
    const portBuffer = valuePort('');
    const portCursor = valuePort({ path: '', line: 0, col: 0 });
    const portDirty = valuePort(false);
    const portVim = valuePort(false);
    const portSaved = eventPort();
    const callSave = callPort(doSave);
    const callGoto = callPort(doGoto);

    // Connect file → buffer so tests work without mount()
    portFile.subscribe((v) => {
      if (v) portBuffer.set(v);
    });

    // --- buffer helpers ---
    function getBuf(path) {
      return buffers.get(path) || { content: '', dirty: false };
    }

    function setBuf(path, content, dirty) {
      buffers.set(path, { content, dirty });
      portDirty.set(dirty);
      updateTabs();
      persist();
    }

    function markDirty() {
      const b = getBuf(currentPath);
      b.dirty = true;
      setBuf(currentPath, b.content, true);
    }

    function markSaved() {
      const b = getBuf(currentPath);
      b.dirty = false;
      setBuf(currentPath, b.content, false);
    }

    function updateCursor() {
      if (!view) return;
      const { head } = view.state.selection.main;
      const line = view.state.doc.lineAt(head);
      const pos = { path: currentPath, line: line.number, col: head - line.from + 1 };
      portCursor.set(pos);
      ws.emit('cursor', pos);
      updateStatus();
    }

    function updateBuffer() {
      if (!view) return;
      const text = view.state.doc.toString();
      portBuffer.set(text);
      ws.emit('buffer', { path: currentPath, text });
      const b = getBuf(currentPath);
      b.content = text;
      setBuf(currentPath, text, b.dirty);
    }

    function updateStatus() {
      if (!statusEl) return;
      const b = getBuf(currentPath);
      const { head } = view ? view.state.selection.main : { head: 0 };
      const line = view ? view.state.doc.lineAt(head).number : 1;
      const col = view ? head - view.state.doc.lineAt(head).from + 1 : 1;
      const symCount = view ? (view.state.doc.toString().match(/\w+/g) || []).length : 0;
      statusEl.textContent = `
Ln:${line} Col:${col}  ${b.dirty ? '●' : '○'}  λ ${symCount}`;
    }

    function updateTabs() {
      if (!tabsEl) return;
      tabsEl.innerHTML = '';
      for (const path of openTabs) {
        const b = getBuf(path);
        const isActive = path === currentPath;

        const tab = document.createElement('div');
        tab.className = 'file-tab' + (isActive ? ' active' : '');
        tab.dataset.path = path;
        tab.title = path;
        tab.style.cssText = `
          display: inline-flex;
          align-items: center;
          gap: 4px;
          padding: 2px 8px;
          font-size: 11px;
          background: ${isActive ? 'var(--bg-active)' : 'transparent'};
          border: 1px solid ${isActive ? 'var(--line-strong)' : 'var(--line)'};
          border-radius: 4px;
          cursor: pointer;
          white-space: nowrap;
        `;

        const dot = document.createElement('span');
        dot.className = 'fdot' + (b.dirty ? ' dirty' : '');
        dot.textContent = b.dirty ? '●' : '○';
        dot.style.cssText = 'font-size: 8px;';

        const lbl = document.createElement('span');
        lbl.className = 'flbl';
        lbl.textContent = path.split('/').pop();
        lbl.style.cssText = 'overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 120px;';

        const x = document.createElement('button');
        x.className = 'fclose';
        x.textContent = '×';
        x.style.cssText = `
          font-size: 12px;
          padding: 0 4px;
          background: none;
          border: none;
          cursor: pointer;
          opacity: 0.6;
        `;
        x.addEventListener('click', (e) => {
          e.stopPropagation();
          closeTab(path);
        });

        tab.append(dot, lbl, x);
        tab.addEventListener('click', () => switchFile(path));
        tabsEl.appendChild(tab);
      }

      const newBtn = document.createElement('button');
      newBtn.textContent = '＋';
      newBtn.title = 'New file';
      newBtn.style.cssText = `
        margin-left: 8px;
        padding: 2px 8px;
        font-size: 12px;
        background: var(--bg-active);
        border: 1px solid var(--line);
        border-radius: 4px;
        cursor: pointer;
      `;
      newBtn.addEventListener('click', () => openNewFile());
      tabsEl.appendChild(newBtn);
    }

    function closeTab(path) {
      const i = openTabs.indexOf(path);
      if (i < 0 || openTabs.length < 2) return;
      openTabs.splice(i, 1);
      if (path === currentPath) {
        const nxt = openTabs[Math.min(i, openTabs.length - 1)];
        updateTabs();
        switchFile(nxt);
      } else {
        updateTabs();
      }
    }

    function openNewFile() {
      const path = prompt('New file path:')?.trim();
      if (!path) return;
      if (buffers.has(path)) {
        switchFile(path);
        return;
      }
      buffers.set(path, { content: '', dirty: true });
      openTabs.push(path);
      updateTabs();
      switchFile(path);
    }

    async function switchFile(path) {
      if (path === currentPath) return;

      // Immediately reflect the file value on the buffer port
      portBuffer.set(path);

      // Save current buffer
      if (view) {
        const b = getBuf(currentPath);
        b.content = view.state.doc.toString();
        setBuf(currentPath, b.content, b.dirty);
      }

      currentPath = path;
      if (!openTabs.includes(path)) openTabs.push(path);

      await loadCM();

      /* FETCH IT IF IT HAS NEVER BEEN READ. `getBuf(path)` answers with a DEFAULT `{content:''}`
         for a path it has never seen, so the `if (b)` below was always true and the endpoint was
         never called: a file opened in this editor showed its tab and no text, and the panes
         wired to its `buffer` port had nothing to read. The endpoint is `file.get` — the name in
         `js/defaults/endpoints.js`; `ws.endpoint('file')` is not a route and threw. */
      let content;
      const b = buffers.has(path) ? buffers.get(path) : null;
      if (b) {
        content = b.content;
      } else {
        try {
          /* `endpoint.call.invoke` is the node's call PORT; invoking it is `.invoke(args)` on the
             port (the double, as js/defaults/wiring/_flows.js and js/panes/graph.js write it). One
             `.invoke` short was a TypeError, caught, and reported as "file not found in backend" —
             for every file. */
          const endpoint = ws.endpoint('file.get');
          const data = await endpoint.call.invoke.invoke({ path });
          content = data?.content || ';- file not found in backend\n';
        } catch (err) {
          console.warn('[editor] could not read ' + path + ':', err && err.message);
          content = ';- file not found in backend\n';
        }
        buffers.set(path, { content, dirty: false });
      }

      portBuffer.set(content);
      portDirty.set(false);
      portCursor.set({ path, line: 1, col: 1 });

      // Create or restore editor view
      if (view) {
        view.destroy();
      }

      if (!CM.EditorState) {
        console.warn('CM6 not loaded, cannot create editor view');
        updateTabs();
        updateStatus();
        ws.emit('file-opened', { path });
        return;
      }

      const state = CM.EditorState.create({
        doc: content,
        extensions: [
          CM.lineNumbers(),
          CM.highlightActiveLineGutter(),
          CM.highlightSpecialChars(),
          CM.history(),
          CM.drawSelection(),
          CM.dropCursor(),
          CM.rectangularSelection(),
          CM.crosshairCursor(),
          CM.indentOnInput(),
          CM.bracketMatching(),
          CM.closeBrackets(),
          CM.highlightSelectionMatches(),
          CM.foldGutter(),
          CM.keymap.of([
            ...CM.closeBracketsKeymap,
            ...CM.defaultKeymap,
            ...CM.historyKeymap,
            ...CM.searchKeymap,
            ...CM.foldKeymap,
            CM.indentWithTab,
          ]),
          CM.keymap.of([
            { key: 'Mod-s', run: () => { doSave(); return true; }, preventDefault: true },
          ]),
          CM.EditorView.updateListener.of((u) => {
            if (u.docChanged) {
              updateBuffer();
              markDirty();
            }
            if (u.docChanged || u.selectionSet) {
              updateCursor();
            }
          }),
        ],
      });

      view = new CM.EditorView({ state, parent: container });
      const editorWrap = container.querySelector('.editor-wrap') || container;
      editorWrap.appendChild(view.dom);
      view.focus();

      portBuffer.set(content);
      portDirty.set(false);
      portCursor.set({ path, line: 1, col: 1 });
      updateTabs();
      updateStatus();
      ws.emit('file-opened', { path });
    }

    async function doSave() {
      if (!view) return;
      const b = getBuf(currentPath);
      const text = view.state.doc.toString();

      // Refuse to save loading sentinel
      if (text.startsWith(LOADING_SENTINEL)) {
        console.warn('Refusing to save loading sentinel');
        return;
      }

      try {
        // `file.save` is the POST route in the table (see js/defaults/endpoints.js)
        const endpoint = ws.endpoint('file.save');
        await endpoint.call.invoke.invoke({ path: currentPath, content: text });
        markSaved();
        portSaved.emit(currentPath);
        ws.emit('saved', currentPath);
      } catch (err) {
        console.error('Save failed:', err);
      }
    }

    async function doGoto(pos) {
      if (!view) return;
      if (pos.path && pos.path !== currentPath) {
        await switchFile(pos.path);
      }
      if (pos.line) {
        const docLine = view.state.doc.line(pos.line - 1);
        let ch = Math.min(pos.col || 1, docLine.length) - 1;
        view.dispatch({
          selection: { anchor: docLine.from + ch, head: docLine.from + ch },
        });
        view.focus();
      }
    }

    function persist() {
      if (pendingSave) clearTimeout(pendingSave);
      pendingSave = setTimeout(() => {
        try {
          ws.storage.put('editor-buffers', Object.fromEntries(buffers));
          ws.storage.put('editor-tabs', openTabs);
          ws.storage.put('editor-current', currentPath);
        } catch {}
        pendingSave = null;
      }, 500);
    }

    function mount(containerEl) {
      container = containerEl;
      container.style.cssText = `display: flex; flex-direction: column; height: 100%;`;

      // Tabs bar
      tabsEl = document.createElement('div');
      tabsEl.className = 'editor-tabs';
      tabsEl.style.cssText = `
        display: flex;
        align-items: center;
        gap: 2px;
        padding: 4px 8px;
        border-bottom: 1px solid var(--line);
        overflow-x: auto;
      `;
      container.appendChild(tabsEl);

      // Editor area
      const editorWrap = document.createElement('div');
      editorWrap.className = 'editor-wrap';
      editorWrap.style.cssText = `flex: 1; overflow: hidden; position: relative;`;
      container.appendChild(editorWrap);

      // Status bar
      statusEl = document.createElement('div');
      statusEl.className = 'editor-status';
      statusEl.style.cssText = `
        display: flex;
        align-items: center;
        gap: 8px;
        padding: 2px 8px;
        font-size: 11px;
        font-family: monospace;
        border-top: 1px solid var(--line);
        background: var(--bg-panel);
      `;
      container.appendChild(statusEl);

      portFile.subscribe((v) => {
        if (v && v !== currentPath) {
          switchFile(v);
        }
      });

      updateTabs();
      updateStatus();
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
        statusEl = null;
        tabsEl = null;
      },
      in: {
        file: portFile,
        goto: {
          kind: 'event',
          on(fn) {
            callGoto.bind(async (args) => {
              await doGoto(args);
              fn(args);
            });
          },
        },
      },
      out: {
        buffer: portBuffer,
        cursor: portCursor,
        dirty: portDirty,
        vimMode: portVim,
        saved: portSaved,
      },
      call: {
        save: callSave,
        goto: callGoto,
      },
    };
  },
};
