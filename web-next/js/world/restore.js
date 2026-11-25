/*
 * restore.js — rebuilding the world from a saved document.
 *
 * Extracted from the inline script in web-next/index.html. `restorePanes` mixed three things
 * that fail differently:
 *
 *   PLANNING    what the document says the world should contain — which clusters, which
 *               free tiles, which state each of those tiles carries. This is arithmetic over
 *               the document and belongs in a test: every rule below is a bug that shipped
 *               once.
 *   TEARING     the world that is already on screen has to go first, completely. A frame or
 *               a sash left behind is a plain world child that still sits on top of the new
 *               tiles — invisible, and grabbing the pointer over them.
 *   APPLYING    the calls the canvas already has: build a cluster, build a tile, re-tile it,
 *               put the camera back, restore the theme.
 *
 * The planning rules, and why each one exists:
 *
 *   · A KIND THE APP DOES NOT HAVE is dropped, not built. A workspace written by a newer
 *     build (or by a user's manifest that has since been deleted) must not produce a tile
 *     whose pane cannot be rendered — it produces a hole in the cluster and an exception in
 *     the body.
 *   · A TILE THE DOCUMENT LISTS TWICE is built once. Older documents wrote a cluster's tiles
 *     into `free` as well; rebuilding both gave two panes with one id, and the wire layer
 *     resolves a wire by id, so every wire to that tile became a coin toss.
 *   · AN ID ALREADY TAKEN gets a suffix instead of colliding. A saved id is KEPT when it is
 *     free — the document's ids are what the wires point at — but a restored world that
 *     already holds it (a document restored on top of a live one) must not end up with two
 *     tiles answering to the same name.
 *   · STATE IS ONLY CARRIED WHEN IT WAS WRITTEN. A kind's extra state (which file a corpus
 *     tile follows, whether its selection is shared, whether it highlights syntax) is
 *     recorded only when it was not the default, so a document is not a dump of every
 *     default the build happened to have. The state objects built here mirror that: an entry
 *     with nothing set contributes nothing, and the tile is born with the current defaults.
 */

/** The fields a saved tile may carry beyond its identity and its box. */
const STATE_FIELDS = ['pin', 'subject', 'selLocal', 'subjectPane', 'path'];

/** Does a saved tile entry carry any non-default state? */
export function hasState(k) {
  return !!(k && (k.pin || k.subject || k.selLink === false || k.syntax === false || k.subjectPane || k.path));
}

/** The state one saved tile hands to the factory, with the defaults filled in.
 *  `path` is the project tile's subject: the corpus directory it names (js/panes/project.js).
 *  Without it here a restored project tile comes back naming "whatever the server serves",
 *  which is a different workspace than the one that was saved. */
export function stateOf(k) {
  return {
    pin: !!k.pin,
    subject: k.subject || null,
    selLink: k.selLink !== false,
    selLocal: k.selLocal || null,
    syntax: k.syntax !== false,
    subjectPane: k.subjectPane || null,
    path: k.path || null,
  };
}

/**
 * What a saved layout asks for, as plain data, with nothing built yet.
 * @param {object} saved                  the document's `layout` (roots + free)
 * @param {object} o
 * @param {(kind:string) => boolean} o.known   does this build have that pane kind?
 * @param {Iterable<string>} [o.taken]    ids the world already holds
 * @returns {{ clusters: object[], free: object[] }}
 */
export function planRestore(saved, { known = () => true, taken = [] } = {}) {
  const held = new Set(taken);          // ids something in the world already answers to
  const inCluster = new Set();          // ids this document already placed in a cluster
  const clusters = [];
  const free = [];
  for (const r of (saved && saved.roots) || []) {
    const entries = (r.kinds || []).filter((k) => k && known(k.kind));
    const stateByKind = {};
    for (const k of entries) if (hasState(k)) stateByKind[k.kind] = stateOf(k);
    clusters.push({
      id: r.id || null,
      x: r.x || 0, y: r.y || 0, w: r.w || null, h: r.h || null,
      kinds: entries.map((k) => k.kind),          // empty means "the default set"
      stateByKind,
      tree: r.tree || null,
    });
    for (const k of entries) {
      const id = k.id || ('cluster:' + (r.id || '') + ':' + k.kind);
      inCluster.add(id);
      held.add(id);
    }
  }
  for (const f of (saved && saved.free) || []) {
    if (!f || !known(f.kind)) continue;
    const wanted = f.id || ('f:' + f.kind);
    // a document that also lists a cluster's tile as free: the cluster version won
    if (inCluster.has(wanted)) continue;
    let id = wanted;
    for (let n = free.length; held.has(id); n++) id = wanted + '~' + n;
    held.add(id);
    free.push({
      id, kind: f.kind,
      x: f.x || 0, y: f.y || 0, w: f.w || 380, h: f.h || 280,
      state: stateOf(f),
    });
  }
  return { clusters, free };
}

/**
 * @param {object} host
 * @param {Array} host.panes                 the live tile list (emptied and refilled)
 * @param {Array} host.clusters              the live cluster list
 * @param {(p) => void} host.dispose         take one tile apart
 * @param {(p) => void} host.detachEl        remove its element
 * @param {() => void} host.clearFrames      frames and sashes are separate world children
 * @param {() => void} host.resetTiles       the id sequences start over with the world
 * @param {(ox,oy,kinds,box,reuse,state) => object} host.addCluster
 * @param {(id,kind,wr,home,state) => object} host.make
 * @param {(c) => void} host.relayout
 * @param {(tree) => object|null} host.treeFromJSON
 * @param {(path,text) => object} host.docOf
 * @param {(path:string) => boolean} host.hasDoc
 * @param {(path:string) => void} host.setActive
 * @param {{x:number,y:number,z:number}} host.cam
 * @param {(z:number) => number} [host.clampZ]
 * @param {() => void} host.applyCam
 * @param {() => void} host.updateLOD
 * @param {(saved:object) => void} [host.theme]
 * @param {(m:string) => void} [host.toast]
 */
export function createRestore(host) {
  const { panes, clusters, cam } = host;
  const toast = host.toast || (() => {});
  const clampZ = host.clampZ || ((z) => z);

  /** Rebuild the world from a saved layout: exact rects, the split ratios, the camera. */
  function apply(saved) {
    if (!saved) return false;
    // 1. the world that is on screen goes first, whole
    panes.slice().forEach((p) => { host.dispose(p); host.detachEl(p); });
    panes.length = 0;
    clusters.length = 0;
    host.resetTiles();
    host.clearFrames();

    // 2. plan, then build
    const plan = planRestore(saved, { known: host.known || (() => true) });
    for (const r of plan.clusters) {
      const c = host.addCluster(r.x, r.y, r.kinds.length ? r.kinds : undefined, null, null, r.stateByKind);
      if (!c) continue;
      if (r.id) c.id = r.id;
      if (r.w && r.h) { c.w = r.w; c.h = r.h; }
      // a saved split tree keeps the sash ratios the user dragged to
      const t = host.treeFromJSON ? host.treeFromJSON(r.tree) : null;
      if (t) { c.tree = t; host.relayout(c); }
    }
    for (const f of plan.free) {
      host.make(f.id, f.kind, { x: f.x, y: f.y, w: f.w, h: f.h }, 'free', f.state);
    }

    // 3. the files it had open, and which of them were unsaved
    for (const d of saved.docs || []) {
      if (!d || !d.path) continue;
      host.docOf(d.path, d.text).dirty = true;
    }
    if (saved.active) {
      if (!host.hasDoc(saved.active)) host.docOf(saved.active);
      host.setActive(saved.active);
    }

    // 4. where you were looking, and what it looked like
    if (saved.canvas) {
      cam.x = saved.canvas.panX || 0;
      cam.y = saved.canvas.panY || 0;
      cam.z = clampZ(saved.canvas.zoom || 1);
      host.applyCam();
      host.updateLOD();
    }
    if (host.theme) host.theme(saved);
    toast('layout restored');
    return true;
  }

  return { apply, plan: (saved, o) => planRestore(saved, o) };
}