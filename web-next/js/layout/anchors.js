/**
 * anchors.js — pure anchor resolution (GOAL §4, D-018).
 *
 * Anchors position one root relative to another root's resolved rect.
 * Reference point P on the parent rect by `toEdge`:
 *   left  → (px, py)      top    → (px, py)
 *   right → (px+pw, py)   bottom → (px, py+ph)
 * Child edge `edge` is placed at P + (dx, dy):
 *   left/top:  child (x,y) = P + (dx, dy)
 *   right:     x = P.x + dx − w,  y = P.y + dy
 *   bottom:    x = P.x + dx,      y = P.y + dy − h
 *
 * Resolution walks the anchor graph in topological order so chains work.
 * Roots without an anchor keep their own stored x,y.
 */

const EDGE_SET = new Set(['left', 'right', 'top', 'bottom']);

function isFiniteRect(r) {
  return r && typeof r.x === 'number' && Number.isFinite(r.x)
    && typeof r.y === 'number' && Number.isFinite(r.y)
    && typeof r.w === 'number' && Number.isFinite(r.w)
    && typeof r.h === 'number' && Number.isFinite(r.h);
}

// Reference point P on parent rect by toEdge.
export function referencePoint(parent, toEdge) {
  switch (toEdge) {
    case 'left':
    case 'top':
      return { x: parent.x, y: parent.y };
    case 'right':
      return { x: parent.x + parent.w, y: parent.y };
    case 'bottom':
      return { x: parent.x, y: parent.y + parent.h };
    default:
      return { x: parent.x, y: parent.y };
  }
}

// Child origin from reference point P, child edge, offset, and child size.
function childOrigin(P, edge, dx, dy, cw, ch) {
  switch (edge) {
    case 'left':
    case 'top':
      return { x: P.x + dx, y: P.y + dy };
    case 'right':
      return { x: P.x + dx - cw, y: P.y + dy };
    case 'bottom':
      return { x: P.x + dx, y: P.y + dy - ch };
    default:
      return { x: P.x + dx, y: P.y + dy };
  }
}

// ---- detectCycle ----
// anchors: [{root, to, edge, toEdge, dx, dy}, ...]
// Returns null if acyclic, or [rootId, ...] cycle path if cyclic.
export function detectCycle(anchors) {
  const parentOf = new Map(); // rootId -> to
  for (const a of anchors) {
    parentOf.set(a.root, a.to);
  }
  const WHITE = 0, GRAY = 1, BLACK = 2;
  const color = new Map();

  function dfs(node, stack) {
    color.set(node, GRAY);
    stack.push(node);
    const parent = parentOf.get(node);
    if (parent !== undefined) {
      const c = color.get(parent) || WHITE;
      if (c === GRAY) {
        // cycle: find where parent appears in stack
        const idx = stack.indexOf(parent);
        return stack.slice(idx);
      }
      if (c === WHITE) {
        const found = dfs(parent, stack);
        if (found) return found;
      }
    }
    stack.pop();
    color.set(node, BLACK);
    return null;
  }

  for (const rootId of parentOf.keys()) {
    if ((color.get(rootId) || WHITE) === WHITE) {
      const found = dfs(rootId, []);
      if (found) return found;
    }
  }
  return null;
}

// rootAnchorParent(anchors, rootId) → the parent root id, or undefined.
export function rootAnchorParent(anchors, rootId) {
  for (const a of anchors) {
    if (a.root === rootId) return a.to;
  }
  return undefined;
}

// resolveAnchors(roots, anchors) → Map<rootId, {x,y}>
// Roots without an anchor keep their stored x,y.
// Processes in topological order; chains use the parent's resolved position.
export function resolveAnchors(roots, anchors) {
  const cycle = detectCycle(anchors);
  if (cycle) {
    throw new Error(`Anchor cycle detected: ${cycle.join(' → ')}`);
  }

  // Initial resolved positions from roots.
  const resolved = new Map();
  for (const root of roots) {
    if (isFiniteRect(root)) {
      resolved.set(root.id, { x: root.x, y: root.y, w: root.w, h: root.h });
    } else {
      resolved.set(root.id, { x: root.x || 0, y: root.y || 0, w: root.w || 0, h: root.h || 0 });
    }
  }

  // Build anchor lookup: rootId -> anchor
  const anchorOf = new Map();
  for (const a of anchors) {
    anchorOf.set(a.root, a);
  }

  const result = new Map();
  const resolvedSet = new Set();

  // Seed: roots without anchors are immediately resolved.
  for (const root of roots) {
    if (!anchorOf.has(root.id)) {
      const r = resolved.get(root.id);
      result.set(root.id, { x: r.x, y: r.y });
      resolvedSet.add(root.id);
    }
  }

  // Iteratively resolve roots whose parent is resolved.
  let changed = true;
  while (changed) {
    changed = false;
    for (const root of roots) {
      if (resolvedSet.has(root.id)) continue;
      const a = anchorOf.get(root.id);
      if (!a) continue;
      const parentResolved = resolved.get(a.to);
      if (!parentResolved) continue; // parent not yet resolved
      const childRect = resolved.get(root.id);
      if (!childRect) continue;
      const P = referencePoint(parentResolved, a.toEdge);
      const origin = childOrigin(P, a.edge, a.dx, a.dy, childRect.w, childRect.h);
      result.set(root.id, { x: origin.x, y: origin.y });
      // Keep w,h from the child's stored rect; only x,y are derived.
      resolved.set(root.id, { ...childRect, x: origin.x, y: origin.y });
      resolvedSet.add(root.id);
      changed = true;
    }
  }

  // Any root not resolved (shouldn't happen without a cycle) keeps stored x,y.
  for (const root of roots) {
    if (!result.has(root.id)) {
      const r = resolved.get(root.id);
      result.set(root.id, { x: r.x, y: r.y });
    }
  }

  return result;
}
