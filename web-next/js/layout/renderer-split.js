/**
 * renderer-split.js — a split node, rendered (M2.07 Part A; the body is the renderer's own).
 *
 * A split is a flex row or column whose children share the extent by `sizes`, with a SASH
 * between each pair — and the sash is the thing the pointer grabs, so it carries a hit area
 * wider than the line it draws (a 4px rule with a 48px band down its middle), and it is drawn
 * on top of the two children's edges rather than in the flow between them: a child that
 * overflows must not be able to cover the seam.
 *
 * This module was a stub that created the split element and then said "the main renderer will
 * handle it": it never rendered a child, never added a sash, and returned an element the
 * caller was expected to finish. The renderer had its own copy of the real thing inline, so
 * this file looked like a rendering path and was not one. The real body lives here now.
 */

/**
 * Render a split node into the container.
 *
 * @param {object} opts
 * @param {HTMLElement} opts.containerEl
 * @param {object} opts.tree - split tree node (`split: 'h'|'v'`, `children`, `sizes`)
 * @param {Array} opts.path - path to this node
 * @param {object} opts.layout - layout document
 * @param {Set} opts.currentPaneIds - panes rendered in this pass (collected by renderTree)
 * @param {function} opts.renderTree - (containerEl, tree, path, layout, currentPaneIds)
 * @param {function} [opts.onSash] - (sashEl, index) → the drag, or a cleanup. The sash is a
 *   control, and what a drag MEANS (a ratio in the document, an undo step, a saved layout) is
 *   the renderer's business, not this module's — so it hands the element over.
 * @returns {HTMLElement} the split element
 */
export function renderSplit({ containerEl, tree, path, layout, currentPaneIds, renderTree, onSash }) {
  const isH = tree.split === 'h';
  const splitEl = document.createElement('div');
  splitEl.className = 'qb-split';
  splitEl.style.cssText =
    'position:relative; width:100%; height:100%; display:flex; flex-direction:'
    + (isH ? 'row' : 'column') + ';';

  const sizes = Array.isArray(tree.sizes) ? tree.sizes :
    new Array(tree.children.length).fill(1 / tree.children.length);

  tree.children.forEach((child, i) => {
    const childEl = document.createElement('div');
    childEl.className = 'qb-split-child-' + i;
    childEl.style.cssText =
      'position:relative; '
      + 'flex:0 0 auto; '
      + (isH ? 'width:' + (Math.round(sizes[i] * 10000) / 100) + '%; height:100%;'
             : 'width:100%; height:' + (Math.round(sizes[i] * 10000) / 100) + '%;')
      + ' min-width:0; min-height:0; display:flex; overflow:hidden;';

    const childPath = [...path, { children: i }];
    renderTree(childEl, child, childPath, layout, currentPaneIds);
    splitEl.appendChild(childEl);

    // Sash between children
    if (i < tree.children.length - 1) {
      const sash = document.createElement('div');
      sash.className = 'qb-sash';
      sash.style.cssText =
        'width:' + (isH ? '4px' : '100%') + ';'
        + ' height:' + (isH ? '100%' : '4px') + ';'
        + ' background:transparent; cursor:' + (isH ? 'ew-resize' : 'ns-resize')
        + '; z-index:5; position:absolute;'
        + (isH ? 'right:' + (-2) + 'px; top:0; bottom:0;'
           : 'left:0; right:0; bottom:' + (-2) + 'px;');

      const sashHit = document.createElement('div');
      sashHit.className = 'qb-sash-hit';
      sashHit.style.cssText =
        'position:absolute; top:' + (isH ? '0' : '-22px') + '; '
        + 'left:' + (isH ? '-22px' : '0') + '; '
        + 'width:' + (isH ? '48px' : '100%') + '; '
        + 'height:' + (isH ? '100%' : '48px') + '; '
        + 'background:transparent;';
      sash.appendChild(sashHit);
      splitEl.appendChild(sash);
      // the sash is the control: the drag that moves the boundary goes on here
      if (typeof onSash === 'function') onSash(sash, i, splitEl);
    }
  });

  containerEl.appendChild(splitEl);
  return splitEl;
}
