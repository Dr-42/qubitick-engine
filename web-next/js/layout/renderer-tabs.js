/**
 * renderer-tabs.js — a tabs node, rendered (M2.07 Part A; the body is the renderer's own).
 *
 * One bar of buttons and one panel per entry, with only the active entry's panel shown. Three
 * details in here are load-bearing and were each found the hard way (they are the renderer's
 * own comments, kept with the code):
 *
 *   THE WRAPPER FILLS ITS CELL. A split child is a flex container and the wrapper is inside
 *   it: `flex:1 1 0; min-width:0; min-height:0; width:100%; height:100%` — without it the
 *   absolutely-positioned panels resolve against a zero-height wrapper and every pane body
 *   collapses to nothing.
 *
 *   THE PATH POINTS AT THE TABS NODE. `tree.active` is this node's own key, so the dispatch
 *   sends the path of THIS node (`path`) and not `path + {tabs: i}` — the difference is
 *   between switching the tab you clicked and switching a tab one level down.
 *
 *   A TAB IS NAMED BY ITS PANE'S TITLE. The kind's title from the registry when there is one,
 *   because `editor` is a better label than the instance id `ed2`.
 *
 * This module was a stub that built a bar and returned `{tabsEl, bar, panels}` for the caller
 * to fill — while the renderer had the real body inline. It is the real body now.
 */

import { addTabChrome } from '../engine/pane-chrome.js';

const TAB_BAR_PX = 24;

/**
 * Render a tabs node into the container.
 *
 * @param {object} opts
 * @param {HTMLElement} opts.containerEl
 * @param {object} opts.tree - tabs node (`tabs`, `active`)
 * @param {Array} opts.path - path to THIS node
 * @param {object} opts.layout - layout document
 * @param {Set} opts.currentPaneIds - panes rendered in this pass
 * @param {function} opts.renderTree - (containerEl, tree, path, layout, currentPaneIds)
 * @param {object} opts.store - layout store (the tab click dispatches `tree.active`)
 * @param {object} [opts.registry] - pane registry, for a tab's label
 * @param {object} [opts.lock] - the design-mode lock: chrome only while unlocked
 * @param {function} [opts.onRemove] - what a tab's ✕ does
 * @returns {HTMLElement} the tabs element
 */
export function renderTabs({ containerEl, tree, path, layout, currentPaneIds, renderTree, store, registry, lock, onRemove }) {
  const bar = document.createElement('div');
  bar.className = 'qb-tabbar';
  bar.style.cssText =
    'display:flex; height:' + TAB_BAR_PX + 'px; border-bottom:1px solid #2a2f3e;'
    + 'background:#12141e; z-index:2; flex-shrink:0;';

  const active = typeof tree.active === 'number' ? tree.active : 0;
  const tabs = tree.tabs || [];

  tabs.forEach((child, i) => {
    const btn = document.createElement('button');
    btn.className = 'qb-tab';
    btn.style.cssText =
      'padding:0 8px; font-size:11px; color:' + (i === active ? '#c8d2e0' : '#6b7280')
      + '; background:' + (i === active ? '#1c2130' : 'transparent')
      + '; border:none; border-bottom:' + (i === active ? '2px solid #3b82f6' : 'none')
      + '; cursor:pointer; flex-shrink:0; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;';

    let label = 'tab' + i;
    if (child && typeof child.pane === 'string') {
      // Prefer the pane kind's title over the instance id (B4).
      const paneDef = layout.panes && layout.panes[child.pane];
      const kind = paneDef && paneDef.kind;
      const kindDef = kind && registry && (registry.getPane ? registry.getPane(kind) : registry.get ? registry.get(kind) : null);
      label = (kindDef && kindDef.title) || kind || child.pane;
    } else if (child && child.tabs) {
      label = 'tabs';
    } else if (child && child.split) {
      label = 'split';
    } else if (child && child.group) {
      label = 'group';
    }

    btn.textContent = label;
    btn.setAttribute('aria-selected', i === active);
    btn.onclick = () => {
      // `path` already points AT this tabs node; set its active index.
      if (store && store.dispatch) store.dispatch({ type: 'tree.active', path, active: i });
    };
    bar.appendChild(btn);

    // Add tab chrome (remove button) when unlocked
    if (typeof addTabChrome === 'function') {
      addTabChrome(btn, (child && child.pane) || 'tab' + i, { lock, onRemove });
    }
  });

  const tabsEl = document.createElement('div');
  /* tabsEl must fill its parent cell (a split child is `display:flex`, the cell is the flex
     container) and lay its bar + panels out as a column, otherwise the absolutely-positioned
     panels resolve against a 0-height container and every pane body collapses to ~0px. */
  tabsEl.style.cssText =
    'display:flex; flex-direction:column; flex:1 1 0; min-width:0; min-height:0;'
    + ' width:100%; height:100%; position:relative; overflow:hidden;';
  tabsEl.appendChild(bar);

  const panels = document.createElement('div');
  panels.className = 'qb-tabpanels';
  panels.style.cssText = 'position:relative; flex:1; overflow:hidden;';

  tabs.forEach((child, i) => {
    const panel = document.createElement('div');
    panel.className = 'qb-tabpanel-' + i;
    panel.style.cssText =
      'position:absolute; top:' + TAB_BAR_PX + 'px; left:0; right:0; bottom:0;'
      + (i === active ? '; display:flex' : '; display:none');

    const childPath = [...path, { tabs: i }];
    renderTree(panel, child, childPath, layout, new Set());
    panels.appendChild(panel);
  });

  tabsEl.appendChild(panels);
  containerEl.appendChild(tabsEl);
  return tabsEl;
}
