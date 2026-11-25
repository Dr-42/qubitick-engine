/**
 * Layout document schema — GOAL §4 "Layout document (JSON, n-ary)".
 *
 * A layout is a JSON document describing an n-ary pane tree arranged in tiled
 * clusters (split/tab trees), isolated roots, screen-space HUD roots and
 * full-screen modal roots.
 *
 * @typedef {Object} Layout
 * @property {1} version - LAYOUT_VERSION
 * @property {string} id - preset id, e.g. "standard-desktop"
 * @property {'standard'} mode - layout mode
 * @property {'desktop'} screen - screen class
 * @property {string} wiring - wiring module name (js/defaults/ or storage)
 * @property {Object} camera - { x:number, y:number, z:number, lock:'free'|'fit' }
 * @property {Object<string, {kind:string, params?:Object}>} panes - pane instances
 * @property {LayoutRoot[]} roots - world-space roots
 * @property {Anchor[]} anchors - DAG, at most one anchor parent per root
 * @property {HudRoot[]} hud - screen-space, docked
 * @property {ModalRoot[]} modals - screen-space, full-screen overlay
 *
 * @typedef {Object} LayoutRoot
 * @property {string} id
 * @property {number} x,y,w,h
 * @property {LayoutTree} tree
 *
 * @typedef {Object} HudRoot
 * @property {string} id
 * @property {Dock} dock
 * @property {number} size
 * @property {LayoutTree} tree
 *
 * @typedef {Object} ModalRoot
 * @property {string} id
 * @property {string[]} open - bus events / port actions that open it
 * @property {string[]} close - events that close it
 * @property {LayoutTree} tree
 * @property {boolean} [persistent] - if true Esc/backdrop/back do not close
 *
 * @typedef {Object} Anchor
 * @property {string} root - anchored root id
 * @property {string} to - parent root id
 * @property {Edge} edge - child edge placed at P+(dx,dy)
 * @property {Edge} toEdge - reference point on parent rect
 * @property {number} dx,dy - offset from reference point
 *
 * @typedef {Object} LeafNode - { pane: string } references a pane instance
 * @typedef {Object} TabsNode - { tabs: LayoutTree[], active:number, cycle?:'fixed'|'mru' }
 * @typedef {Object} SplitNode - { split:'h'|'v', sizes:number[], children: LayoutTree[] }
 * @typedef {Object} GroupNode - { group: LayoutTree } transparent wrapper
 *
 * @typedef {LeafNode|TabsNode|SplitNode|GroupNode} LayoutTree
 *
 * @typedef {'left'|'right'|'top'|'bottom'} Edge
 * @typedef {'top'|'bottom'|'left'|'right'} Dock
 *
 * Rules: leaves reference pane instances; `sizes` sum to 1; tab groups cycle
 * "fixed" (default) or "mru"; the same pane instance cannot appear twice;
 * anchors form a DAG (moving a parent moves its children); a `group` hosts a
 * sub-layout so nesting is arbitrary.
 */

/** Current layout document version. */
export const LAYOUT_VERSION = 1;

/** Pane rect edges used by anchors. */
export const EDGES = ['left', 'right', 'top', 'bottom'];

/** Screen-space dock positions for HUD roots. */
export const DOCKS = ['top', 'bottom', 'left', 'right'];

/** Split orientations: 'h' = left→right, 'v' = top→bottom. */
const SPLIT_ORIENTATIONS = new Set(['h', 'v']);

/** Allowed keys per tree-node kind (strict schema validation). */
const KEYS_LEAF = ['pane'];
const KEYS_TABS = ['tabs', 'active', 'cycle'];
const KEYS_SPLIT = ['split', 'sizes', 'children'];
const KEYS_GROUP = ['group'];

function isPlainObject(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function onlyAllowedKeys(node, allowed) {
  return Object.keys(node).every((k) => allowed.includes(k));
}

/**
 * Create an empty layout document for the given preset id.
 * @param {string} [id] - preset id, defaults to "standard-desktop"
 * @returns {Layout}
 */
export function emptyLayout(id) {
  return {
    version: LAYOUT_VERSION,
    id: id || 'standard-desktop',
    mode: 'standard',
    screen: 'desktop',
    wiring: 'standard',
    camera: { x: 0, y: 0, z: 1, lock: 'free' },
    panes: {},
    roots: [],
    anchors: [],
    hud: [],
    modals: [],
  };
}

/**
 * Type guard: true when `n` is a leaf node referencing a pane instance.
 * @param {any} n
 * @returns {n is LeafNode}
 */
export function isLeaf(n) {
  return (
    isPlainObject(n)
    && typeof n.pane === 'string'
    && n.pane.length > 0
    && onlyAllowedKeys(n, KEYS_LEAF)
  );
}

/**
 * Type guard: true when `n` is a tabs node with at least one tab.
 * @param {any} n
 * @returns {n is TabsNode}
 */
export function isTabs(n) {
  return (
    isPlainObject(n)
    && Array.isArray(n.tabs)
    && n.tabs.length > 0
    && typeof n.active === 'number'
    && Number.isFinite(n.active)
    && onlyAllowedKeys(n, KEYS_TABS)
  );
}

/**
 * Type guard: true when `n` is a split node with sizes and children.
 * @param {any} n
 * @returns {n is SplitNode}
 */
export function isSplit(n) {
  return (
    isPlainObject(n)
    && SPLIT_ORIENTATIONS.has(n.split)
    && Array.isArray(n.sizes)
    && n.sizes.length > 0
    && n.sizes.every((s) => typeof s === 'number' && Number.isFinite(s))
    && Array.isArray(n.children)
    && onlyAllowedKeys(n, KEYS_SPLIT)
  );
}

/**
 * Type guard: true when `n` is a group node wrapping a sub-layout tree.
 * @param {any} n
 * @returns {n is GroupNode}
 */
export function isGroup(n) {
  return isPlainObject(n) && isPlainObject(n.group) && onlyAllowedKeys(n, KEYS_GROUP);
}

/**
 * Return the kind of a layout tree node, or null when it is not a node.
 * @param {any} n
 * @returns {'leaf'|'tabs'|'split'|'group'|null}
 */
export function kindOf(n) {
  if (isLeaf(n)) return 'leaf';
  if (isTabs(n)) return 'tabs';
  if (isSplit(n)) return 'split';
  if (isGroup(n)) return 'group';
  return null;
}
