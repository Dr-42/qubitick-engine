/*
 * world/home.js — the arrangement the workspace ships with, and the one promise about it.
 *
 * Extracted from the inline script in web-next/index.html. Two things live here, and they are
 * the same subject seen twice: what a workspace looks like when nobody has arranged one, and
 * what happens when a workspace arrives WITHOUT the tiles that control it.
 *
 *   THE SHIPPED ARRANGEMENT is one corpus cluster, one isolated box beside it, and the
 *   console cluster below. It is what a first visit gets, and what "default view" restores —
 *   the ARRANGEMENT and the camera, not the theme, the wires or the preferences, which are
 *   the user's.
 *
 *   THE CONSOLE IS NOT OPTIONAL. A layout saved by an older build (or a foreign document) may
 *   have no console tiles at all, and a workspace that cannot control itself — cannot add a
 *   tile, save, frame or reset — is a trap with no exit that is not a reload. So the missing
 *   console kinds are made, and only the MISSING ones: a workspace that already has them is
 *   left exactly as it is, because "make sure" is not "make again".
 *
 * The console goes BELOW whatever is there rather than on top of it: dropping a new cluster
 * over the user's tiles hides the work they were doing.
 *
 *   THE CORPUS IS NOT OPTIONAL EITHER, and for the same reason: since the app stopped fetching the
 *   corpus itself and reads it off the corpus TILE (js/panes/corpus.js, js/host/canvas/data.js), a
 *   workspace with no corpus tile — one saved before the pair existed, or one whose tile the user
 *   closed — would boot with an empty interface and no way to fill it. So the pair is made when it
 *   is missing: a project tile, a corpus tile, and the wire between them.
 */

/**
 * @param {object} host
 * @param {Array} host.panes
 * @param {Array} host.clusters
 * @param {(id:string, kind:string, wr:object, home?:string) => object} host.make  build one tile
 * @param {(ox:number, oy:number, kinds?:string[], box?:object) => object} host.addCluster
 * @param {(x:number,y:number,w:number,h:number,label:string) => void} host.frameBox
 * @param {() => void} host.clearFrames
 * @param {() => void} host.resetTiles          the id sequences start over
 * @param {(p:object) => void} host.dispose
 * @param {(p:object) => void} host.detachEl
 * @param {() => object} host.bbox
 * @param {(wr:object, pad?:number) => object} host.firstView   the camera a first render gets
 * @param {(wr:object, pad?:number) => object} host.camFor
 * @param {object} host.cam
 * @param {() => void} host.applyCam
 * @param {() => void} host.updateLOD
 * @param {() => void} [host.exitFullscreen]
 * @param {() => string[]} [host.consoleKinds]  the kinds the console is made of
 * @param {() => void} [host.ensureProjectWire]  wire the project tile into the corpus tile
 * @param {(m:string) => void} [host.toast]
 * @param {() => void} [host.markDirty]
 * @param {() => void} [host.save]
 */
export function createHome(host) {
  const { panes, clusters, cam } = host;
  const toast = host.toast || (() => {});
  const consoleKinds = () => (host.consoleKinds ? host.consoleKinds() : []);

  /** The arrangement the workspace ships with. */
  function buildDefault() {
    const first = host.addCluster(0, 0);
    // the isolated box: a single tile with a frame of its own, off to the side
    host.frameBox(2020, -170, 472, 412, 'root s · isolated');
    host.make('s:stats', 'stats', { x: 2040, y: -150, w: 440, h: 380 });
    ensureCorpus();
    ensureConsole();
    return first;
  }

  /**
   * Make the project/corpus pair — and the wire between them — when the workspace has neither.
   *
   * The pair is where the interface's corpus comes from: the project tile names a project, the
   * corpus tile serves it, and the app reads the corpus tile's ports. They ship as a small
   * isolated root, and a workspace that already has them is left exactly as it is (the wire is
   * the user's to snap and re-make).
   */
  function ensureCorpus() {
    const has = (kind) => panes.some((p) => p.kind === kind);
    const made = [];
    const box = panes.length ? host.bbox() : { x: 0, y: 0, w: 1400, h: 820 };
    /* 186 tall, not 150: the corpus tile's readout is a title and three rows (files, nodes,
       symbols), and a tile it cannot fit is a tile that hides one of its own numbers */
    if (!has('project')) {
      host.make('p:project', 'project', { x: Math.round(box.x), y: Math.round(box.y - 300), w: 320, h: 186 });
      made.push('project');
    }
    if (!has('corpus')) {
      host.make('p:corpus', 'corpus', { x: Math.round(box.x + 360), y: Math.round(box.y - 300), w: 320, h: 186 });
      made.push('corpus');
    }
    if (made.length && host.ensureProjectWire) host.ensureProjectWire();
    if (made.length) toast('corpus tiles: ' + made.join(', '));
    return made;
  }

  /** Put the views back the way they ship: the arrangement and the camera, nothing else. */
  function resetToDefault() {
    if (host.exitFullscreen) host.exitFullscreen();
    panes.slice().forEach((p) => { host.dispose(p); host.detachEl(p); });
    panes.length = 0;
    clusters.length = 0;
    host.resetTiles();
    host.clearFrames();
    buildDefault();
    /* the same camera a first visit gets: "default view" restores the ARRANGEMENT, and a default
       that opens as an unreadable map is not one worth restoring (js/app/viewport.js::firstViewCam) */
    Object.assign(cam, host.firstView(host.bbox(), 60));
    host.applyCam();
    host.updateLOD();
    toast('default view restored');
    if (host.markDirty) host.markDirty();
    if (host.save) host.save();
  }

  /**
   * Make the console tiles a layout is missing — and only those. Returns the kinds it had to
   * add (empty when the workspace could already control itself).
   */
  function ensureConsole() {
    const kinds = consoleKinds();
    const missing = kinds.filter((k) => !panes.some((p) => p.kind === k));
    if (!missing.length) return [];
    const box = panes.length ? host.bbox() : { x: 0, y: 0, w: 1400, h: 820 };
    const ox = Math.round(box.x);
    // below what is there, never on top of it
    const oy = Math.round(box.y + (panes.length ? box.h + 140 : 0));
    host.addCluster(ox, oy, missing, { w: 1240, h: 820 });
    toast('console tiles: ' + missing.join(', '));
    return missing;
  }

  return { buildDefault, resetToDefault, ensureConsole, ensureCorpus };
}