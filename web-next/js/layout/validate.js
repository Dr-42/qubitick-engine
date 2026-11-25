/**
 * validate.js — pure layout validation (M1.20).
 *
 * validate(layout) → { ok, errors: [{ path, message }] }
 * Collects ALL errors (never stops at the first).
 */

import { EDGES, DOCKS, isLeaf, isTabs, isSplit, isGroup } from './schema.js';
import { detectCycle } from './anchors.js';
import { CONFIG } from '../core/config.js';

/* `mode` is the WORKING MODE of a workspace, not the app that draws it: three wiring presets
   (minimal/standard/power) and now the canvas arrangement, which js/app/tiles-to-layout.js writes
   when it converts the canvas's tiles and clusters into a layout document. A canvas-shaped
   document is a document this schema accepts — roots with trees, one per tile or cluster — which
   is what makes "the canvas is a layout mode of the engine" a statement about the model rather
   than about intent. */
const MODE_SET = new Set(['minimal', 'standard', 'power', 'canvas']);
const SCREEN_SET = new Set(['desktop', 'tablet', 'phone']);
const CYCLE_SET = new Set(['fixed', 'mru']);
const LOCK_SET = new Set(['free', 'fit']);
const SIZE_TOL = 1e-6;

function err(errors, path, message) {
  errors.push({ path, message });
}

function isFiniteNum(v) {
  return typeof v === 'number' && Number.isFinite(v);
}

function isPlainObject(v) {
  return v !== null && typeof v === 'object' && !Array.isArray(v);
}

function hasTabsKey(n) {
  return isPlainObject(n) && 'tabs' in n;
}

function validateCamera(cam, path, errors) {
  if (!isPlainObject(cam)) {
    err(errors, path, 'camera must be an object');
    return;
  }
  if (!LOCK_SET.has(cam.lock)) {
    err(errors, [...path, 'lock'], `camera.lock must be 'free' or 'fit', got ${JSON.stringify(cam.lock)}`);
  }
  if (!isFiniteNum(cam.x)) err(errors, [...path, 'x'], 'camera.x must be a finite number');
  if (!isFiniteNum(cam.y)) err(errors, [...path, 'y'], 'camera.y must be a finite number');
  if (!isFiniteNum(cam.z) || cam.z < CONFIG.zoom.min || cam.z > CONFIG.zoom.max) {
    err(errors, [...path, 'z'], `camera.z must be in [${CONFIG.zoom.min}, ${CONFIG.zoom.max}]`);
  }
}

function validatePaneDef(id, p, errors) {
  if (!isPlainObject(p)) {
    err(errors, ['panes', id], 'pane definition must be an object');
    return;
  }
  if (typeof p.kind !== 'string' || p.kind.length === 0) {
    err(errors, ['panes', id, 'kind'], 'pane kind must be a non-empty string');
  }
  if (p.params !== undefined && !isPlainObject(p.params)) {
    err(errors, ['panes', id, 'params'], 'pane params must be an object');
  }
}

function validateTree(node, path, errors, paneRefs) {
  if (isLeaf(node)) {
    if (typeof node.pane !== 'string' || node.pane.length === 0) {
      err(errors, [...path, 'pane'], 'leaf pane must be a non-empty string');
    } else {
      paneRefs.push({ paneId: node.pane, path: [...path] });
    }
    return;
  }
  if (hasTabsKey(node)) {
    // node attempts to be tabs
    if (!Array.isArray(node.tabs) || node.tabs.length === 0) {
      err(errors, [...path, 'tabs'], 'tabs must be a non-empty array');
      return;
    }
    node.tabs.forEach((child, i) => validateTree(child, [...path, { tabs: i }], errors, paneRefs));
    if (!isFiniteNum(node.active)) {
      err(errors, [...path, 'active'], 'tabs.active must be a finite integer');
    } else {
      const n = node.tabs.length;
      if (node.active < 0 || node.active >= n) {
        err(errors, [...path, 'active'], `tabs.active ${node.active} out of range [0, ${n - 1}]`);
      }
    }
    if (node.cycle !== undefined && !CYCLE_SET.has(node.cycle)) {
      err(errors, [...path, 'cycle'], `tabs.cycle must be 'fixed' or 'mru', got ${JSON.stringify(node.cycle)}`);
    }
    return;
  }
  if (isSplit(node)) {
    if (node.split !== 'h' && node.split !== 'v') {
      err(errors, [...path, 'split'], `split must be 'h' or 'v', got ${JSON.stringify(node.split)}`);
    }
    if (!Array.isArray(node.children) || node.children.length < 2) {
      err(errors, [...path, 'children'], 'split must have at least 2 children');
    } else {
      node.children.forEach((child, i) => validateTree(child, [...path, { children: i }], errors, paneRefs));
    }
    const childrenLen = Array.isArray(node.children) ? node.children.length : 0;
    if (!Array.isArray(node.sizes) || node.sizes.length !== childrenLen) {
      err(errors, [...path, 'sizes'], 'split sizes.length must equal children.length');
    } else {
      let okSizes = true;
      let sum = 0;
      for (const s of node.sizes) {
        if (!isFiniteNum(s) || s <= 0) okSizes = false;
        sum += s;
      }
      if (!okSizes) {
        err(errors, [...path, 'sizes'], 'every split size must be a finite number > 0');
      } else if (Math.abs(sum - 1) > SIZE_TOL) {
        err(errors, [...path, 'sizes'], `split sizes must sum to 1 (got ${sum})`);
      }
    }
    return;
  }
  if (isGroup(node)) {
    validateTree(node.group, [...path, { group: 0 }], errors, paneRefs);
    return;
  }
  err(errors, path, 'invalid layout tree node');
}

function validateRoot(root, path, errors, paneRefs) {
  if (!isPlainObject(root)) {
    err(errors, path, 'root must be an object');
    return;
  }
  if (typeof root.id !== 'string' || root.id.length === 0) {
    err(errors, [...path, 'id'], 'root id must be a non-empty string');
  }
  if (!isFiniteNum(root.x)) err(errors, [...path, 'x'], 'root.x must be a finite number');
  if (!isFiniteNum(root.y)) err(errors, [...path, 'y'], 'root.y must be a finite number');
  if (!isFiniteNum(root.w) || root.w <= 0) err(errors, [...path, 'w'], 'root.w must be a finite number > 0');
  if (!isFiniteNum(root.h) || root.h <= 0) err(errors, [...path, 'h'], 'root.h must be a finite number > 0');
  if (root.tree) validateTree(root.tree, [...path, 'tree'], errors, paneRefs);
}

function validateHud(h, path, errors, paneRefs) {
  if (!isPlainObject(h)) {
    err(errors, path, 'hud root must be an object');
    return;
  }
  if (typeof h.id !== 'string' || h.id.length === 0) {
    err(errors, [...path, 'id'], 'hud id must be a non-empty string');
  }
  if (!DOCKS.includes(h.dock)) {
    err(errors, [...path, 'dock'], `hud.dock must be one of ${JSON.stringify(DOCKS)}`);
  }
  if (!isFiniteNum(h.size) || h.size <= 0) err(errors, [...path, 'size'], 'hud.size must be a finite number > 0');
  if (h.tree) validateTree(h.tree, [...path, 'tree'], errors, paneRefs);
}

function validateModal(m, path, errors, paneRefs) {
  if (!isPlainObject(m)) {
    err(errors, path, 'modal must be an object');
    return;
  }
  if (typeof m.id !== 'string' || m.id.length === 0) {
    err(errors, [...path, 'id'], 'modal id must be a non-empty string');
  }
  if (!Array.isArray(m.open) || !m.open.every(v => typeof v === 'string')) {
    err(errors, [...path, 'open'], 'modal.open must be an array of strings');
  }
  if (!Array.isArray(m.close) || !m.close.every(v => typeof v === 'string')) {
    err(errors, [...path, 'close'], 'modal.close must be an array of strings');
  }
  if (m.persistent !== undefined && typeof m.persistent !== 'boolean') {
    err(errors, [...path, 'persistent'], 'modal.persistent must be a boolean');
  }
  if (m.tree) validateTree(m.tree, [...path, 'tree'], errors, paneRefs);
}

function validateAnchors(anchors, rootIds, errors) {
  if (!Array.isArray(anchors)) {
    err(errors, ['anchors'], 'anchors must be an array');
    return;
  }
  const anchorPerRoot = new Map();
  for (const a of anchors) {
    if (!isPlainObject(a)) {
      err(errors, ['anchors'], 'anchor must be an object');
      continue;
    }
    if (!rootIds.has(a.root)) {
      err(errors, ['anchors', 'root'], `anchor.root '${a.root}' does not exist among roots`);
    }
    if (!rootIds.has(a.to)) {
      err(errors, ['anchors', 'to'], `anchor.to '${a.to}' does not exist among roots`);
    }
    if (a.root === a.to) {
      err(errors, ['anchors'], 'anchor.root and anchor.to must differ');
    }
    if (!EDGES.includes(a.edge)) {
      err(errors, ['anchors', 'edge'], `anchor.edge must be one of ${JSON.stringify(EDGES)}`);
    }
    if (!EDGES.includes(a.toEdge)) {
      err(errors, ['anchors', 'toEdge'], `anchor.toEdge must be one of ${JSON.stringify(EDGES)}`);
    }
    if (!isFiniteNum(a.dx)) err(errors, ['anchors', 'dx'], 'anchor.dx must be a finite number');
    if (!isFiniteNum(a.dy)) err(errors, ['anchors', 'dy'], 'anchor.dy must be a finite number');
    if (anchorPerRoot.has(a.root)) {
      err(errors, ['anchors', 'root'], `root '${a.root}' has more than one anchor`);
    } else {
      anchorPerRoot.set(a.root, a);
    }
  }
  const cycle = detectCycle(anchors.filter(a => isPlainObject(a)));
  if (cycle) {
    err(errors, ['anchors'], `anchor cycle detected: ${cycle.join(' → ')}`);
  }
}

export function validate(layout) {
  const errors = [];

  if (!isPlainObject(layout)) {
    return { ok: false, errors: [{ path: [], message: 'layout must be an object' }] };
  }

  if (layout.version !== 1) {
    err(errors, ['version'], `layout.version must be 1, got ${JSON.stringify(layout.version)}`);
  }
  if (!MODE_SET.has(layout.mode)) {
    err(errors, ['mode'], `layout.mode must be one of ${JSON.stringify([...MODE_SET])}`);
  }
  if (!SCREEN_SET.has(layout.screen)) {
    err(errors, ['screen'], `layout.screen must be one of ${JSON.stringify([...SCREEN_SET])}`);
  }
  if (layout.wiring !== undefined && typeof layout.wiring !== 'string') {
    err(errors, ['wiring'], 'layout.wiring must be a string');
  }
  validateCamera(layout.camera, ['camera'], errors);

  const panes = layout.panes;
  if (!isPlainObject(panes)) {
    err(errors, ['panes'], 'layout.panes must be an object');
  } else {
    for (const [id, p] of Object.entries(panes)) {
      validatePaneDef(id, p, errors);
    }
  }

  const rootIds = new Set();
  const paneRefs = [];

  const roots = layout.roots;
  if (Array.isArray(roots)) {
    const seen = new Set();
    for (let i = 0; i < roots.length; i++) {
      const r = roots[i];
      if (typeof r.id === 'string') {
        if (seen.has(r.id)) err(errors, ['roots', i, 'id'], `duplicate root id '${r.id}'`);
        seen.add(r.id);
        rootIds.add(r.id);
      }
      validateRoot(r, ['roots', i], errors, paneRefs);
    }
  } else {
    err(errors, ['roots'], 'layout.roots must be an array');
  }

  validateAnchors(layout.anchors || [], rootIds, errors);

  const hud = layout.hud;
  if (Array.isArray(hud)) {
    const seen = new Set();
    for (let i = 0; i < hud.length; i++) {
      const h = hud[i];
      if (seen.has(h.id)) err(errors, ['hud'], `duplicate hud id '${h.id}'`);
      seen.add(h.id);
      validateHud(h, ['hud', i], errors, paneRefs);
    }
  } else {
    err(errors, ['hud'], 'layout.hud must be an array');
  }

  const modals = layout.modals;
  if (Array.isArray(modals)) {
    const seen = new Set();
    for (let i = 0; i < modals.length; i++) {
      const m = modals[i];
      if (seen.has(m.id)) err(errors, ['modals'], `duplicate modal id '${m.id}'`);
      seen.add(m.id);
      validateModal(m, ['modals', i], errors, paneRefs);
    }
  } else {
    err(errors, ['modals'], 'layout.modals must be an array');
  }

  const paneExists = panes && isPlainObject(panes);
  for (const ref of paneRefs) {
    if (!paneExists || !(ref.paneId in panes)) {
      err(errors, ref.path, `pane '${ref.paneId}' is not defined in layout.panes`);
    }
  }

  const refCount = {};
  for (const ref of paneRefs) {
    refCount[ref.paneId] = (refCount[ref.paneId] || 0) + 1;
  }
  for (const [paneId, count] of Object.entries(refCount)) {
    if (count > 1) {
      err(errors, [], `pane '${paneId}' is referenced ${count} times (each pane may appear at most once)`);
    }
  }

  return { ok: errors.length === 0, errors };
}
