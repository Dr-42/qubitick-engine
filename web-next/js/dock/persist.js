/**
 * dock/persist.js — the layout, remembered per project.
 *
 * A docking layout is a property of HOW SOMEONE WORKS ON A CORPUS, not of the browser
 * profile: the tree is narrow when the corpus is deep, the bottom dock is tall when the
 * corpus has errors. So the key carries the project id — two projects on one machine get
 * two layouts — and the store is localStorage, which is synchronous and survives a reload
 * with no request to make.
 *
 * A stored document is never trusted raw: `model.js::normalize` drops a panel this build
 * does not ship and repairs a group with no tabs. A layout from an older build must open
 * the editor, not paint a blank dock.
 */

import { normalize } from './model.js';

const VERSION = 1;
export const KEY_PREFIX = 'qubitick.dock.v' + VERSION + '.';

/** The storage key for a project. `default` is the key for "no project chosen yet". */
export function keyFor(projectId) {
  return KEY_PREFIX + (projectId || 'default');
}

/** Whether this browser can remember anything at all (a private window may not). */
export function available() {
  try {
    const k = KEY_PREFIX + '__probe';
    window.localStorage.setItem(k, '1');
    window.localStorage.removeItem(k);
    return true;
  } catch {
    return false;
  }
}

/** The saved document for a project, or null. */
export function loadDock(projectId) {
  if (typeof window === 'undefined' || !window.localStorage) return null;
  try {
    const raw = window.localStorage.getItem(keyFor(projectId));
    if (!raw) return null;
    const parsed = JSON.parse(raw);
    if (!parsed || parsed.v !== VERSION || !parsed.doc) return null;
    return normalize(parsed.doc);
  } catch (err) {
    console.warn('[dock] stored layout unreadable:', err.message);
    return null;
  }
}

/** Remember a document for a project. Returns false when storage refused. */
export function saveDock(projectId, doc) {
  if (typeof window === 'undefined' || !window.localStorage) return false;
  try {
    window.localStorage.setItem(keyFor(projectId), JSON.stringify({ v: VERSION, doc, at: Date.now() }));
    return true;
  } catch (err) {
    console.warn('[dock] could not store the layout:', err.message);
    return false;
  }
}

/** Forget a project's layout: what "Reset layout" leaves behind before writing the preset. */
export function forgetDock(projectId) {
  try { window.localStorage.removeItem(keyFor(projectId)); } catch { /* nothing to do */ }
}