/**
 * panes/editor-symbols.js — Symbol-aware editor pane (M4.12).
 *
 * CM6-based code editor with symbol and citation awareness.
 * Integrates with symbols.js and citations.js for symbol completion,
 * hover, and navigation.
 *
 * In:  `file` (value path), `goto` (event `{line,col}|{pos}`)
 * Out: `buffer` (value text), `cursor` (value `{path,line,col}`), `dirty` (value bool), `vimMode` (value bool), `saved` (event path)
 * Call: `save`, `goto({line,col}|{pos})`
 */

import { valuePort, eventPort, callPort } from '../core/ports.js';
import { symbolEntries, triggerFor } from '../lib/symbols.js';
import { rankCites, CITE_RE } from '../lib/citations.js';

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
    console.warn('CM6 not available, editor-symbols will use fallback:', err.message);
  }
  return CM;
}

export default {
  kind: 'editor-symbols',
  title: 'Editor (Symbols)',
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
    const buffers = new Map();
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
      // Count symbol references (@-citations and \-symbols)
      const text = view.state.doc.toString();
      const symCount = (text.match(CITE_RE) || []).length + (text.match(/\\[a-zA-Z]+/g) || []).length;
      statusEl.textContent = `
Ln:${line} Col:${col}  ${b.dirty ? '●' : '○'}  ⊛ ${symCount}`;
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
      await loadCM();
      if (path === currentPath) return;

      portBuffer.set(path);

      if (view) {
        const b = getBuf(currentPath);
        b.content = view.state.doc.toString();
        setBuf(currentPath, b.content, b.dirty);
      }

      currentPath = path;
      if (!openTabs.includes(path)) openTabs.push(path);

      let content;
      const b = getBuf(path);
      if (b) {
        content = b.content;
      } else {
        try {
          const endpoint = ws.endpoint('file');
          const data = await endpoint.call.invoke({ path });
          content = data?.content || ';- file not found in backend\n';
        } catch {
          content = ';- file not found in backend\n';
        }
        buffers.set(path, { content, dirty: false });
      }

      portBuffer.set(content);
      portDirty.set(false);
      portCursor.set({ path, line: 1, col: 1 });

      if (view) view.destroy();

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
      if (text.startsWith(LOADING_SENTINEL)) {
        console.warn('Refusing to save loading sentinel');
        return;
      }
      try {
        const endpoint = ws.endpoint('file');
        await endpoint.call.invoke({ path: currentPath, content: text });
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
          ws.storage.put('editor-symbols-buffers', Object.fromEntries(buffers));
          ws.storage.put('editor-symbols-tabs', openTabs);
          ws.storage.put('editor-symbols-current', currentPath);
        } catch {}
        pendingSave = null;
      }, 500);
    }

    function mount(containerEl) {
      container = containerEl;
      container.style.cssText = `display: flex; flex-direction: column; height: 100%;`;

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

      const editorWrap = document.createElement('div');
      editorWrap.className = 'editor-wrap';
      editorWrap.style.cssText = `flex: 1; overflow: hidden; position: relative;`;
      container.appendChild(editorWrap);

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
