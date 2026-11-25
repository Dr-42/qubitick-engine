// presets.js - preset loader
const PHONE_MAX_DIM = 600;
const TABLET_MAX_DIM_COARSE = 1000;

export const PRESET_IDS = [
  'minimal-desktop', 'minimal-tablet', 'minimal-phone',
  'standard-desktop', 'standard-tablet', 'standard-phone',
  'power-desktop', 'power-tablet', 'power-phone',
];

export function presetId(mode, screen) {
  return mode + '-' + screen;
}

export function presetUrl(id) {
  return '/js/defaults/presets/' + id + '.json';
}

export function screenClass(dims) {
  const minDim = Math.min(dims.width, dims.height);
  if (minDim < PHONE_MAX_DIM) return 'phone';
  if (dims.coarse && minDim < TABLET_MAX_DIM_COARSE) return 'tablet';
  return 'desktop';
}

export function suggestMode() {
  return 'standard';
}

function err(errors, path, message) {
  errors.push({ path: path, message: message });
}

export function validatePreset(layout, paneKinds, id) {
  var errors = [];
  if (!layout || typeof layout !== 'object') {
    err(errors, [], 'layout must be an object');
    return { ok: false, errors: errors };
  }
  if (layout.version !== 1) {
    err(errors, ['version'], 'layout.version must be 1');
  }
  if (typeof layout.id !== 'string' || layout.id.length === 0) {
    err(errors, ['id'], 'layout.id must be a non-empty string');
  }
  var modeSet = new Set(['minimal', 'standard', 'power']);
  if (!modeSet.has(layout.mode)) {
    err(errors, ['mode'], "layout.mode must be one of [minimal, standard, power]");
  }
  var screenSet = new Set(['desktop', 'tablet', 'phone']);
  if (!screenSet.has(layout.screen)) {
    err(errors, ['screen'], "layout.screen must be one of [desktop, tablet, phone]");
  }
  var wiringSet = new Set(['minimal', 'standard', 'power']);
  if (typeof layout.wiring !== 'string' || !wiringSet.has(layout.wiring)) {
    err(errors, ['wiring'], "layout.wiring must be one of [minimal, standard, power]");
  } else if (layout.mode && layout.wiring !== layout.mode) {
    err(errors, ['wiring'], "layout.wiring must match mode");
  }
  if (!layout.camera || typeof layout.camera !== 'object') {
    err(errors, ['camera'], 'camera must be an object');
  } else {
    if (layout.camera.lock !== 'free' && layout.camera.lock !== 'fit') {
      err(errors, ['camera', 'lock'], "camera.lock must be 'free' or 'fit'");
    }
    if (typeof layout.camera.x !== 'number' || !Number.isFinite(layout.camera.x)) {
      err(errors, ['camera', 'x'], 'camera.x must be a finite number');
    }
    if (typeof layout.camera.y !== 'number' || !Number.isFinite(layout.camera.y)) {
      err(errors, ['camera', 'y'], 'camera.y must be a finite number');
    }
    if (typeof layout.camera.z !== 'number' || !Number.isFinite(layout.camera.z)) {
      err(errors, ['camera', 'z'], 'camera.z must be a finite number');
    }
  }
  if (!layout.panes || typeof layout.panes !== 'object' || Array.isArray(layout.panes)) {
    err(errors, ['panes'], 'panes must be an object');
  } else {
    for (var paneId of Object.keys(layout.panes)) {
      var p = layout.panes[paneId];
      if (!p || typeof p !== 'object') {
        err(errors, ['panes', paneId], 'pane definition must be an object');
        continue;
      }
      if (typeof p.kind !== 'string' || p.kind.length === 0) {
        err(errors, ['panes', paneId, 'kind'], 'pane kind must be a non-empty string');
      } else if (paneKinds && !paneKinds.has(p.kind)) {
        err(errors, ['panes', paneId, 'kind'], "unknown pane kind '" + p.kind + "'");
      }
      if (p.params !== undefined && (!p.params || typeof p.params !== 'object' || Array.isArray(p.params))) {
        err(errors, ['panes', paneId, 'params'], 'pane params must be an object');
      }
      if (p.open && (!Array.isArray(p.open) || !p.open.every(function(s) { return typeof s === 'string'; }))) {
        err(errors, ['panes', paneId, 'open'], 'pane open must be an array of strings');
      }
      if (p.close && (!Array.isArray(p.close) || !p.close.every(function(s) { return typeof s === 'string'; }))) {
        err(errors, ['panes', paneId, 'close'], 'pane close must be an array of strings');
      }
    }
  }
  if (!Array.isArray(layout.roots) || layout.roots.length === 0) {
    err(errors, ['roots'], 'layout.roots must be a non-empty array');
  } else {
    layout.roots.forEach(function(root, i) {
      var p = ['roots', i];
      if (!root || typeof root !== 'object') {
        err(errors, p, 'root must be an object');
        return;
      }
      if (typeof root.id !== 'string' || root.id.length === 0) {
        err(errors, p.concat('id'), 'root id must be a non-empty string');
      }
      if (typeof root.x !== 'number' || !Number.isFinite(root.x)) {
        err(errors, p.concat('x'), 'root.x must be a finite number');
      }
      if (typeof root.y !== 'number' || !Number.isFinite(root.y)) {
        err(errors, p.concat('y'), 'root.y must be a finite number');
      }
      if (typeof root.w !== 'number' || !Number.isFinite(root.w)) {
        err(errors, p.concat('w'), 'root.w must be a finite number');
      }
      if (typeof root.h !== 'number' || !Number.isFinite(root.h)) {
        err(errors, p.concat('h'), 'root.h must be a finite number');
      }
      if (!root.tree) {
        err(errors, p.concat('tree'), 'root.tree is required');
      }
    });
  }
  return { ok: errors.length === 0, errors: errors };
}

export async function loadPreset(id, opts) {
  return { ok: false, errors: [{ path: [], message: 'not implemented' }] };
}
