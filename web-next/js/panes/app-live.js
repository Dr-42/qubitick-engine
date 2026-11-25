/*
 * panes/app-live.js — the four canvas panes that are not pane modules.
 *
 * The tree, the graph, the editor and the handwriting pad are different from every other
 * kind: they build their own markup instead of mounting `js/panes/*.js`, because what they
 * need is a LIVE object (a canvas, a CodeMirror instance, a tree widget) with the app's
 * documents plugged into it, not a render function. So they keep their own wrappers — but
 * the wrappers were 250 lines inside web-next/index.html, which is the thing being moved.
 *
 * The rules that live here, and are the reason it is a module rather than a paste:
 *
 *   ONE UNDO HISTORY PER DOCUMENT. Two editor tiles on one file must not have two histories;
 *   the tile that owns the document's history is the one whose undo runs, and the others hand
 *   their keys to it (and fall back to their own history only if it has gone).
 *
 *   A TILE THAT IS DESTROYED WHILE LOADING STAYS DESTROYED. Every one of these wrappers
 *   awaits a dynamic import; a tile closed in that window must not attach itself to the
 *   document afterwards, or a closed tile keeps writing into a document nobody can see.
 *
 *   A LINKED TILE NAVIGATES THE WORKSPACE, AN UNLINKED ONE ONLY ITSELF. The tree's pick
 *   either opens the file everywhere (linked) or moves its own highlight, which is the whole
 *   point of an unlinked reference view.
 */

import { plan as proofPlan, rowFor, splitRelation } from '../live/proof-row.js';
import { inductionPlan, caseEdits } from '../live/induce.js';
import { createCheckStrip } from '../live/check-strip.js';
import { followCite } from '../live/cite-link.js';
import { paintSyntaxButton } from '../live/syntax-button.js';

/**
 * @param {object} app  the canvas app (see web-next/js/README.md)
 */
export function createLivePanes(app) {
  const esc = (app.ui && app.ui.esc) || ((s) => String(s));
  const toast = (app.ui && app.ui.toast) || (() => {});
  const data = () => app.data();

  /* ------------------------------------- Enter: a sentence becomes the next proof row */
  /**
   * The line under the caret, as the next row of the proof above it.
   *
   * The decision is made WITHOUT the engine, and that is the point: `true` here claims the key,
   * so the newline the author pressed for does not land first and push the row down. A line with
   * no statement above it (or a blank one, or prose) is not a conversion, and Enter stays Enter.
   */
  function convertOnEnter(v) {
    const lines = v.state.doc.toString().split('\n');
    const index = v.state.doc.lineAt(v.state.selection.main.head).number - 1;
    const p = proofPlan({ lines, index });
    if (!p.ready) return false;
    convert(v, p);
    return true;
  }

  /** Ask the engine which citation bridges the statement above to this line, then write the row. */
  async function convert(v, p) {
    let answer = null;
    let error = app.autocite ? null : 'this host has no citation search';
    if (app.autocite) {
      try { answer = await app.autocite(p.lhs, p.expr); }
      catch (e) { error = e.message; }
    }
    /* The author may have kept typing while the search ran. The row belongs to the line as it
       was, so it is written only while that line is still there, unchanged — writing it over
       whatever is there now would put a citation on somebody else's sentence. */
    const doc = v.state.doc;
    const line = doc.line(Math.min(p.lineNumber + 1, doc.lines));
    const stillThere = line.text === p.typed;
    const out = rowFor({ plan: p, answer, error });
    if (!out.ok) {
      toast(out.error);
      if (stillThere) newlineAt(v, line.to);
      return;
    }
    if (!stillThere) {
      toast('the line changed while the citation was looked up — nothing was written');
      return;
    }
    v.dispatch({
      changes: { from: line.from, to: line.to, insert: out.line + '\n' },
      selection: { anchor: line.from + out.line.length + 1 },
    });
  }

  /** The newline the author asked for, when no row was written in its place. */
  function newlineAt(v, at) {
    v.dispatch({ changes: { from: at, insert: '\n' }, selection: { anchor: at + 1 } });
  }

  /**
   * `Mod+Enter`: write the goal of every case of the induction block the caret is in.
   *
   * The author declares the induction (`\mu a`, then a `\mapsto` group per case — the corpus's own
   * shape) and the chord writes each case's goal under its pattern, in the author's notation. It is
   * synchronous: everything it needs is in the document (`js/live/induce.js` decides it), so there
   * is nothing to wait for and the block is written or it is not.
   */
  function induceCases(v) {
    const lines = v.state.doc.toString().split('\n');
    const index = v.state.doc.lineAt(v.state.selection.main.head).number - 1;
    const p = inductionPlan({ lines, index });
    if (!p.ready) { toast(p.why || 'nothing to induce here'); return true; }
    const edits = caseEdits({ plan: p, lineEnd: (n) => v.state.doc.line(n + 1).to });
    if (!edits.length) { toast('every case of this block already has a body'); return true; }
    /* the caret waits at the end of the FIRST goal written, which is where the next row goes.
       The offset is the insert's own end, in the document the transaction is making — not a line
       number read out of the document as it is now, which shifts by the insert itself. */
    const first = edits[0];
    v.dispatch({ changes: edits, selection: { anchor: first.from + first.insert.length } });
    toast('induced ' + p.cases.length + (p.cases.length === 1 ? ' case' : ' cases') + ': ' + p.cases.map((c) => c.pattern).join(', '));
    return true;
  }

  /* ------------------------------------------------------------------ tree */
  function renderTree(p) {
    p.body.innerHTML = '';
    const host = document.createElement('div');
    host.className = 'tv-host';
    p.body.appendChild(host);
    p.summary = () => data().files.length + ' files';
    import('../live/tree.js').then((m) => {
      if (!p.el.isConnected) return;
      const tv = m.mountTree(host, data().files, {
        selected: app.selOf(p) || app.editor().file.get() || null,
        openPrefix: (app.editor().file.get() || data().files[0] || ''),
        onPick: (path) => {
          app.setSel(p, path);
          // a linked tree navigates the whole workspace; an unlinked one is a reference
          // view, so it only moves its own highlight
          if (app.linkedSel(p)) { app.tree().file.set(path); app.openFile(path); }
          else { tv.setSelected(path); app.paintChrome(p); }
          toast(path.split('/').pop());
        },
      });
      p.chrome = () => { if (p.el.isConnected && tv.setSelected) tv.setSelected(app.selOf(p)); };
      if (app.linkedSel(p)) app.editor().file.on((path) => { if (path) tv.setSelected(path); });
    }).catch((e) => {
      console.warn('[tree]', e);
      p.body.innerHTML = '<div class="empty">tree unavailable</div>';
    });
  }

  /* ----------------------------------------------------------------- graph */
  function renderGraph(p) {
    p.body.innerHTML = '<div class="empty">laying out...</div>';
    const host = document.createElement('div');
    host.className = 'g-host';
    const graph = data().graph || {};
    const nodes = graph.nodes || [], edges = graph.edges || [];
    p.summary = () => nodes.length + ' nodes · ' + edges.length + ' edges';
    p.implDraw = null;
    // a graph node id is whatever the corpus calls it: a label, a name or an id
    const hitFor = (id) => {
      for (const n of nodes) {
        const label = String(n.label ?? n.name ?? n.id);
        if (label === id || String(n.id) === String(id)) return String(n.id ?? n.name);
      }
      return null;
    };
    import('../live/graph.js').then((m) => {
      if (!p.el.isConnected) return;
      p.body.innerHTML = '';
      p.body.appendChild(host);
      return m.mountGraph(host, { nodes, edges, selected: app.selOf(p), onPick: (n) => { app.setSel(p, n.label); toast(n.label); } }).then((api) => {
        p.implDraw = null;
        p.chrome = () => { if (api.setSelected) api.setSelected(hitFor(app.selOf(p))); };
        app.tree().selection.on((id) => { if (api.setSelected) api.setSelected(hitFor(id)); });
        /* A WIRE CAN HIGHLIGHT A NODE. `highlight` has been on this surface since the graph was
           written and nothing ever delivered a value to it: `graph`'s only declared input was
           offered to the drawer and could not arrive, so a wire into the tile was decoration. The
           tree's own selection reaches the tile through the app (the line above), which is a
           different path — this is the one a person can draw. */
        /* composes with the host's forwarder (js/app/pane-host.js), which runs first */
    const prevOnInputs = p.onInputs;
    p.onInputs = (ins) => { if (prevOnInputs) prevOnInputs(ins);
          if (ins && ins.highlight !== undefined && api.highlight) api.highlight(hitFor(ins.highlight));
        };
      });
    }).catch((e) => {
      console.warn('[graph]', e);
      p.body.innerHTML = '<div class="empty">graph unavailable: ' + esc(e.message || e) + '</div>';
    });
  }

  /* ---------------------------------------------------------------- editor */
  function renderEditor(p) {
    p.body.innerHTML = '';
    const bar = document.createElement('div');
    bar.className = 'ed-bar';
    const sel = document.createElement('select');
    sel.className = 'ed-files';
    sel.title = 'the file this tile shows';
    for (const f of data().files) {
      const o = document.createElement('option');
      o.value = f;
      /* THE PATH AS THE CORPUS SPELLS IT. This used to strip `./project/` and then
         `Era0/Strata(0|1)/` — the harness's own directory and its two first strata — which made
         another project's files look like they lived somewhere they do not, and hid the very
         directories `scope_of_path` reads to place a node. A leading `./` is the only noise that
         is not information. */
      o.textContent = String(f).replace(/^\.\//, '');
      sel.appendChild(o);
    }
    const kindEl = document.createElement('span');
    kindEl.className = 'ed-kind';
    const dirty = document.createElement('span');
    dirty.className = 'ed-dirty';
    dirty.title = 'unsaved edits';
    const synBtn = document.createElement('button');
    synBtn.className = 'ed-tog';
    synBtn.type = 'button';
    /* SAVE. The file select is here, so this is where a person looks; a pinned tile saves ITS document
       (the Ctrl+S path saves the one the editor follows). */
    const saveBtn = Object.assign(document.createElement('button'), {
      className: 'ed-save', type: 'button', textContent: 'save',
      title: 'write this tile’s file to the corpus  (Ctrl+S)',
    });
    saveBtn.addEventListener('click', () => { if (app.saveDoc) app.saveDoc(p); });
    bar.append(sel, dirty, synBtn, saveBtn, kindEl);
    p.body.appendChild(bar);
    /* THE CHECK STRIP: what the corpus makes of the row the caret is on, and — when there is
       something to accept — the citation that would complete it. It is a strip rather than a
       decoration because the answer is a sentence (`A1.4 does not get there (it reaches …)`), and
       because "try this" has to be clickable. */
    const check = document.createElement('div');
    check.className = 'ed-check';
    p.body.appendChild(check);
    const host = document.createElement('div');
    host.className = 'ed-host';
    p.body.appendChild(host);

    if (p.onUnmount) p.onUnmount();
    let ed = null, disposed = false;
    p.ed = null;
    p.onUnmount = () => {
      disposed = true;
      if (p.doc) p.doc.views.delete(p.view);
      if (p.doc && p.doc.primary === ed) p.doc.primary = null;
      if (ed) { try { ed.destroy(); } catch (e) { /* already gone */ } }
      ed = null;
      p.ed = null;
    };

    const dataFirst = () => data().files[0] || '';
    function chrome() {
      const path = (p.doc && p.doc.path) || app.editor().file.get() || dataFirst();
      if (sel.value !== path) sel.value = path;
      const seg = (path || '').match(/\/(Axioms|Definitions|Theorems|Calibrations)\//);
      const tag = seg ? ({ Axioms: 'AXIOM', Definitions: 'DEF', Theorems: 'THM', Calibrations: 'CAL' }[seg[1]]) : 'FILE';
      const text = (p.doc && p.doc.text) || (ed ? ed.getDoc() : '');
      const nv = (p.doc && p.doc.views.size) || 1;
      kindEl.textContent = tag + ' · ' + text.split('\n').length + 'L' + (nv > 1 ? (' · ' + nv + ' views') : '');
      const d = (p.doc && p.doc.dirty) || (p.doc === app.activeDoc() && app.editor().dirty.get());
      dirty.style.color = d ? 'var(--amber)' : 'var(--dim)';
      dirty.textContent = d ? '●' : '';
      p.summary = () => text.split('\n').length + ' lines · ' + ((path || '').split('/').pop() || '');
      if (p._summary && p.card && p.card.lastChild) p.card.lastChild.textContent = p.summary();
    }
    p.chrome = chrome;

    /* Syntax highlighting: a local switch, which a boolean on the tile's `syntax` port takes over the
       moment something is wired to it (js/live/syntax-button.js paints it). */
    const paintSyntax = () => paintSyntaxButton(synBtn, { on: p.syntax, wired: p.syntaxWired });
    const prevOnInputs = p.onInputs;
    p.onInputs = (ins) => { if (prevOnInputs) prevOnInputs(ins);
      if (ins.syntax === undefined) { p.syntaxWired = false; paintSyntax(); return; }
      p.syntaxWired = true;
      p.syntax = !!ins.syntax;
      paintSyntax();
      if (p.ed && p.ed.setOpts) p.ed.setOpts({ syntax: p.syntax });
    };
    synBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      if (p.syntaxWired) { toast('this tile’s syntax port is wired'); return; }
      p.syntax = !p.syntax;
      paintSyntax();
      if (p.ed && p.ed.setOpts) p.ed.setOpts({ syntax: p.syntax });
      app.setViewSyntax(p.syntax);
      app.save();
      toast('syntax highlighting ' + (p.syntax ? 'on' : 'off'));
    });
    paintSyntax();

    // this tile's own picker selects the file THIS tile shows; on a following tile that is
    // also the active file, so the other followers come along
    sel.addEventListener('change', async () => {
      const d = app.docOf(sel.value, await app.loadText(sel.value));
      app.showDoc(p, d);
      if (!p.pin) app.setActive(sel.value);
      toast(p.pin ? ('pinned to ' + sel.value.split('/').pop()) : ('opened ' + sel.value.split('/').pop()));
    });

    /* THE CHECK STRIP: what the corpus makes of the row the caret is on, and the citation that
       would complete it (`js/live/check-strip.js`). */
    const strip = createCheckStrip({ app, p, el: check, toast, lines: () => String((p.ed && p.ed.getDoc && p.ed.getDoc()) || (p.doc && p.doc.text) || '').split('\n') });

    (async () => {
      const path = (p.pin && p.subject) || app.editor().file.get() || dataFirst();
      if (!path) { host.innerHTML = '<div class="empty">no corpus files</div>'; return; }
      const d = app.docOf(path, await app.loadText(path));
      const primary = !d.primary;                       // one history per document
      const m = await app.loadEditor();
      if (disposed) return;
      /* No editor module: say so in the tile. The plain textarea lives IN that module, so
         there is nothing to fall back on here — but a tile that explains itself is worth
         more than one that stays blank. */
      if (!m) { host.innerHTML = '<div class="empty">editor module unavailable</div>'; return; }
      const opts = {
        doc: d.text, wrap: app.editorPrefs().wrap, tabSize: app.editorPrefs().tabSize, syntax: p.syntax,
        onChange: (text, changes, remote) => {
          const dd = p.doc;
          if (!dd) return;
          if (remote) dd.text = text; else app.docEdited(dd, text, p.view, changes);
          chrome();
          strip.scheduleCheck();
        },
        onCursor: (c, remote) => {
          if (remote) return;
          app.editor().cursor.set(c);
          strip.paint();
          strip.scheduleLook();
        },
        /* type the statement you want next, press Enter: the row is written with the sign that
           comes and the citation that justifies it, or the error says no citation was found
           (js/live/proof-row.js decides what that means) */
        onEnter: (v) => convertOnEnter(v),
        /* a `[A1.4]` is a reference: clicking it opens what it names (js/live/editor.js marks the
           tags, this pane follows them) */
        onCite: (name) => followCite({ app, toast, name }),
        /* declare an induction (`\mu a` and a `\mapsto` group per case) and press Mod+Enter: each
           case's goal is written under its pattern (js/live/induce.js) */
        onInduce: (v) => induceCases(v),
        // one undo history per document: the views that do not own it hand their keys to
        // the view that does, and fall back to their own history if it has gone
        onUndo: () => { const dd = p.doc; if (dd && dd.primary && dd.primary !== ed && dd.primary.undo) { dd.primary.undo(); return true; } return false; },
        onRedo: () => { const dd = p.doc; if (dd && dd.primary && dd.primary !== ed && dd.primary.redo) { dd.primary.redo(); return true; } return false; },
      };
      try { ed = await m.mountEditor(host, opts); }
      catch (err) {
        console.warn('[editor] CodeMirror unavailable, falling back:', err && err.message);
        ed = m.mountFallback(host, opts);
      }
      if (disposed) { try { ed.destroy(); } catch (e) { /* gone */ } return; }
      p.ed = ed;
      /* FLIPPING `suggest rewrites` HAS TO REACH THE STRIP NOW: the setting is read when the strip
         looks (js/live/check-strip.js), and a knob whose effect waits for the next caret move is a
         knob that reads as broken. The hook rides the editor handle, so the app's
         `applyEditorPrefs` (js/app/prefs.js) calls it for this tile and the strip dies with it. */
      ed.onPrefs = () => { strip.paint(); strip.scheduleLook(); };
      if (primary) d.primary = ed;
      if (!p.view) p.view = { seen: p.id, applyRemote: (ch) => { if (p.ed && p.ed.applyRemote) p.ed.applyRemote(ch); } };
      p.docView = true;                                 // a view, not just a watcher
      d.views.add(p.view);
      app.showDoc(p, d, { force: true });
      app.watchActive(p);
      app.addEditor(ed);
      app.applyEditorPrefs();                           // current font / tab / wrap
      chrome();
      /* the strip has nothing to say until the buffer has been checked once */
      strip.scheduleCheck();
    })();
  }

  /* -------------------------------------------------------------- handwrite */
  function renderHandwrite(p) {
    p.body.innerHTML = '';
    const hw = document.createElement('div');
    hw.className = 'hw';
    const bar = document.createElement('div');
    bar.className = 'bar';
    const clearBtn = document.createElement('button');
    clearBtn.textContent = 'clear';
    const ocrBtn = document.createElement('button');
    ocrBtn.textContent = 'recognize (LaTeX)';
    const out = document.createElement('div');
    out.className = 'out';
    bar.append(ocrBtn, clearBtn);
    hw.append(bar);
    const cv = document.createElement('canvas');
    cv.className = 'bd';
    hw.appendChild(cv);
    hw.appendChild(out);
    p.body.appendChild(hw);

    const ctx = cv.getContext('2d');
    let drawing = false, last = null;
    function size() {
      const r = cv.getBoundingClientRect();
      const dpr = devicePixelRatio || 1;
      if (cv.width !== r.width * dpr || cv.height !== r.height * dpr) {
        cv.width = r.width * dpr;
        cv.height = r.height * dpr;
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        ctx.fillStyle = '#fff';
        ctx.fillRect(0, 0, r.width, r.height);
      }
      ctx.strokeStyle = '#000';
      ctx.lineWidth = 2.5;
      ctx.lineCap = 'round';
      ctx.lineJoin = 'round';
    }
    size();
    cv.addEventListener('pointerdown', (e) => {
      e.preventDefault();
      cv.setPointerCapture(e.pointerId);
      const r = cv.getBoundingClientRect();
      drawing = true;
      last = { x: e.clientX - r.left, y: e.clientY - r.top };
      ctx.beginPath();
      ctx.moveTo(last.x, last.y);
    });
    cv.addEventListener('pointermove', (e) => {
      if (!drawing) return;
      const r = cv.getBoundingClientRect();
      const nx = e.clientX - r.left, ny = e.clientY - r.top;
      ctx.lineTo(nx, ny);
      ctx.stroke();
      last = { x: nx, y: ny };
    });
    cv.addEventListener('pointerup', () => { drawing = false; });
    clearBtn.addEventListener('click', () => {
      size();
      const r = cv.getBoundingClientRect();
      ctx.fillStyle = '#fff';
      ctx.fillRect(0, 0, r.width, r.height);
      out.textContent = '';
    });
    ocrBtn.addEventListener('click', async () => {
      ocrBtn.disabled = true;
      out.textContent = 'recognizing...';
      try {
        const res = await app.recognizeHandwriting(cv);
        out.textContent = res;
      } catch (err) {
        out.textContent = 'OCR unavailable: ' + (err && err.message ? err.message : err);
      }
      ocrBtn.disabled = false;
    });
    const ro = new ResizeObserver(size);
    ro.observe(cv);
    p.summary = () => 'pen input · OCR';
    if (p._summary && p.card && p.card.lastChild) p.card.lastChild.textContent = p.summary();
    p._ro = ro;
  }

  return { renderTree, renderGraph, renderEditor, renderHandwrite };}
