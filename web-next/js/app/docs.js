/*
 * docs.js — documents, views, and the ports the editor exposes.
 *
 * Extracted from the inline script in web-next/index.html, and it is the model the editor
 * is built on:
 *
 *   a DOCUMENT is one per source file. It owns the text, the unsaved flag, the single undo
 *   history and a port that carries the text to anyone watching. A VIEW is a tile: it owns
 *   its own cursor, selection and scroll, and points at a document. Two editor tiles on
 *   one file are two views of one document and stay in step by themselves; two tiles on
 *   different files share nothing at all.
 *
 *   a tile either FOLLOWS the active document or is PINNED to its own subject (Blender's
 *   pin on a node editor, Obsidian's linked pane): a follower is retargeted whenever the
 *   active file moves, a pinned tile is not.
 *
 * The subsystem talks to the app through four injected things — how to read a file, how to
 * mark the workspace dirty, how to repaint a tile's chrome, and the app's file-text cache.
 * Everything else (the map, the active document, the ports) is its own.
 *
 * The editor is a PRODUCER: `EDITOR.file`, `.cursor`, `.buffer`, `.dirty` are value ports,
 * and the canvas wires them to the panes that consume them. The tree is a producer too.
 */

/** A value port: current value, subscribers, and an immediate call on subscribe. */
export function makePort(initial, kind = 'value') {
  let v = initial;
  const subs = new Set();
  return {
    kind,
    get: () => v,
    set(nv) {
      if (kind === 'value' && Object.is(nv, v)) return;
      v = nv;
      for (const f of [...subs]) f(v);
    },
    on(f) { subs.add(f); if (kind === 'value') f(v); return () => subs.delete(f); },
  };
}

/**
 * @param {object} deps
 * @param {(path: string) => Promise<string>} deps.readFile   fetch a file's text
 * @param {() => void} deps.markDirty                          the workspace has unsaved work
 * @param {(p: object) => void} deps.paint                     repaint a tile's chrome
 * @param {(path: string) => string|undefined} deps.cachedText  text already in hand
 */
export function createDocs({ readFile, markDirty, paint, cachedText }) {
  // the editor is a PRODUCER module exposing output ports
  const EDITOR = {
    file: makePort(null),                      // value: current file path
    cursor: makePort({ line: 1, col: 1 }),     // value: cursor position
    buffer: makePort(''),                      // value: current buffer text
    dirty: makePort(false),                    // value: unsaved edits?
  };
  // the corpus TREE is a producer module: selection + file output drive the graph/symbols
  const TREE = {
    selection: makePort(null),                 // value: selected node id
    file: makePort(null),                      // value: selected file path
  };

  const DOCS = new Map();                      // path -> document
  let ACTIVE = null;                           // the document the rail and the wires watch

  function docOf(path, text) {
    let d = DOCS.get(path);
    if (!d) {
      d = {
        path, text: text !== undefined ? text : (cachedText(path) || ''),
        dirty: false, views: new Set(), primary: null, port: makePort(''),
      };
      d.port.set(d.text);
      DOCS.set(path, d);
    } else if (text !== undefined && text !== d.text) {
      d.text = text;
      d.port.set(text);
    }
    return d;
  }

  /** One view changed a document: tell the document, then its OTHER views. */
  function docEdited(d, text, origin, changes) {
    d.text = text;
    d.dirty = true;
    d.port.set(text);
    for (const v of [...d.views]) {
      if (v === origin || !v.applyRemote || !changes) continue;   // never echo a change back
      try { v.applyRemote(changes); }
      catch (e) {
        // a view that has drifted cannot be patched with a change set; hand it the whole
        // document instead of dropping it out of the document for good
        console.warn('[doc] a view could not take the change, resyncing it:', e && e.message,
          '| change newLen', changes && changes.length, '| view len', v.len && v.len(),
          '| doc len', (text || '').length);
        try { v.resync && v.resync(text); } catch (e2) { d.views.delete(v); }
      }
    }
    if (d === ACTIVE) { EDITOR.buffer.set(text); EDITOR.dirty.set(true); }
    if (markDirty) markDirty();
  }

  function setActive(path) {
    const d = DOCS.get(path) || (path ? docOf(path) : null);
    ACTIVE = d;
    EDITOR.file.set(d ? d.path : null);
    EDITOR.buffer.set(d ? d.text : '');
    EDITOR.dirty.set(d ? d.dirty : false);
    return d;
  }

  async function loadText(path) {
    if (!path) return '';
    const fresh = cachedText(path) === undefined;
    if (fresh) {
      let text = '';
      try { text = await readFile(path); } catch (e) { text = ''; }
      cachedText.set(path, text);
    }
    // a document may have been created before its text arrived (a restored active file,
    // say): it is only ever seeded once, so an emptied buffer stays emptied
    if (fresh) {
      const d = DOCS.get(path);
      const now = cachedText(path);
      if (d && !d.text && now) {
        d.text = now;
        d.port.set(d.text);
        if (d === ACTIVE) EDITOR.buffer.set(d.text);
      }
    }
    return cachedText(path) || '';
  }

  /** A tile joins a document's view list and reads its text from then on. */
  function showDoc(p, d, opts = {}) {
    if (p.doc === d && !opts.force) return;
    const prev = p.doc;
    if (prev && prev.views) {
      prev.views.delete(p.view);
      if (prev.primary === p.ed) prev.primary = null;
    }
    p.doc = d || null;
    p.subject = d ? d.path : null;
    if (d && !d.primary && p.ed && p.ed.undo) d.primary = p.ed;      // the owner left; adopt it
    if (d && p.view && p.docView) d.views.add(p.view);               // watchers are not views
    if (p.ed && prev !== d) {
      if (p.ed.swapDoc) p.ed.swapDoc(d ? d.text : '');
      else p.ed.setDoc(d ? d.text : '');
    }
    if (p.retarget) p.retarget();            // the tile's own subscriptions moved with it
    if (paint) paint(p);
  }

  /** A text tile follows the active document unless it is pinned. */
  function watchActive(p) {
    if (!p.view) {
      p.view = {
        seen: p.id,
        applyRemote: (ch) => { if (p.ed && p.ed.applyRemote) p.ed.applyRemote(ch); },
      };
    }
    p.view.resync = (text) => { if (p.ed && p.ed.swapDoc) p.ed.swapDoc(text); };
    p.view.len = () => (p.ed ? (p.ed.getDoc() || '').length : -1);
    if (p._actOff) p._actOff();               // never leave two subscriptions behind
    p._actOff = EDITOR.file.on((path) => {
      if (p.pin) return;
      showDoc(p, path ? docOf(path) : null);
    });
  }

  /** Re-run `onChange` when this tile's document changes, and again when it retargets. */
  function docBound(p, onChange) {
    p.retarget = () => {
      if (p._docOff) { p._docOff(); p._docOff = null; }
      if (p.doc) p._docOff = p.doc.port.on(onChange);
      if (paint) paint(p);
    };
    p.retarget();
    onChange();
  }

  /**
   * The document's text is on the server now.
   *
   * `docEdited` is the only other writer of `dirty`, so a save that did not clear it here left the tile
   * marked unsaved for ever — the rail's `● unsaved` and the editor bar's dot were both readings of
   * this one flag, and nothing ever set it back.
   */
  function docSaved(d) {
    if (!d) return;
    d.dirty = false;
    if (d === ACTIVE) EDITOR.dirty.set(false);
    /* NOT a repaint from here: `d.views` holds CodeMirror views (they get change sets, not chrome), and
       the tiles that show this document are the caller's to know. `doc-save.js` repaints them. */
  }

  return {
    EDITOR, TREE, DOCS,
    docOf, docEdited, docSaved, setActive, loadText, showDoc, watchActive, docBound,
    get active() { return ACTIVE; },
    setActiveDoc(d) { ACTIVE = d; },
    docText: (p) => (p.doc && p.doc.text) || '',
    docPath: (p) => (p.doc && p.doc.path) || '',
  };
}