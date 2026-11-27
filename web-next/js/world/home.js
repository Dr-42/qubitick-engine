/*
 * world/home.js — the arrangement the workspace ships with, and the one promise about it.
 *
 * Extracted from the inline script in web-next/index.html. Two things live here, and they are
 * the same subject seen twice: what a workspace looks like when nobody has arranged one, and
 * what happens when a workspace arrives WITHOUT the tiles that control it.
 *
 *   THE SHIPPED ARRANGEMENT is two roots side by side — the writing one, which owns the middle
 *   of the screen, and a reference column — with the corpus pair above the column and the
 *   console below it. It is what a first visit gets, and what "default view" restores: the
 *   ARRANGEMENT and the camera, not the theme, the wires or the preferences, which are the
 *   user's.
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

  /**
   * The arrangement the workspace ships with.
   *
   * TWO ROOTS, AND THE WRITING ONE IS THE FOCUS. The first cluster is the arrangement you
   * prove in — the tree to pick a node, the editor to write it, the emitted Lean and the goal
   * at the caret beside it — and it is the big, central root, because that is what you look at
   * while working. Everything else is still here, in the second root: the graph, the symbol
   * table and the stats are tiles like any other, wireable and draggable, just not competing
   * for the middle of the screen.
   *
   * It is still THE CANVAS: same tiles, same wires, same clusters, same sashes. The focus is a
   * matter of which root is biggest and first, not of a different application — and any tile
   * can be dragged into (or out of) either root, so this is a starting point rather than a
   * container.
   */
  const FOCUS_KINDS = ['tree', 'editor', 'render', 'goal'];
  const REFERENCE_KINDS = ['graph', 'symbols', 'stats'];

  /* THE ARRANGEMENT AROUND THE FOCUS, in world px. The focus root is the one the camera frames
     (defaultCam), so its size is what decides whether the writing arrangement opens legible: at
     1440x900 a 1400x900 root fits at ~85%, which is a workspace, and the whole canvas fits at
     38%, which is a map (js/app/viewport.js::firstViewCam).

     The other roots sit on a side each — corpus pair above, reference below-right, console below —
     so each is ONE pan in a direction rather than somewhere off-diagonal, and each is the thing
     that direction naturally holds: what corpus am I in (up), what else does it contain (right),
     how do I drive the workspace (down). */
  const FOCUS_BOX = { x: 0, y: 0, w: 1400, h: 760 };
  const COLUMN_X = 1520, COLUMN_W = 900, ROW_GAP = 140, PAIR_DY = -240, CONSOLE_W = 1240;
  /* WHAT THE FIRST VIEW FRAMES: the writing root AND the corpus pair above it. The pair is not
     optional here for a mechanical reason, not a cosmetic one — nothing off screen is built
     (js/world/tile-mount.js), the corpus TILE is where the interface gets its corpus
     (js/panes/app-mounts.js::corpusFrom), so a first view that frames only the writing root leaves
     the corpus tile unbuilt and the app opens empty. Including it costs 8 percentage points of
     zoom (0.91 -> 0.83 at 1500x950) and buys a readable arrangement that has data in it. */
  const OPEN_BOX = { x: 0, y: PAIR_DY, w: FOCUS_BOX.w, h: FOCUS_BOX.h - PAIR_DY };

  function buildDefault() {
    const focus = host.addCluster(FOCUS_BOX.x, FOCUS_BOX.y, FOCUS_KINDS, { w: FOCUS_BOX.w, h: FOCUS_BOX.h });
    host.addCluster(COLUMN_X, 0, REFERENCE_KINDS, { w: COLUMN_W, h: FOCUS_BOX.h });
    ensureCorpus({ x: FOCUS_BOX.x, y: PAIR_DY });
    ensureConsole({ x: FOCUS_BOX.x, y: FOCUS_BOX.h + ROW_GAP, w: CONSOLE_W });
    return focus;
  }

  /**
   * The camera the shipped arrangement OPENS at: the writing root, framed.
   *
   * Framing the whole canvas is the wrong question here — the roots around the focus are more world
   * than any viewport, so a fit of everything lands under the readable threshold and the
   * legible-zoom fallback then anchors the top-left corner of a 2420x2160 world, which puts the
   * arrangement in a quarter of the screen and the console past the bottom edge. The root you work
   * in is the root that opens; the reference root and the console are one pan away each, which is
   * what a canvas is for ("fit all" frames everything when that is what you want).
   */
  function defaultCam() {
    return host.firstView(OPEN_BOX, 60);
  }

  /**
   * Make the project/corpus pair — and the wire between them — when the workspace has neither.
   *
   * The pair is where the interface's corpus comes from: the project tile names a project, the
   * corpus tile serves it, and the app reads the corpus tile's ports. In the shipped arrangement
   * they sit at the top of the reference column (`at`); a workspace that was already arranged
   * gets them above its own bounding box instead, and one that already has them is left exactly
   * as it is (the wire is the user's to snap and re-make).
   */
  function ensureCorpus(at) {
    const has = (kind) => panes.some((p) => p.kind === kind);
    const made = [];
    const box = at || (panes.length ? host.bbox() : { x: 0, y: 0, w: FOCUS_BOX.w, h: FOCUS_BOX.h });
    /* 186 tall, not 150: the corpus tile's readout is a title and three rows (files, nodes,
       symbols), and a tile it cannot fit is a tile that hides one of its own numbers */
    if (!has('project')) {
      host.make('p:project', 'project', { x: Math.round(box.x), y: Math.round(box.y), w: 320, h: 186 });
      made.push('project');
    }
    if (!has('corpus')) {
      host.make('p:corpus', 'corpus', { x: Math.round(box.x + 360), y: Math.round(box.y), w: 320, h: 186 });
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
       that opens as an unreadable map is not one worth restoring (defaultCam above) */
    Object.assign(cam, defaultCam());
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
  function ensureConsole(at) {
    const kinds = consoleKinds();
    const missing = kinds.filter((k) => !panes.some((p) => p.kind === k));
    if (!missing.length) return [];
    if (at) {
      host.addCluster(Math.round(at.x), Math.round(at.y), missing, { w: at.w || 1240, h: 820 });
      toast('console tiles: ' + missing.join(', '));
      return missing;
    }
    const box = panes.length ? host.bbox() : { x: 0, y: 0, w: FOCUS_BOX.w, h: 820 };
    const ox = Math.round(box.x);
    // below what is there, never on top of it
    const oy = Math.round(box.y + (panes.length ? box.h + 140 : 0));
    host.addCluster(ox, oy, missing, { w: 1240, h: 820 });
    toast('console tiles: ' + missing.join(', '));
    return missing;
  }

  return { buildDefault, defaultCam, resetToDefault, ensureConsole, ensureCorpus };
}