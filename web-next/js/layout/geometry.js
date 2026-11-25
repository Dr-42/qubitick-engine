/**
 * geometry.js — pure layout geometry computation (GOAL §4).
 *
 * computeRects(tree, rect, opts) walks a layout tree and produces, for every
 * node, the screen-space rect it occupies, plus sash hit rects for splits.
 * Pure: never mutates input; returns floats (no rounding).
 *
 * Node rect conventions:
 *   split 'h'  — children share width by `sizes`, laid left→right
 *   split 'v'  — children share height by `sizes`, laid top→bottom
 *   tabs       — top `tabBarPx` is the bar; only the active child gets
 *                a content rect below it; inactive entries are listed
 *                with `inactive: true` and are not descended into
 *   group      — child gets the full rect
 *   leaf       — rect is the rect passed down; paneId names the pane
 *
 * Sashes appear between adjacent children of a split:
 *   orient 'col' — vertical sash for an 'h' split
 *   orient 'row' — horizontal sash for a 'v' split
 *   rect is the hit area (`sashHitPx` wide), centred on the child boundary
 */

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
function kindOf(n) {
  if (isLeaf(n)) return 'leaf';
  if (isTabs(n)) return 'tabs';
  if (isSplit(n)) return 'split';
  if (isGroup(n)) return 'group';
  return 'leaf';
}

const DEFAULT_TAB_BAR = 0;
const DEFAULT_SASH_HIT = 8;

// ---- export ----

export function computeRects(tree, rect, opts = {}) {
  const tabBarPx = Number.isFinite(opts.tabBarPx) ? opts.tabBarPx : DEFAULT_TAB_BAR;
  const sashHit = Number.isFinite(opts.sashHit) ? opts.sashHit : DEFAULT_SASH_HIT;

  const nodes = [];
  const sashes = [];

  function pushEntry(path, kind, r, extra) {
    nodes.push({
      path: [...path],
      kind,
      rect: { x: r.x, y: r.y, w: r.w, h: r.h },
      ...(extra || {}),
    });
  }

  function walk(node, path, r) {
    if (isLeaf(node)) {
      pushEntry(path, 'leaf', r, { paneId: node.pane });
      return;
    }
    if (isTabs(node)) {
      const tabsRect = { x: r.x, y: r.y, w: r.w, h: tabBarPx };
      pushEntry(path, 'tabs', r, { tabsRect });
      const contentRect = { x: r.x, y: r.y + tabBarPx, w: r.w, h: Math.max(0, r.h - tabBarPx) };
      const active = Math.max(0, Math.min(Number.isFinite(node.active) ? node.active : 0, node.tabs.length - 1));
      node.tabs.forEach((child, i) => {
        const cpath = [...path, { tabs: i }];
        if (i === active) {
          walk(child, cpath, contentRect);
        } else {
          const extra = { inactive: true };
          if (isLeaf(child)) extra.paneId = child.pane;
          pushEntry(cpath, kindOf(child), contentRect, extra);
        }
      });
      return;
    }
    if (isSplit(node)) {
      pushEntry(path, 'split', r);
      if (node.split === 'h') {
        let cx = r.x;
        node.sizes.forEach((size, i) => {
          const cw = r.w * size;
          const childRect = { x: cx, y: r.y, w: cw, h: r.h };
          walk(node.children[i], [...path, { children: i }], childRect);
          if (i < node.children.length - 1) {
            const boundary = cx + cw;
            sashes.push({
              path: [...path],
              index: i,
              orient: 'col',
              rect: { x: boundary - sashHit / 2, y: r.y, w: sashHit, h: r.h },
            });
          }
          cx += cw;
        });
      } else {
        let cy = r.y;
        node.sizes.forEach((size, i) => {
          const ch = r.h * size;
          const childRect = { x: r.x, y: cy, w: r.w, h: ch };
          walk(node.children[i], [...path, { children: i }], childRect);
          if (i < node.children.length - 1) {
            const boundary = cy + ch;
            sashes.push({
              path: [...path],
              index: i,
              orient: 'row',
              rect: { x: r.x, y: boundary - sashHit / 2, w: r.w, h: sashHit },
            });
          }
          cy += ch;
        });
      }
      return;
    }
    if (isGroup(node)) {
      pushEntry(path, 'group', r);
      walk(node.group, [...path, { group: 0 }], r);
      return;
    }
  }

  walk(tree, [], rect);
  return { nodes, sashes };
}
