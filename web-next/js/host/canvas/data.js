/*
 * host/canvas/data.js — loading the corpus, and the two little services everything uses.
 *
 * Extracted from the inline script in web-next/index.html. The workspace can be dragged around
 * before any of this has landed, so the loading is written to be a PROGRESS, not a gate:
 *
 *   · each step (files, graph, stats, symbols) is its own request, and a failure logs and
 *     carries on — a corpus with no symbol table is still a corpus you can read;
 *   · the progress bar moves because each step says so, so a slow server looks slow rather
 *     than broken;
 *   · three file bodies are fetched EAGERLY, because the editor and the render tile should
 *     show real content on arrival instead of a placeholder that fills in a second later;
 *   · the first corpus file is opened and the data panes are rebuilt once the bodies are
 *     cached, which is the one moment when "rebuild everything" is cheaper than being lazy.
 *
 * `toast` and the load bar live here too: they are the two pieces of feedback the app has, and
 * they belong with the thing that reports progress.
 */

/**
 * @param {object} app
 * @param {(sel:string) => Element|null} app.el
 * @param {() => string} app.url                  the route table
 * @param {Promise<any>} app.routesReady
 * @param {object} app.data                       the corpus (mutated)
 * @param {() => object} app.editor
 * @param {Array} app.panes
 * @param {(p:object) => void} app.render
 * @param {() => Array} app.tiles                 the live tiles
 * @param {(path:string) => void} app.setActive
 * @param {(path:string) => Promise<string>} app.readFile   one file body, with the app's cache
 * @param {(m:string) => void} [app.warn]
 */
export function createData(app) {
  const warn = app.warn || ((m) => console.warn(m));

  /** One line of feedback at the top of the page, for a couple of seconds. */
  const toast = (msg) => {
    const t = app.el('#toast');
    if (!t) return;
    t.textContent = msg;
    t.style.opacity = 1;
    setTimeout(() => { t.style.opacity = 0; }, 1600);
  };

  /** The load bar: a proportion, then out of the way. */
  function setLoad(p) {
    const b = app.el('#loadbar');
    if (!b) return;
    b.style.opacity = 1;
    b.style.width = (p * 100) + '%';
    if (p >= 1) setTimeout(() => { b.style.opacity = 0; }, 300);
  }

  async function fetchJSON(url) {
    const r = await fetch(url);
    if (!r.ok) throw new Error(url + ' ' + r.status);
    return r.json();
  }

  /** Open a corpus file into the active document, caching its body on the way. */
  async function openFile(path) {
    path = path || (app.data.files[0] || '');
    if (!path) return;
    if (app.readFile) await app.readFile(path);
    app.setActive(path);
  }

  /**
   * WHERE THE CORPUS COMES FROM: the corpus TILE.
   *
   * The app does not fetch the corpus any more — `js/panes/corpus.js` does, for whatever project
   * is wired into it, and this reads that tile's four out-ports into `data`, which is what every
   * pane in the interface draws from. The consequence is the point: with a project wired into the
   * corpus tile the interface is that project, wire a different project in and the whole thing
   * changes over, and with the wire OUT the corpus is empty and the interface empties with it.
   *
   * The per-corpus work (eager file bodies, the first file, the repaint) happens each time a
   * corpus ARRIVES — every load is a new one, because the project behind it is a new one.
   */
  let corpusSeq = 0;
  let repainting = false;      // `repaintData` does not re-enter (see it below)

  /** Follow a corpus tile: what it publishes is the app's corpus.
   *
   *  NO OTHER BINDING STEP EXISTS, and that is deliberate: the tile is handed to this function by
   *  the mount table when the tile MOUNTS (js/panes/app-mounts.js), so a corpus tile that appears
   *  later — dropped in by the add-tile menu, restored from a saved document — is followed too,
   *  and the boot has nothing to call.
   *
   *  ONE CORPUS AT A TIME. A workspace may hold several corpus tiles (two projects kept side by
   *  side, each wired to its own), and `data` is one object: with a subscription per tile the two
   *  would take turns writing it, and whichever published last would win — a corpus that flips
   *  because an unrelated tile refreshed. So following a tile UNFOLLOWS the last one: the app
   *  reads the newest corpus tile to mount, and its wire is what the interface is. */
  let following = null;         // the tile the app is reading, and the unsubscribe that stops it

  function corpusFrom(inst) {
    if (following) { try { following(); } catch (e) {} following = null; }
    if (!inst || !inst.out) return () => {};
    const offs = [];
    for (const field of ['files', 'graph', 'stats', 'symbols']) {
      const p = inst.out[field];
      if (!p || typeof p.subscribe !== 'function') continue;
      offs.push(p.subscribe((v) => {
        const value = (field === 'files') ? (Array.isArray(v) ? v.slice() : []) : (v === undefined ? null : v);
        app.data[field] = value;
        onCorpusChanged(field);
      }));
    }
    const off = () => { for (const o of offs) { try { o(); } catch (e) {} } };
    following = off;
    return off;
  }

  /** A field of the corpus changed: repaint what reads it, and do the once-per-corpus work. */
  function onCorpusChanged(field) {
    const data = app.data;
    const st = data.stats || {};
    const hd = app.el('#h-data');
    if (hd) hd.textContent = (st.nodes || (data.files || []).length) + ' · ' + (st.symbols || '–') + ' · ' + (st.operations || '–');

    if (field === 'files') {
      const empty = !(data.files || []).length;
      if (empty) {
        corpusSeq++;
        setLoad(1);
        repaintData();
        toast('no corpus: wire a project into the corpus tile');
        return;
      }
      const mine = ++corpusSeq;
      setLoad(0.9);
      (async () => {
        // eager file bodies: the editor and the render tile should arrive full, not blank
        const picks = data.files.filter((p) => /Axioms|Definitions/.test(p)).slice(0, 3);
        for (const p of picks) {
          if (app.readFile) { try { await app.readFile(p); } catch (e) { /* one file is not the corpus */ } }
        }
        if (mine !== corpusSeq) return;             // another project arrived meanwhile
        await openFile(data.files[0] || '');        // editor + render show real content
        if (mine !== corpusSeq) return;
        setLoad(1);
        repaintData();
        toast('corpus: ' + ((st.nodes || data.files.length) || '?') + ' nodes · ' + (app.editor().file.get() || ''));
      })();
      return;
    }
    repaintData();
  }

  /**
   * Rebuild the tiles that draw the corpus.
   *
   * NOT THE SOURCE TILES THEMSELVES, and not re-entrantly. A corpus tile is the SOURCE of
   * everything being repainted here, and rendering it remounts it — which re-subscribes the app
   * to its ports (`corpusFrom` above), and a subscription delivers the current value the moment
   * it is made, so a repaint that renders the corpus tile calls itself: mount, subscribe,
   * repaint, mount... to a stack overflow (`Maximum call stack size exceeded`, caught by the port
   * layer and logged once per frame). The project tile is the same argument one step further up:
   * it is what FEEDS the corpus tile, and remounting it resets its `project` port to "nothing
   * yet" — the corpus would blink empty on the next input pass and fetch itself again. Neither
   * tile draws the corpus, and both paint their own faces, so skipping them costs nothing;
   * `repainting` covers the other direction, a paint that arrives while this loop is already
   * running from somewhere else (a LOD change, a resize).
   */
  function repaintData() {
    if (repainting) return;
    repainting = true;
    try {
      const list = app.tiles ? app.tiles() : app.panes;
      for (const p of list) {
        if (p.kind === 'handwrite' || p.kind === 'corpus' || p.kind === 'project') continue;
        app.render(p);
      }
    } finally {
      repainting = false;
    }
  }

  return { toast, setLoad, fetchJSON, openFile, corpusFrom };}
