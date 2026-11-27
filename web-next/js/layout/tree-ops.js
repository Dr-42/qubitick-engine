/**
 * tree-ops.js — pure, immutable layout tree operations (GOAL §4).
 *
 * Every function returns a NEW tree; never mutates input.
 *
 * Tree node kinds:
 *   leaf   { pane: string }
 *   tabs   { tabs: LayoutTree[], active: number, cycle?: 'fixed' | 'mru' }
 *   split  { split: 'h' | 'v', sizes: number[], children: LayoutTree[] }
 *   group  { group: LayoutTree }
 *
 * Path conventions (D-032):
 *   []           — the container's root tree
 *   split step   { children: i }
 *   tabs step    { tabs: i }
 *   group step   { group: 0 }
 */

// ---- node checks ----
function isPlainObject(v) {
  return v !== null && typeof v === 'object' && !Array.isArray(v);
}
function isLeaf(n) { return isPlainObject(n) && typeof n.pane === 'string' && n.pane.length > 0; }
function isTabs(n) {
  return isPlainObject(n) && Array.isArray(n.tabs) && n.tabs.length > 0 && typeof n.active === 'number';
}
function isSplit(n) {
  return isPlainObject(n) && (n.split === 'h' || n.split === 'v') && Array.isArray(n.children) && n.children.length > 0;
}
function isGroup(n) { return isPlainObject(n) && isPlainObject(n.group); }

function splitOrientation(side) {
  switch (side) {
    case 'left':
    case 'right':
      return 'h';
    case 'top':
    case 'bottom':
      return 'v';
    default:
      return null;
  }
}

// Walk path (array of steps) to reach the node. Returns the node or null.
function resolvePath(tree, path) {
  let node = tree;
  for (let i = 0; i < path.length; i++) {
    const step = path[i];
    if (!isPlainObject(step)) return null;
    if ('children' in step && isSplit(node)) {
      node = node.children[step.children];
    } else if ('tabs' in step && isTabs(node)) {
      node = node.tabs[step.tabs];
    } else if ('group' in step && isGroup(node)) {
      node = node.group;
    } else {
      return null;
    }
    if (!node) return null;
  }
  return node;
}

// Return { parent, key, index } for the node at path[path.length-1],
// walking path[0..-2] to reach its parent. Returns null if not addressable.
function parentRef(tree, path) {
  if (path.length === 0) return null;
  let node = tree;
  for (let i = 0; i < path.length - 1; i++) {
    const step = path[i];
    if ('children' in step && isSplit(node)) node = node.children[step.children];
    else if ('tabs' in step && isTabs(node)) node = node.tabs[step.tabs];
    else if ('group' in step && isGroup(node)) node = node.group;
    else return null;
  }
  const last = path[path.length - 1];
  if ('children' in last && isSplit(node)) return { parent: node, key: 'children', index: last.children };
  if ('tabs' in last && isTabs(node)) return { parent: node, key: 'tabs', index: last.tabs };
  return null;
}

// Deep clone via JSON.
function deep(v) { return JSON.parse(JSON.stringify(v)); }

// ---- exports ----

export function getAt(tree, path = []) {
  return resolvePath(tree, path);
}

export function setAt(tree, path = [], node) {
  const copy = deep(tree);
  if (path.length === 0) {
    return node === null ? null : deep(node);
  }
  let cur = copy;
  for (let i = 0; i < path.length - 1; i++) {
    const step = path[i];
    if ('children' in step) cur = cur.children[step.children];
    else if ('tabs' in step) cur = cur.tabs[step.tabs];
    else if ('group' in step) cur = cur.group;
  }
  const last = path[path.length - 1];
  if (last.children !== undefined) cur.children[last.children] = deep(node);
  else if (last.tabs !== undefined) cur.tabs[last.tabs] = deep(node);
  return copy;
}

export function paneIds(tree) {
  const out = [];
  function walk(n) {
    if (isLeaf(n)) { out.push(n.pane); return; }
    if (isTabs(n)) { n.tabs.forEach(walk); return; }
    if (isSplit(n)) { n.children.forEach(walk); return; }
    if (isGroup(n)) { walk(n.group); return; }
  }
  walk(tree);
  return out;
}

export function findPane(tree, id) {
  function walk(n, path) {
    if (isLeaf(n)) return n.pane === id ? path : null;
    if (isTabs(n)) {
      for (let i = 0; i < n.tabs.length; i++) {
        const r = walk(n.tabs[i], [...path, { tabs: i }]);
        if (r !== null) return r;
      }
      return null;
    }
    if (isSplit(n)) {
      for (let i = 0; i < n.children.length; i++) {
        const r = walk(n.children[i], [...path, { children: i }]);
        if (r !== null) return r;
      }
      return null;
    }
    if (isGroup(n)) return walk(n.group, [...path, { group: 0 }]);
    return null;
  }
  return walk(tree, []);
}

export function normalizeSizes(sizes) {
  const anyInvalid = sizes.some((s) => typeof s !== 'number' || !Number.isFinite(s) || s <= 0);
  if (anyInvalid || sizes.length === 0) {
    return sizes.map(() => 1 / sizes.length);
  }
  const sum = sizes.reduce((a, b) => a + b, 0);
  return sizes.map((s) => s / sum);
}

// Replace the node located at `path` in `tree` with `replacement` (deep cloned).
function replaceAt(tree, path, replacement) {
  return setAt(tree, path, replacement);
}

export function removePane(tree, id) {
  const path = findPane(tree, id);
  if (!path) return null;
  // Removing the root itself when it is a single leaf.
  if (path.length === 0) return null;
  const copy = deep(tree);
  // Navigate to the parent of the node to remove.
  const rel = parentRef(copy, path);
  if (!rel) return copy;
  const last = path[path.length - 1];
  const idx = last.children !== undefined ? last.children : last.tabs;

  if (rel.key === 'children') {
    // parent is a split
    rel.parent.children.splice(idx, 1);
    rel.parent.sizes.splice(idx, 1);
    if (rel.parent.children.length === 0) {
      // split became empty → remove it from its own parent
      return replaceAt(tree, path.slice(0, -1), null);
    }
    if (rel.parent.children.length === 1) {
      // split with one child becomes that child
      return replaceAt(tree, path.slice(0, -1), rel.parent.children[0]);
    }
    rel.parent.sizes = normalizeSizes(rel.parent.sizes);
    return copy;
  }

  // rel.key === 'tabs'
  rel.parent.tabs.splice(idx, 1);
  if (rel.parent.tabs.length === 0) {
    return replaceAt(tree, path.slice(0, -1), null);
  }
  if (rel.parent.tabs.length === 1) {
    // tabs with 1 entry → that entry
    return replaceAt(tree, path.slice(0, -1), rel.parent.tabs[0]);
  }
  rel.parent.active = Math.max(0, Math.min(rel.parent.active, rel.parent.tabs.length - 1));
  return copy;
}

export function insertBeside(tree, targetPaneId, newPane, side, ratio = 0.5) {
  const targetPath = findPane(tree, targetPaneId);
  if (!targetPath) return tree;
  const orientation = splitOrientation(side);
  if (orientation === null) return tree;
  /* A LAYOUT PLACES A PANE ONCE (`js/layout/schema.js`). `cmdTreeInsert` already refuses a placed
     pane and `cmdTreeMove` removes one before placing it again, so this is the second lock on the
     same door — and it keeps the document valid for a caller that edits a tree directly.
     THE ROOT IS A TARGET LIKE ANY OTHER: refusing `targetPath.length === 0` made `insert beside` on
     a root holding a single pane return the tree unchanged, so the command reported success while
     placing nothing. Wrapping the root in the new split is what beside means. */
  if (paneIds(tree).includes(newPane)) return tree;
  const newPaneNode = { pane: newPane };
  const parentPath = targetPath.slice(0, -1);
  const targetStep = targetPath[targetPath.length - 1];
  const parent = resolvePath(tree, parentPath);

  // Case 1: parent is a split of the same orientation → splice beside target.
  if (isSplit(parent) && parent.split === orientation && targetStep.children !== undefined) {
    const copy = deep(tree);
    // navigate to parent in copy
    let cur = copy;
    for (let i = 0; i < parentPath.length - 1; i++) {
      const step = parentPath[i];
      if ('children' in step) cur = cur.children[step.children];
      else if ('tabs' in step) cur = cur.tabs[step.tabs];
      else if ('group' in step) cur = cur.group;
    }
    const idx = targetStep.children;
    const targetSize = cur.sizes[idx];
    const take = targetSize * ratio;
    const newSizes = cur.sizes.slice();
    newSizes[idx] = targetSize - take;
    newSizes.splice(idx + 1, 0, take);
    const newChildren = cur.children.slice();
    const insertIdx = (side === 'left' || side === 'top') ? idx : idx + 1;
    newChildren.splice(insertIdx, 0, deep(newPaneNode));
    cur.sizes = normalizeSizes(newSizes);
    cur.children = newChildren;
    return copy;
  }

  // Case 2: wrap target in a new split of `orientation`.
  const target = resolvePath(tree, targetPath);
  const before = side === 'left' || side === 'top';
  const newSplit = {
    split: orientation,
    sizes: [1 - ratio, ratio],
    children: [before ? newPaneNode : deep(target), before ? deep(target) : newPaneNode],
  };
  return replaceAt(tree, targetPath, newSplit);
}

export function insertTab(tree, targetPaneId, newPane) {
  const targetPath = findPane(tree, targetPaneId);
  if (!targetPath) return tree;
  if (paneIds(tree).includes(newPane)) return tree; // one placement per pane; see insertBeside
  const copy = deep(tree);
  const parentPath = targetPath.slice(0, -1);
  const targetStep = targetPath[targetPath.length - 1];
  const newTabNode = { pane: newPane };

  // Navigate to parent in copy.
  let cur = copy;
  for (let i = 0; i < parentPath.length - 1; i++) {
    const step = parentPath[i];
    if ('children' in step) cur = cur.children[step.children];
    else if ('tabs' in step) cur = cur.tabs[step.tabs];
    else if ('group' in step) cur = cur.group;
  }
  const parent = resolvePath(copy, parentPath);
  const target = resolvePath(copy, targetPath);

  if (isTabs(parent) && targetStep.tabs !== undefined) {
    // target already in tabs → push after target and activate
    const idx = targetStep.tabs;
    parent.tabs.splice(idx + 1, 0, deep(newTabNode));
    parent.active = idx + 1;
    return copy;
  }

  if (isLeaf(target)) {
    // target leaf → new tabs [target, new] active 1
    const tabsNode = { tabs: [deep(target), deep(newTabNode)], active: 1 };
    return replaceAt(tree, targetPath, tabsNode);
  }

  return copy;
}

export function isDescendant(tree, ancestorPath, path) {
  if (path.length <= ancestorPath.length) return false;
  for (let i = 0; i < ancestorPath.length; i++) {
    if (JSON.stringify(path[i]) !== JSON.stringify(ancestorPath[i])) return false;
  }
  // ancestor must actually exist at ancestorPath
  return resolvePath(tree, ancestorPath) !== null;
}

export function clone(tree) {
  return deep(tree);
}
