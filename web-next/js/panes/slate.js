/**
 * panes/slate.js — Slate pane (M4.23).
 *
 * Lean scratch evaluator. Editable CM6 editor; POSTs to the `slate`
 * endpoint to evaluate Lean, falls back to the `check` WS endpoint.
 * Displays term output below the editor.
 *
 * Ported from app.js 896-908 (evalSlate).
 *
 * In:  `ink` (value Lean text)
 * Out: `latex` (value), `html` (value)
 * Call: {}
 */

import { valuePort, eventPort, callPort } from '../core/ports.js';

let CM = null;

async function loadCM() {
  if (CM) return CM;
  CM = {};
  /* THE BUNDLE THE PAGES ACTUALLY SERVE. Neither page declares an import map (web-next/index.html,
     web-next/shell.html), so `import('@codemirror/view')` — a bare specifier — cannot resolve in a
     browser and threw `Failed to resolve module specifier`; this function caught it, reported "CM6 not
     available" and returned an empty record, and no editor was ever built. `/vendor/cm.bundle.js` is
     where the canvas's own editor gets CodeMirror (js/live/editor.js) and it exports the same names.
     The bare specifiers stay as the fallback for a host that does resolve them. */
  try {
    CM = await import('/vendor/cm.bundle.js');
    return CM;
  } catch (err) {
    console.warn('[slate] no codemirror bundle:', err && err.message);
    CM = {};
  }
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
    console.warn('CM6 not available, slate pane will use fallback:', err.message);
  }
  return CM;
}

export default {
  kind: 'slate',
  title: 'Slate',
  ports: {
    in: { ink: 'value' },
    out: { latex: 'value', html: 'value' },
    call: {},
  },
  create(ctx) {
    const { ws } = ctx;
    /* THE ENDPOINT NODE, from whichever name this host answers. The engine host hands a pane its
       workspace facade (`ws.endpoint`); the canvas answers `ctx.endpoint` directly, because `ctx.ws`
       is how the panes DETECT the engine host (js/panes/new-node.js chooses its modal shape on it) and
       a canvas tile must not look like one. Asking for the capability rather than the host is what
       lets one module run in both. */
    const endpointOf = (name) => {
      if (ws && typeof ws.endpoint === 'function') return ws.endpoint(name);
      if (typeof ctx.endpoint === 'function') return ctx.endpoint(name);
      return null;
    };
    let el = null;
    let view = null;
    let container = null;
    let outputEl = null;
    let statusEl = null;
    let currentBuffer = '';

    // Ports
    const portInk = valuePort('');
    const portLatex = valuePort(null);
    const portHtml = valuePort(null);

    portInk.subscribe((inkText) => {
      currentBuffer = inkText || '';
      if (view && view.state.doc.toString() !== currentBuffer) {
        view.dispatch({
          changes: { from: 0, to: view.state.doc.length, insert: currentBuffer },
        });
      }
    });

    function termLine(text, kind, out) {
      if (!out) return;
      const div = document.createElement('div');
      const ts = new Date().toLocaleTimeString([], { hour12: false });
      div.innerHTML = `<span class="term-ts">[${ts}]</span> ${esc(text)}`;
      div.style.cssText = `
        padding: 1px 4px;
        font-size: 10px;
        font-family: monospace;
        border-bottom: 1px solid var(--line);
        white-space: pre-wrap;
      `;
      if (kind === 'cmd') div.style.color = 'var(--cyan)';
      else if (kind === 'warn') div.style.color = 'var(--amber)';
      else if (kind === 'ok') div.style.color = 'var(--green)';
      else div.style.color = 'var(--ink)';
      out.appendChild(div);
      out.scrollTop = out.scrollHeight;
    }

    function esc(s) {
      return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
    }

    /**
     * The editor this pane was written around.
     *
     * Every part of it already assumes one — the `ink` port pushes its text into `view`, `unmount`
     * destroys it, `runSlate` reads `view.state.doc` — and nothing ever built one: the editor area was
     * an empty div, so the tile could be filled by a wire and never typed into, and its Run button
     * evaluated whatever the wire had last delivered. This is that missing half.
     *
     * The extensions are listed defensively (`&&`, then `filter(Boolean)`) because the vendor bundle
     * is a build, not the package: it does not export `closeBrackets`, `closeBracketsKeymap` or
     * `crosshairCursor`, and a missing extension must cost that feature rather than the editor.
     */
    async function buildEditor(host) {
      const C = await loadCM();
      /* `view` already built (a re-mount, or CM arriving after the first tile): one editor per pane.
         And NOT a check on `host.isConnected`: a tile can be culled by the LOD while CodeMirror loads,
         and giving up there left the tile permanently without an editor — the editor is built into the
         host and renders as soon as the tile is on screen again. */
      if (!host || view) return;
      if (!C.EditorView) {
        termLine('⚠ no editor: codemirror did not load', 'warn', outputEl);
        return;
      }
      const ext = [
        C.lineNumbers && C.lineNumbers(),
        C.highlightActiveLineGutter && C.highlightActiveLineGutter(),
        C.highlightSpecialChars && C.highlightSpecialChars(),
        C.history && C.history(),
        C.foldGutter && C.foldGutter(),
        C.indentOnInput && C.indentOnInput(),
        C.bracketMatching && C.bracketMatching(),
        C.closeBrackets && C.closeBrackets(),
        C.drawSelection && C.drawSelection(),
        C.dropCursor && C.dropCursor(),
        C.rectangularSelection && C.rectangularSelection(),
        C.crosshairCursor && C.crosshairCursor(),
        C.highlightSelectionMatches && C.highlightSelectionMatches(),
        C.keymap && C.keymap.of([
          ...(C.indentWithTab ? [C.indentWithTab] : []),
          ...(C.defaultKeymap || []),
          ...(C.searchKeymap || []),
          ...(C.historyKeymap || []),
          ...(C.foldKeymap || []),
          ...(C.closeBracketsKeymap || []),
        ]),
        C.EditorView.updateListener.of((u) => {
          if (!u.docChanged) return;
          currentBuffer = u.state.doc.toString();
          updateStatus();
        }),
      ].filter(Boolean);
      view = new C.EditorView({ doc: currentBuffer, extensions: ext, parent: host });
      updateStatus();
    }

    async function runSlate() {
      if (!outputEl) return;
      outputEl.innerHTML = '';
      const text = view ? view.state.doc.toString() : currentBuffer;
      termLine('$ lake env lean TempCheck.lean   (HL scratch → parse_hl → emit_lean → CIC kernel)', 'cmd', outputEl);

      try {
        {
          const slateEp = endpointOf('slate.post');
          if (slateEp && slateEp.call && slateEp.call.invoke && slateEp.call.invoke.invoke) {
            const j = await slateEp.call.invoke.invoke({ content: text });
            termLine((j && (j.output || (j.ok ? '✓ ok' : ''))) || '✓ ok', '', outputEl);
            return;
          }
        }
      } catch {}

      // Fallback to the registered `ws.check` endpoint.
      try {
        {
          const checkEp = endpointOf('ws.check');
          if (checkEp && checkEp.out && checkEp.out.message && checkEp.out.message.on) {
            const off = checkEp.out.message.on((e) => termLine(String(e.data !== undefined ? e.data : e), '', outputEl));
            if (checkEp.call && checkEp.call.send && checkEp.call.send.invoke) {
              await checkEp.call.send.invoke(text);
            }
            setTimeout(off, 4000);
            return;
          }
        }
      } catch {}

      termLine('⚠ backend unreachable — slate requires backend to evaluate Lean', 'warn', outputEl);
    }

    function mount(containerEl) {
      container = containerEl;
      el = document.createElement('div');
      el.className = 'slate-pane';
      el.style.cssText = `display: flex; flex-direction: column; height: 100%; font-size: 11px; font-family: monospace;`;

      // Header
      const header = document.createElement('div');
      header.className = 'slate-header';
      header.style.cssText = `
        display: flex;
        align-items: center;
        gap: 6px;
        padding: 4px 8px;
        border-bottom: 1px solid var(--line);
      `;

      const title = document.createElement('div');
      title.textContent = 'Slate (Lean scratch)';
      title.style.cssText = 'font-weight: 600; font-size: 12px; color: var(--ink);';

      const runBtn = document.createElement('button');
      runBtn.textContent = '⇥ Run';
      runBtn.title = 'Evaluate Lean via /api/slate';
      runBtn.style.cssText = `
        margin-left: auto;
        padding: 4px 12px;
        font-size: 11px;
        background: var(--cyan);
        border: none;
        border-radius: 4px;
        color: var(--ink);
        cursor: pointer;
        font-weight: 600;
      `;
      runBtn.addEventListener('click', runSlate);

      header.append(title, runBtn);
      el.appendChild(header);

      // CM6 editor area
      const editorWrap = document.createElement('div');
      editorWrap.className = 'slate-editor-wrap';
      editorWrap.style.cssText = `flex: 1; overflow: hidden; position: relative;`;
      el.appendChild(editorWrap);
      // the editor is built when CodeMirror arrives; CM loads once per page, so this is a promise
      // that resolves immediately after the first tile
      buildEditor(editorWrap);

      // Output panel
      const outputWrap = document.createElement('div');
      outputWrap.className = 'slate-output-wrap';
      outputWrap.style.cssText = `height: 120px; border-top: 1px solid var(--line);`;

      const outputTitle = document.createElement('div');
      outputTitle.textContent = 'Output'; // eslint-disable-line no-undef
      outputTitle.style.cssText = `padding: 2px 8px; font-size: 10px; color: var(--muted); border-bottom: 1px solid var(--line);`;
      outputWrap.appendChild(outputTitle);

      outputEl = document.createElement('div');
      outputEl.className = 'slate-output';
      outputEl.style.cssText = `overflow-y: auto; padding: 6px; background: var(--bg-editor); color: var(--ink);`;
      outputWrap.appendChild(outputEl);
      el.appendChild(outputWrap);

      // Status bar
      statusEl = document.createElement('div');
      statusEl.className = 'slate-status';
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
      updateStatus();
    }

    function updateStatus() {
      if (!statusEl) return;
      statusEl.textContent = `Slate · ${currentBuffer.length} chars`;
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
        outputEl = null;
        statusEl = null;
      },
      in: {
        ink: portInk,
      },
      out: {
        latex: portLatex,
        html: portHtml,
      },
      call: {},
    };
  },
};
