/**
 * panes/source.js — Source pane.
 *
 * The tile that shows another tile's source code. This is the pane the whole module tree
 * exists for: point it at a tile, and what you read is the file that tile is built from —
 * the module for an app pane, the `program` body for a user pane, the `render` view for a
 * manifest pane.
 *
 * In:  `subject` (value pane id)   Out: {}   Call: `save`, `revert`
 *
 * Three things a source tile is allowed to do, and it says which one it is doing:
 *
 *   reading    an app module — shown, not editable, because editing the running app from
 *              inside the running app is how you lose the app
 *   live       a user pane's program: edits change the pane immediately, but nothing is
 *              written to the server
 *   saving     a manifest pane: `save` writes it back through the pane's own endpoint
 *
 * The editor is js/live/editor.js (CodeMirror), the same one the editor tile uses, and it
 * falls back to a plain textarea if that module cannot load.
 */

import { valuePort, callPort } from '../core/ports.js';

export default {
  kind: 'source',
  title: 'Source',
  ports: { in: { subject: 'value' }, out: {}, call: { save: 'call', revert: 'call' } },
  create(ctx) {
    const esc = (ctx.ui && ctx.ui.esc) || ((s) => String(s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c])));

    let el = null, bar = null, label = null, saveBtn = null, revertBtn = null, host = null;
    let ed = null, disposed = false, timer = 0, original = null, src = null, target = null;
    let queue = Promise.resolve();

    const inSubject = valuePort(null);

    inSubject.subscribe((id) => { load(id); });

    /** Read the text of a source descriptor: in hand for a program, fetched for a file. */
    async function textOf(descriptor) {
      if (descriptor.text != null) return descriptor.text;
      try {
        const r = await fetch(ctx.url('file.get') + '?path=' + encodeURIComponent(descriptor.path));
        const j = await r.json();
        return j.content || '';
      } catch (e) {
        return '// could not read ' + descriptor.path;
      }
    }

    function chrome() {
      if (!label) return;
      if (!src) {
        label.textContent = 'no pane bound';
        if (saveBtn) saveBtn.disabled = true;
        if (revertBtn) revertBtn.disabled = true;
        return;
      }
      label.textContent = (src.editable ? 'editing ' : 'reading ') + (target.kind || '?')
        + ' · ' + (src.what || String(src.path || '').split('/').pop());
      label.title = src.editable ? 'this tile is the program the pane runs' : (src.path || '');
      if (saveBtn) {
        saveBtn.disabled = !src.editable || src.save !== 'manifest';
        saveBtn.title = saveBtn.disabled ? (src.editable ? 'live only: this program is not from the manifest' : 'app module — read only') : 'write the manifest to the server';
      }
      if (revertBtn) revertBtn.disabled = !src.editable;
    }

    /** Mount the editor for one subject and hand it the text. */
    function load(subjectId) {
      if (!host) return;
      queue = queue.then(async () => {
        /* the tile may have been closed while this was queued: a load that runs after unmount
           would write into elements that no longer exist (a null host, a null label) */
        if (disposed || !host) return;
        if (ed && ed.destroy) { try { ed.destroy(); } catch (e) {} }
        ed = null;
        host.textContent = '';
        target = (ctx.panes() || []).find((q) => q.id === subjectId) || null;
        src = target && ctx.paneSource ? ctx.paneSource(target) : null;
        chrome();
        if (!src) {
          host.innerHTML = '<div class="empty">open this from a tile header: ‹/› edit source</div>';
          return;
        }
        const text = await textOf(src);
        if (disposed || !host) return;
        original = text;
        const m = await (ctx.loadEditor ? ctx.loadEditor() : Promise.resolve(null));
        if (disposed || !host) return;
        if (!m) { host.innerHTML = '<div class="empty">editor module unavailable</div>'; return; }
        const opts = {
          doc: text, wrap: false, tabSize: 2, readOnly: !src.editable, syntax: true,
          onChange: (t) => {
            if (!src.editable) return;
            clearTimeout(timer);
            timer = setTimeout(() => { if (ctx.applyPaneSource) ctx.applyPaneSource(target, t); }, 250);
          },
        };
        try { ed = await m.mountEditor(host, opts); } catch (e) { ed = m.mountFallback(host, opts); }
        if (disposed) { try { ed.destroy(); } catch (e) {} return; }
      });
      return queue;
    }

    const callSave = callPort(() => {
      if (!src || !src.editable || src.save !== 'manifest') return { ok: false, error: 'not writable' };
      if (ctx.savePaneSource) ctx.savePaneSource(target);
      return { ok: true };
    });
    const callRevert = callPort(() => {
      if (!ed || !original) return { ok: false, error: 'nothing to revert' };
      ed.setDoc(original);
      if (ctx.applyPaneSource && target) ctx.applyPaneSource(target, original);
      return { ok: true };
    });

    return {
      mount(h) {
        el = document.createElement('div');
        el.className = 'ed-host-wrap';
        bar = document.createElement('div');
        bar.className = 'ed-bar';
        label = document.createElement('span');
        label.className = 'ed-kind';
        const btn = (text) => {
          const b = document.createElement('button');
          b.className = 'ed-tog';
          b.textContent = text;
          b.style.width = 'auto';
          b.style.padding = '3px 8px';
          return b;
        };
        revertBtn = btn('revert');
        saveBtn = btn('save');
        revertBtn.addEventListener('click', () => callRevert.invoke());
        saveBtn.addEventListener('click', () => callSave.invoke());
        bar.append(label, revertBtn, saveBtn);
        host = document.createElement('div');
        host.className = 'ed-host';
        el.append(bar, host);
        h.appendChild(el);
        chrome();
      },
      unmount() {
        disposed = true;
        clearTimeout(timer);
        if (ed && ed.destroy) { try { ed.destroy(); } catch (e) {} }
        ed = null;
        if (el && el.parentNode) el.parentNode.removeChild(el);
        el = bar = label = saveBtn = revertBtn = host = null;
      },
      /** the page re-renders this tile when the selected pane changes */
      setSubject(id) { load(id); },
      in: { subject: inSubject },
      out: {},
      call: { save: callSave, revert: callRevert },
      get ports() { return { in: this.in, out: this.out, call: this.call }; },
    };
  },
};