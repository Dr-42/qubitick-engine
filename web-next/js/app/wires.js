/*
 * wires.js — resolving what a wire carries, and expanding the declared wires.
 *
 * Extracted from the inline script in web-next/index.html. Two rules live here, and both
 * fail SILENTLY when they are wrong — a wire that does not resolve is a wire that does not
 * exist, and nothing on screen says so:
 *
 *   RESOLVING an input's `from`. A port source is named, not looked up: `editor_buffer`,
 *   `clock`, `theme`, or `node:<id>` with an optional `.field` into that node's result. The
 *   important half is the last one: a node that has not run yet resolves to `undefined`, not
 *   to a stale value, so a program sees "no input" rather than the previous tick's.
 *
 *   EXPANDING a declared wire into the pairs that actually exist. A declared wire names two
 *   KINDS (`editor.buffer → render`), and the workspace may hold several tiles of each, so
 *   one declaration becomes every live pairing — a tile spawned away from its cluster is
 *   still part of the graph. The ports are picked when the declaration gives only a module:
 *   the input with the matching type, else the first one. There is a cap on the pairing (25
 *   per declaration), because N tiles of one kind feeding M of another is N×M wires and a
 *   workspace with a dozen of each would quietly draw hundreds.
 */

/** Every named port source a program may name, as `from`. */
export const SOURCE_NAMES = ['editor_buffer', 'editor_file', 'editor_cursor', 'clock', 'theme', 'constant'];

/**
 * The value an input's `from` names.
 * @param {string} from
 * @param {object} o
 * @param {(name: string) => any} o.value      a named source (editor_buffer, clock, ...)
 * @param {(id: string, field?: string) => any} o.nodeValue  a node's last result
 */
export function resolveValue(from, { value, nodeValue } = {}) {
  if (!from) return undefined;
  if (SOURCE_NAMES.includes(from)) return value ? value(from) : undefined;
  /* `node:<id>` or a bare id, optionally with a field: `node:js-node-1.result`. The id may
     contain spaces and dashes (a user pane's id is its title), so the pattern is loose on
     purpose and the field is the only part that may not. */
  const m = /^(?:node:)?([a-zA-Z0-9$_ -]+?)(?:\.([\w$]+))?$/.exec(String(from));
  if (!m) return undefined;
  if (!nodeValue) return undefined;
  const r = nodeValue(m[1]);
  if (r == null) return undefined;          // never run: undefined, not a stale value
  return m[2] ? r[m[2]] : r;
}

/** A pane kind's ports, in one shape, whichever shape the registry declared them in. */
export function portsOf(rec, dir) {
  if (!rec) return [];
  const list = (dir === 'out' ? rec.outputs : rec.inputs) || [];
  return list
    .map((p) => (Array.isArray(p) ? { id: p[0], type: p[1] } : { id: p.id || p.name, type: p.type || 'any' }))
    .filter((p) => p && p.id);
}

/** The declared type of one port, or 'any' when the registry did not say. */
export function typeOfPort(rec, dir, port) {
  const hit = portsOf(rec, dir).find((p) => p.id === port);
  return hit ? hit.type : 'any';
}

/**
 * The port a half-written wire means: the one whose type matches what is coming, else the
 * first. A declaration that names only a module has to be resolved to a port somehow, and
 * "the one that fits" is the only answer that does not require the author to say.
 */
export function pickPort(rec, dir, wantType) {
  const list = portsOf(rec, dir);
  if (!list.length) return null;
  return (list.find((p) => p.type === wantType) || list[0]).id;
}

/** The cap on one declaration's pairings: N of one kind feeding M of another is N×M. */
export const MAX_PAIRS = 25;

/**
 * Expand the declared wires into the pairs that exist right now.
 *
 * @param {Array} declared  [{ from:'editor.buffer', to:'render', type:'value', port:'Text' }]
 *                              or a concrete { fromP, toP } which is passed through
 * @param {object} o
 * @param {(kind: string) => string[]} o.instancesOf   the live tile ids of a kind
 * @param {(kind: string) => object} o.registry        a kind's record, for its ports
 * @param {(id: string) => string} o.instanceKind      a tile id's kind (for `fromP`/`toP`)
 * @param {Array} [o.extra]                            the wired-in ones (DYN_WIRES)
 */
export function expandWires(declared = [], { instancesOf, registry, instanceKind, extra = [] } = {}) {
  const out = [];
  declared.forEach((w, i) => {
    // a concrete wire (drawn by hand, or seeded with explicit ids) is already a pair
    if (w.fromP || w.toP) return;
    const [fk, fport0] = String(w.from).split('.');
    const [tk, tport0] = String(w.to).split('.');
    const froms = instancesOf ? instancesOf(fk) : [];
    const tos = instancesOf ? instancesOf(tk) : [];
    if (!froms.length || !tos.length) return;                  // a wire with no ends is not a wire
    const recF = registry ? registry(fk) : null;
    const recT = registry ? registry(tk) : null;
    const fport = fport0 || pickPort(recF, 'out');
    const wantType = typeOfPort(recF, 'out', fport);
    const tport = tport0 || pickPort(recT, 'in', wantType);
    if (!fport || !tport) return;
    let n = 0;
    for (const fp of froms) {
      for (const tp of tos) {
        if (n++ > MAX_PAIRS) break;
        out.push({
          id: 'w' + i + ':' + fp + '>' + tp,
          from: { p: fp, port: fport },
          to: { p: tp, port: tport },
          kind: w.type || 'value',
          type: w.port || wantType,
        });
      }
    }
  });
  return out.concat(extra);
}