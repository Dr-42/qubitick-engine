/**
 * renderer.js — layout tree → DOM renderer (M2.07 Part A).
 *
 * On every store change (subscribed, coalesced into one governor.requestFrame):
 * - for each root compute rects with computeRects
 * - reconcile DOM by key (data-root, data-path, data-pane)
 *   — reuse existing elements, create missing ones, remove stale ones
 *   — set left/top/width/height only if changed
 * - Leaf .pane gets .pane-head (title from pane definition, buttons none yet)
 *   and .pane-body; instances.mount(id, bodyEl) is called when a pane
 *   first appears **and is not culled/inactive**; unmount when the pane
 *   disappears from the layout.
 * Panes keep their DOM element identity across layout changes
 * (never re-create .pane-body of a surviving pane — moving it with
 * appendChild is fine).
 */

import { computeRects } from './geometry.js';
import { renderTabs } from './renderer-tabs.js';
import { renderSplit } from './renderer-split.js';
import { kindOf, isLeaf, isTabs, isSplit, isGroup } from './schema.js';
import { createPaneElement, updatePaneLOD, updatePaneRect } from './renderer-pane.js';
import { renderModal } from './renderer-modal.js';
import { nextTab, touchMru } from '../engine/tabs.js';
import { attachSash } from '../engine/sash.js';
import { addPaneChrome } from '../engine/pane-chrome.js';
import { createContextMenu } from '../design/context-menu.js';
/* the anchors menu is this menu's: its items name their action, and js/design/anchor-ui.js
   both writes them (the submenu of valid parents) and applies them (`anchor.set`) */
import { handleAnchorSet, handleClearAnchor } from '../design/anchor-ui.js';

// DOM attribute constants
const DATA_ROOT = 'data-root';
const DATA_PATH = 'data-path';
const DATA_PANE = 'data-pane';
const DATA_LOD = 'data-lod';

// CSS class prefixes
const ROOT_PREFIX = 'qb-root-';
const HUD_PREFIX = 'qb-hud-';
const MODAL_PREFIX = 'qb-modal-';
const TABS_PREFIX = 'qb-tabs-';
const SPLIT_PREFIX = 'qb-split-';
const GROUP_PREFIX = 'qb-group-';
const PANE_PREFIX = 'qb-pane-';
const PANE_HEAD_PREFIX = 'qb-pane-head-';
const PANE_BODY_PREFIX = 'qb-pane-body-';
const TABS_BAR_PREFIX = 'qb-tabbar-';
const TAB_PREFIX = 'qb-tab-';
const SPLIT_CHILD_PREFIX = 'qb-split-child-';
const SASH_PREFIX = 'qb-sash-';
const SASH_HIT_PREFIX = 'qb-sash-hit-';
const TAB_PANEL_PREFIX = 'qb-tabpanel-';

// CSS constants
const TAB_BAR_PX = 24;
const SASH_HIT_PX = 8;

/**
 * Create a renderer that reconciles layout to DOM.
 * @param {object} opts
 * @param {HTMLElement} opts.worldEl - world element (from createWorld)
 * @param {object} opts.store - layout store
 * @param {object} opts.instances - instance manager (from createInstanceManager)
 * @param {object} opts.registry - pane registry
 * @param {object} opts.governor - frame governor
 * @param {object} opts.camera - camera state {x, y, z, lock}
 * @param {object} opts.lod - LOD module (optional)
 * @returns {function(layout): void} render function
 */
export function createRenderer({ worldEl, store, instances, registry, governor, camera, lock, onFit, onDetach, onRemove, lod, huds }) {
  // Context menu (M3.03)
  const contextMenu = createContextMenu({ root: worldEl, onAction: (action) => {
    const { action: actStr, clientX, clientY } = action;
    /* An anchor action carries its arguments IN ITS OWN NAME — `anchor-set:<child>:<parent>:
       <edge>:<toEdge>:<dx>:<dy>`, written by js/design/anchor-ui.js — so it is matched by
       prefix rather than split on the first colon. */
    if (actStr.startsWith('anchor-set:')) { handleAnchorSet(store, actStr); return; }
    if (actStr.startsWith('clear-anchor:')) { handleClearAnchor(store, actStr); return; }
    const [act, paneId] = actStr.split(':');
    switch (act) {
      case 'fit':
        if (onFit) onFit(paneId);
        break;
      case 'detach':
        if (onDetach) onDetach(paneId);
        break;
      case 'remove':
        if (onRemove) onRemove(paneId);
        break;
      case 'anchor-to':
        /* the pane id travels in the DETAIL: a listener cannot match `ui:anchor-to:<id>`,
           it can only match the bare name, and the two sides had agreed on the suffix —
           so the anchors submenu never opened. */
        worldEl.dispatchEvent(new CustomEvent('ui:anchor-to', { bubbles: true, detail: { paneId, clientX, clientY } }));
        break;
      case 'clear-anchor':
        worldEl.dispatchEvent(new CustomEvent('ui:clear-anchor', { bubbles: true, detail: { paneId, clientX, clientY } }));
        break;
    }
  } });
  // Element caches keyed by stable IDs
  const rootEls = new Map();    // rootId -> HTMLElement
  const hudEls = new Map();     // hudId -> HTMLElement
  const modalEls = new Map();   // modalId -> HTMLElement
  const paneEls = new Map();    // paneId -> HTMLElement
  const paneHeadEls = new Map(); // paneId -> HTMLElement
  const paneBodyEls = new Map(); // paneId -> HTMLElement

  // Track which pane IDs were rendered in the last frame
  let lastRenderedPaneIds = new Set();

  // Subscribe to store changes
  store.subscribe((layout) => {
    governor.requestFrame('renderer', () => render(layout));
  });

  /**
   * Render a layout into the DOM.
   * @param {object} layout - layout document
   */
  function render(layout) {
    if (!layout) return;

    const currentPaneIds = new Set();

    // Render roots
    if (Array.isArray(layout.roots)) {
      layout.roots.forEach((root) => {
        if (root && root.tree) {
          const rootId = root.id || 'main';
          renderRoot(root, layout, currentPaneIds);
        }
      });
    }

    // Render HUD
    if (Array.isArray(layout.hud)) {
      layout.hud.forEach((hud) => {
        if (hud && hud.tree) {
          const hudId = hud.id || 'hud';
          /* the HUD manager owns the layer AND the element (its docking, its insets); this
             fills the element with the tree, which is the renderer's half */
          const hudEl = document.querySelector('[data-root="' + hudId + '"]')
            || (huds && typeof huds.elementFor === 'function' ? huds.elementFor(hud) : null);
          if (!hudEl) return;
          if (hud.tree) {
            while (hudEl.firstChild) {
              hudEl.removeChild(hudEl.firstChild);
            }
            renderTree(hudEl, hud.tree, [{ hud: hudId }], layout, currentPaneIds);
          }
        }
      });
    }

    // Render modals
    if (Array.isArray(layout.modals) && layout.modals.length > 0) {
      layout.modals.forEach((modal) => {
        if (modal && modal.tree) {
          /* `renderModal` renders the tree into the content it makes (that is why it is handed
             `renderTree`), so this used to render it a SECOND time into the same content: every
             modal's panes were built twice — a one-pane modal held two panes, a three-tab modal
             six tabs — and the second copy's instances were the ones that survived. */
          renderModal(worldEl, modal, layout, currentPaneIds, renderTree);
        }
      });
    }

    // Unmount panes that are no longer in the layout
    lastRenderedPaneIds.forEach((paneId) => {
      if (!currentPaneIds.has(paneId)) {
        instances._unmount(paneId);
      }
    });

    lastRenderedPaneIds = currentPaneIds;
  }

  // ---- Root rendering ----
  function renderRoot(root, layout, currentPaneIds) {
    const rootId = root.id || 'main';
    let rootEl = document.querySelector('[data-root="' + rootId + '"]');
    if (!rootEl) {
      rootEl = document.createElement('div');
      rootEl.className = 'qb-root-' + rootId;
      rootEl.setAttribute('data-root', rootId);
      rootEl.style.cssText =
        'position:absolute; overflow:hidden; touch-action:none; contain:strict;';
      worldEl.appendChild(rootEl);
    }

    // Position and size
    setRect(rootEl, root.x, root.y, root.w, root.h);

    // Clear existing children before re-rendering
    while (rootEl.firstChild) {
      rootEl.removeChild(rootEl.firstChild);
    }

    // Render tree into root element
    if (root.tree) {
      renderTree(rootEl, root.tree, [{ root: root.id || 'main' }], layout, currentPaneIds);
    }
  }

  // ---- Tree rendering ----
  function renderTree(containerEl, tree, path, layout, currentPaneIds) {
    if (!tree || !containerEl) return;

    if (isLeaf(tree)) {
      renderPane(containerEl, tree.pane, path, layout, currentPaneIds);
      return;
    }

    if (isTabs(tree)) {
      renderTabsNode(containerEl, tree, path, layout, currentPaneIds);
      return;
    }

    if (isSplit(tree)) {
      renderSplitNode(containerEl, tree, path, layout, currentPaneIds);
      return;
    }

    if (isGroup(tree)) {
      const groupEl = document.createElement('div');
      groupEl.className = 'qb-group-';
      groupEl.style.cssText = 'position:absolute; inset:0;';
      containerEl.appendChild(groupEl);
      renderTree(groupEl, tree.group, [...path, { group: 0 }], layout, currentPaneIds);
      return;
    }
  }

  // ---- Pane rendering ----
  function renderPane(containerEl, paneId, path, layout, currentPaneIds) {
    currentPaneIds.add(paneId);

    const paneDef = layout.panes && layout.panes[paneId];
    if (!paneDef) return;

    let paneEl = document.createElement('div');
    const result = createPaneElement(paneId, paneDef);
    if (result) {
      const { paneEl: paneEl2, paneHeadEl, paneBodyEl } = result;
      paneEl2.setAttribute('data-pane', paneId);
      containerEl.appendChild(paneEl2);

      // Position and size
      const rect = getRectForPane(path, layout);
      if (rect) {
        updatePaneRect(paneEl2, rect);
      }
      paneEl2.setAttribute('data-path', pathToString(path));

      // Check LOD and culled status
      const lodLevel = computeLOD(paneId, layout);
      const isCulled = lodLevel === 'culled';
      const isInactive = lodLevel === 'frozen' || lodLevel === 'card';

      /* The attribute goes on the element THIS pass is making. The manager (js/engine/lod-apply.js)
         computes the level and writes the attribute when a level CHANGES, but its pass runs
         before this one has made the element it would write to — so a pane whose level never
         changes afterwards (every pane, in a layout that is never zoomed) had no `data-lod` at
         all, and the stylesheets that read it had nothing to read. Same number, both writers:
         the level is the manager's `levelOf` when there is a manager. */
      paneEl2.setAttribute('data-lod', lodLevel);

      // Mount/unmount based on LOD
      if (!isCulled) {
        instances._mount(paneId, paneBodyEl);
        paneBodyEl._mounted = true;
      }

      if (isInactive) {
        paneBodyEl.style.display = 'none';
      } else {
        paneBodyEl.style.display = '';
      }

      // Add pane chrome buttons when unlocked
      addPaneChrome(paneHeadEl, paneId, {
        lock,
        store,
        onFit,
        onDetach,
        onRemove,
      });

      // Right-click context menu on pane head
      paneHeadEl.addEventListener('contextmenu', (e) => {
        e.preventDefault();
        const isRoot = path.length === 1 && path[0] && path[0].root !== undefined;
        const items = [
          { label: 'Fit pane', action: 'fit:' + paneId },
          { label: 'Detach', action: 'detach:' + paneId },
          { label: 'Remove', action: 'remove:' + paneId },
        ];
        if (isRoot) {
          items.splice(1, 0, { label: 'Split right…', action: 'split-right:' + paneId });
          items.splice(2, 0, { label: 'Split below…', action: 'split-below:' + paneId });
          items.push({ label: 'Anchor to…', action: 'anchor-to:' + paneId, rootOnly: true });
          items.push({ label: 'Clear anchor', action: 'clear-anchor:' + paneId, rootOnly: true });
        }
        contextMenu.open(items, e.clientX, e.clientY, isRoot);
      });
    }
  }

  // ---- Tabs rendering: js/layout/renderer-tabs.js ----
  // (the rendering lives there: the bar, the labels, the panels, and the three details its
  //  own header explains — a wrapper that fills its cell, a path that points at THIS node,
  //  and a label that is the pane kind's title)
  function renderTabsNode(containerEl, tree, path, layout, currentPaneIds) {
    renderTabs({ containerEl, tree, path, layout, currentPaneIds, renderTree, store, registry, lock, onRemove });
  }

  // ---- Split rendering: js/layout/renderer-split.js ----
  // (a flex row or column with a sash between each pair, and the sash's hit band on top of the
  //  children's edges — a child that overflows must not cover the seam)
  function renderSplitNode(containerEl, tree, path, layout, currentPaneIds) {
    renderSplit({
      containerEl, tree, path, layout, currentPaneIds, renderTree,
      /* THE SASH IS A CONTROL, and this is the only place that can hand it one: the module that
         draws the sash knows which index it is, and the drag (a ratio in the document, one undo
         step, a saved layout) is the renderer's. `attachSash` was imported here and never
         called, so the sashes the new module draws were decoration. */
      onSash: (sashEl, index, container) => attachSash({
        el: sashEl, store, path, index, container, camera, orient: tree.split === 'h' ? 'h' : 'v',
      }),
    });
  }


  // ---- Helper functions ----
  function getRectForPane(path, layout) {
    if (!layout.roots || !layout.roots.length) return null;

    const rootStep = path[0];
    const rootId = rootStep && rootStep.root !== undefined ? rootStep.root : null;
    const root = rootId ? layout.roots.find((r) => r.id === rootId) : layout.roots[0];
    if (!root) return null;

    const rects = computeRects(root.tree, { x: root.x, y: root.y, w: root.w, h: root.h }, {
      tabBarPx: 24,
      sashHit: 8,
    });

    const pathStr = pathToString(path);
    const node = rects.nodes.find((n) => pathToString(n.path) === pathStr);
    if (node) {
      return node.rect;
    }
    return null;
  }

  /* The LOD manager (js/engine/lod-apply.js) computes and applies the levels — it has the
     geometry, the camera and the hysteresis — and it writes `data-lod` itself. When there is
     no manager the pane is live, which is what this renderer assumed for everything. */
  const lodPort = () => (lod && typeof lod.levelOf === 'function' ? lod : null);

  function computeLOD(paneId, layout) {
    const port = lodPort();
    if (!port) return 'live';
    return port.levelOf(paneId) || 'live';
  }

  function pathToString(path) {
    return path.map(step => {
      if (step.root !== undefined) return 'root:' + step.root;
      if (step.hud !== undefined) return 'hud:' + step.hud;
      if (step.modal !== undefined) return 'modal:' + step.modal;
      if (step.children !== undefined) return 'c:' + step.children;
      if (step.tabs !== undefined) return 't:' + step.tabs;
      if (step.group !== undefined) return 'g:' + step.group;
      return '?';
    }).join('|');
  }

  function setRect(el, x, y, w, h) {
    if (el.style.left === (Math.round(x) + 'px') &&
        el.style.top === (Math.round(y) + 'px') &&
        el.style.width === (Math.round(w) + 'px') &&
        el.style.height === (Math.round(h) + 'px')) {
      return;
    }
    el.style.left = Math.round(x) + 'px';
    el.style.top = Math.round(y) + 'px';
    el.style.width = Math.round(w) + 'px';
    el.style.height = Math.round(h) + 'px';
  }

  /* The context menu is the app's: js/design/anchor-ui.js adds an item and needs the menu it
     adds to, and creating a second one would be a second popup. So it goes out with the
     renderer. */
  render.contextMenu = contextMenu;
  render.renderTree = renderTree;
  return render;
}

const DEFAULT_RENDERER = (opts) => createRenderer(opts);
export default DEFAULT_RENDERER;

/**
 * renderLayout - compatibility wrapper for main.js
 * Creates a renderer with the expected API.
 */
export function renderLayout({ root, store, instances, registry, governor, camera, lod, worldEl }) {
  const renderer = createRenderer({ worldEl: worldEl || root, store, instances, registry, governor, camera, lod });
  return renderer;
}