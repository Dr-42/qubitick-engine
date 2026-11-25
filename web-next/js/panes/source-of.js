/*
 * source-of.js — "edit source": which source a tile has, and what may be done with it.
 *
 * Extracted from the inline script in web-next/index.html. The feature has one rule, and it
 * is a rule about PERMISSION:
 *
 *   a tile declared as DATA (a manifest pane: a program, a code body, a view) can be edited
 *   from inside the app, because the thing being edited is that declaration;
 *
 *   a tile backed by an APP MODULE cannot, because the thing being edited would be the app
 *   itself — opened read-only, in a code view, so you can read the module that draws the
 *   tile you are looking at. An editor that lets a tile quietly rewrite the code drawing it
 *   is an editor that can brick the app it is running in.
 *
 * The kinds of source, in the order they are found:
 *
 *   program   a behaviour node's `program` — a function body the node runs
 *   code      a manifest pane's render code (`render: { type: 'code', code }`)
 *   view      a manifest pane's view (`render: { type: 'view', view }`)
 *   builtin   a manifest pane that DELEGATES to another kind (`render: {type:'builtin', ref}`),
 *             so the source belongs to that kind, not to this one — one level of indirection,
 *             followed, because that is how a manifest pane is made from a shipped pane
 *   module    the app's own kind: a file in this repo, read-only
 *
 * Save follows the same split: a manifest pane's change goes back to `panes/manifests.json`
 * (the declaration is data, so it can be written); a built-in program is live for the
 * session only; app code is not saveable at all.
 */

/** The app module that implements each shipped kind — what "edit source" opens. */
export const MODULE_OF = {
  /* the canvas app's kinds (what a tile can be there) */
  tree: 'web-next/js/live/tree.js', graph: 'web-next/js/live/graph.js', editor: 'web-next/js/live/editor.js',
  handwrite: 'web-next/js/live/editor.js', wires: 'web-next/js/panes/wires.js', output: 'web-next/js/panes/output.js',
  stats: 'web-next/js/panes/stats.js', symbols: 'web-next/js/panes/symbols.js', goal: 'web-next/js/panes/goal.js',
  render: 'web-next/js/panes/render.js', ir: 'web-next/js/panes/ir.js', lean: 'web-next/js/panes/lean.js',
  core: 'web-next/js/panes/core.js', view: 'web-next/js/panes/view.js', prefs: 'web-next/js/panes/prefs.js',
  source: 'web-next/js/panes/source.js', constant: 'web-next/js/panes/constant.js', palette: 'web-next/js/panes/palette.js',
  diag: 'web-next/js/panes/diag.js', switch: 'web-next/js/panes/switch.js', projects: 'web-next/js/panes/projects.js',
  'theme-editor': 'web-next/js/panes/theme-editor.js',

  /* the engine's kinds — the same vocabulary, the shell's half of it. They were missing from
     this table because "edit source" grew on the canvas side, which left every engine kind
     with no answer for it (and a kind with no entry here fell back to index.html, which is
     the one answer that must never be right). One module per kind, both apps. */
  logo: 'web-next/js/panes/logo.js', controls: 'web-next/js/panes/controls.js',
  nodes: 'web-next/js/panes/nodes.js', slate: 'web-next/js/panes/slate.js',
  'symbol-add': 'web-next/js/panes/symbol-add.js', 'new-node': 'web-next/js/panes/new-node.js',
  'script-editor': 'web-next/js/panes/script-editor.js',
  'editor-vim': 'web-next/js/panes/editor-vim.js', 'editor-symbols': 'web-next/js/panes/editor-symbols.js',
  'editor-citations': 'web-next/js/panes/editor-citations.js', 'editor-ctxmenu': 'web-next/js/panes/editor-ctxmenu.js',
  'settings-theme': 'web-next/js/panes/settings-theme.js',
  'settings-typography': 'web-next/js/panes/settings-typography.js',
  'settings-layout': 'web-next/js/panes/settings-layout.js',
  'layout-json': 'web-next/js/panes/layout-json.js', 'layout-outline': 'web-next/js/panes/layout-outline.js',
  project: 'web-next/js/panes/project.js', corpus: 'web-next/js/panes/corpus.js',
};

/**
 * The source behind one tile.
 * @param {object} p             the tile
 * @param {object} o
 * @param {object} o.registry    the pane registry (kind → { program?, render?, _manifest? })
 * @param {object[]} [o.panes]   the live tiles, for a `builtin` that points at a kind
 * @returns {{text:string|null, what:string, editable:boolean, path?:string, save:string|null}|null}
 */
export function sourceOf(p, { registry = {}, panes = [] } = {}) {
  if (!p) return null;
  const rec = registry[p.kind];
  if (!rec) return null;
  const fromManifest = !!rec._manifest;
  if (typeof rec.program === 'string') {
    return { text: rec.program, what: 'program', editable: true, save: fromManifest ? 'manifest' : 'none' };
  }
  const r = rec.render;
  if (r && r.type === 'code') {
    return { text: r.code || '', what: 'code', editable: true, save: fromManifest ? 'manifest' : 'none' };
  }
  if (r && r.type === 'view') {
    return { text: r.view || '', what: 'view', editable: true, save: fromManifest ? 'manifest' : 'none' };
  }
  if (r && r.type === 'builtin') {
    // a manifest pane made from a shipped pane: the source belongs to the shipped kind
    const ref = panes.find((q) => q.kind === r.ref) || { kind: r.ref };
    return sourceOf(ref, { registry });
  }
  // the app's own kinds: their module is a file in this repo, opened read-only
  const path = MODULE_OF[p.kind] || 'web-next/index.html';
  return { text: null, what: 'module', path, editable: false, save: null };
}

/**
 * Put edited text into the running tile: the kind's program or render body is replaced and
 * the tile is mounted again, so the change is visible at once.
 * @param {object} target  the tile
 * @param {string} text
 * @param {object} host    { registry, panes, nodeRuns, userMount, behaviourMount, render, dirty, save }
 */
export function applySource(target, text, host) {
  const rec = target && host.registry[target.kind];
  if (!rec) return false;
  const src = sourceOf(target, { registry: host.registry, panes: host.panes });
  if (!src || !src.editable) return false;
  if (src.what === 'program') {
    rec.program = text;
    if (host.nodeRuns && host.nodeRuns[target.kind]) host.nodeRuns[target.kind] = null;   // recompiled on next run
  } else if (src.what === 'code') {
    rec.render = { ...(rec.render || {}), code: text };
  } else if (src.what === 'view') {
    rec.render = { ...(rec.render || {}), view: text };
  }
  if (src.what === 'code' || src.what === 'view') rec.mount = host.userMount(rec.render);
  if (rec.category === 'behaviour' && host.behaviourMount) rec.mount = host.behaviourMount(rec);
  // a pane may hold subscriptions (the editor does): it is asked to let go before the remount
  if (target.onUnmount) { try { target.onUnmount(); } catch (e) { /* a pane's own teardown */ } target.onUnmount = null; }
  if (!target.el || !target.el.isConnected) return true;
  host.render(target);
  if (host.dirty) host.dirty();
  if (host.save) host.save();
  return true;
}

/**
 * Save an edited source back where it came from.
 * @param {object} target
 * @param {object} host  { registry, panes, manifests, saveManifest, toast }
 */
export async function saveSource(target, host) {
  const rec = target && host.registry[target.kind];
  if (!rec) return false;
  const src = sourceOf(target, { registry: host.registry, panes: host.panes });
  if (!src) return false;
  const say = host.toast || (() => {});
  if (!src.editable) { say('this module is app code — read only'); return false; }
  if (src.save !== 'manifest') { say('this program is built in — the change is live for this session'); return false; }
  const entry = rec._manifest;
  if (!entry) { say('not from the manifest'); return false; }
  if (src.what === 'program') entry.program = src.text;
  else if (src.what === 'code') entry.render = { ...(entry.render || {}), code: src.text };
  else if (src.what === 'view') entry.render = { ...(entry.render || {}), view: src.text };
  const ok = await host.saveManifest(host.manifests);
  say(ok ? ('saved ' + target.kind + ' to the manifest') : 'save refused by the server');
  return ok;
}