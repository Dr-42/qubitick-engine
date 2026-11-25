/*
 * host/canvas/tile-state.js — what a tile's own state MEANS: its pin, its selection, and the
 * three kind tables that decide which tiles have either.
 *
 * Extracted from the inline script in web-next/index.html. None of it is about drawing; it is
 * about the two questions a corpus tile answers for itself, and the tables that say who may
 * answer them:
 *
 *   PIN   a tile that SHOWS corpus text may follow the active file or be pinned to one. Pinned
 *         is a reference: the file it was pinned to is the file it keeps, whatever the workspace
 *         does around it. Unpinning goes back to following — and says which file that is now,
 *         not the one it was pinned to.
 *
 *   LINK  a tile that can PICK (the tree, the graph) may share the workspace's selection or
 *         keep its own. Linked, picking there moves what every corpus tile shows; unlinked it is
 *         a reference view you can browse without dragging the workspace along. A read-out has
 *         no selection of its own to keep, which is why only two kinds carry the control.
 *
 * The three tables are DATA and live here so "which kinds follow the active file" is a fact
 * you can read, instead of something nineteen wrappers implied.
 */

/** Kinds that show corpus text: they can be pinned to a file. */
export const DOC_KINDS = new Set(['editor', 'render', 'ir', 'lean', 'goal']);

/** Kinds that read the corpus and can PICK: their selection can be their own. */
export const LINK_KINDS = new Set(['tree', 'graph']);

/** Kinds with a syntax-highlighting switch (token colours in the editor, maths in render). */
export const SYNTAX_KINDS = new Set(['editor', 'render']);

/**
 * @param {object} app
 * @param {object} app.tree        { selection, file } ports
 * @param {() => object|null} app.activeDoc
 * @param {(p:object, d:object) => void} app.showDoc
 * @param {(p:object) => void} app.paintChrome
 * @param {(m:string) => void} app.toast
 * @param {() => void} app.save
 */
export function createTileState(app) {
  const linkedSel = (p) => p.selLink !== false;

  /** The selection this tile shows: the shared one, or its own. */
  const selOf = (p) => (p.selLink === false ? (p.selLocal ?? null) : app.tree.selection.get());

  /** Set it — into the workspace's selection when linked, into the tile's own when not. */
  function setSel(p, id) {
    if (linkedSel(p)) app.tree.selection.set(id);
    else { p.selLocal = id; app.paintChrome(p); }
  }

  /** Pin a tile to its subject (or let it follow the active file again). */
  function pinTile(p, on) {
    p.pin = !!on;
    if (p.pin) p.pinned = p.subject || (p.doc && p.doc.path) || null;
    else { p.pinned = null; app.showDoc(p, app.activeDoc()); }
    app.paintChrome(p);
  }

  /** Repaint the three pieces of chrome that state can change. */
  function paintChrome(p) {
    if (!p) return;
    try { if (p.chrome) p.chrome(); } catch (e) { /* a pane's own paint */ }
    try { if (p.pinPaint) p.pinPaint(); } catch (e) { /* the pin names the subject too */ }
    try { if (p.linkPaint) p.linkPaint(); } catch (e) { /* and the link names the selection */ }
  }

  /** The pin button, painted from the tile's own state. */
  function wirePin(p) {
    const btn = p.head.querySelector('[data-a="pin"]');
    if (!btn) return;
    const paint = () => {
      const d = p.doc;
      btn.classList.toggle('on', !!p.pin);
      const name = (d && d.path ? d.path.split('/').pop() : '') || 'the active file';
      btn.title = p.pin
        ? ('pinned to ' + name + ' — click to follow the active file')
        : ('following ' + name + ' — click to pin this tile to it');
      btn.textContent = p.pin ? '📌' : '◎';
    };
    p.pinPaint = paint;
    paint();
  }

  /** The link button, painted from the tile's own state. */
  function wireSel(p) {
    const btn = p.head.querySelector('[data-a="link"]');
    if (!btn) return;
    const paint = () => {
      btn.classList.toggle('on', linkedSel(p));
      const id = selOf(p);
      const what = id ? String(id).split('/').pop() : 'the active selection';
      btn.title = linkedSel(p)
        ? ('selection is shared — tracking ' + what + ' · click to give this tile its own')
        : ('its own selection: ' + what + ' · click to follow the shared one');
      btn.textContent = linkedSel(p) ? '⌖' : '◌';
    };
    p.linkPaint = paint;
    paint();
  }

  /** Toggle the link: going back to shared drops the tile's own selection, and going private
   *  takes the shared one as its starting point (so the tile does not jump). */
  function toggleSelLink(p) {
    p.selLink = !linkedSel(p);
    if (p.selLink) p.selLocal = null;
    else p.selLocal = app.tree.selection.get();
    if (p.linkPaint) p.linkPaint();
    paintChrome(p);
    app.toast(p.selLink ? 'selection: shared' : 'selection: this tile only');
    app.save();
  }

  function togglePin(p) {
    pinTile(p, !p.pin);
    if (p.pin) app.toast('pinned to ' + ((p.subject || '').split('/').pop() || '—'));
    else app.toast('following the active file');
    app.save();
  }

  return {
    DOC_KINDS, LINK_KINDS, SYNTAX_KINDS,
    linkedSel, selOf, setSel, pinTile, paintChrome, wirePin, wireSel, toggleSelLink, togglePin,
  };}
