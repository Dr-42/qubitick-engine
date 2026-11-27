/**
 * define.js — Pane definition validation (M2.04).
 *
 * Validates and returns a pane definition object.
 * Throws TypeError for invalid definitions.
 */

// D-028: Pane kind strings are exactly these 24
const VALID_KINDS = new Set([
  'logo', 'controls', 'nodes', 'tree', 'editor', 'editor-vim', 'editor-symbols', 'editor-citations', 'editor-ctxmenu', 'goal',
  'ir', 'lean', 'render', 'output', 'diag', 'graph',
  'slate', 'symbol-add', 'handwrite',
  'settings-theme', 'settings-typography', 'settings-layout',
  'layout-json', 'layout-outline', 'script-editor',
  'stats', 'new-node',
  /* the swap: a project source and the corpus it serves (js/panes/{project,corpus}.js) */
  'project', 'corpus',
  /* the project LIST — as portable as the two above it, and it was the canvas's private kind for no
     reason: it declares in/out/call ports and asks its ctx for the host's project services, so it
     mounts in either app (js/panes/projects.js). It was missing here, so a layout document that
     named it in the engine shell was "Unknown pane kind" and drew a placeholder. */
  'projects',
  /* the corpus BUILD — verified/admitted/errors from one report (js/panes/build.js). `diag` answers
     "what is wrong and where"; this answers "did it build, and what is still taken on faith", and
     the docking shell's Build panel is the one that asks. */
  'build',
]);

const VALID_PORT_TYPES = new Set(['value', 'event', 'call']);

/**
 * Validate a pane definition.
 * @param {object} def - Pane definition
 * @param {string} def.kind - Must be in VALID_KINDS or start with 'x-'
 * @param {string} [def.title] - Display title (optional)
 * @param {object} def.ports - { in: {...}, out: {...}, call: {...} }
 *   Port values must be among 'value' | 'event' | 'call'
 * @param {function} def.create - Factory function returning pane instance
 * @returns {object} The validated definition (same object)
 * @throws {TypeError} If validation fails
 */
export function definePane(def) {
  if (!def || typeof def !== 'object') {
    throw new TypeError('Pane definition must be an object');
  }

  // kind: must be in VALID_KINDS or start with 'x-'
  if (typeof def.kind !== 'string') {
    throw new TypeError('Pane definition.kind must be a string');
  }
  if (!VALID_KINDS.has(def.kind) && !def.kind.startsWith('x-')) {
    throw new TypeError(`Pane kind must be one of: ${[...VALID_KINDS].join(', ')} or start with 'x-', got: ${def.kind}`);
  }

  // ports: { in, out, call } with valid port types
  if (!def.ports || typeof def.ports !== 'object') {
    throw new TypeError('Pane definition.ports must be an object');
  }
  for (const direction of ['in', 'out', 'call']) {
    const ports = def.ports[direction];
    if (ports !== undefined) {
      if (typeof ports !== 'object' || ports === null) {
        throw new TypeError(`Pane definition.ports.${direction} must be an object`);
      }
      for (const [name, type] of Object.entries(ports)) {
        if (typeof name !== 'string') {
          throw new TypeError('Port name must be a string');
        }
        if (!VALID_PORT_TYPES.has(type)) {
          throw new TypeError(`Port '${name}' type must be one of: value, event, call; got: ${type}`);
        }
      }
    }
  }

  // create: must be a function
  if (typeof def.create !== 'function') {
    throw new TypeError('Pane definition.create must be a function');
  }

  // title: optional string
  if (def.title !== undefined && typeof def.title !== 'string') {
    throw new TypeError('Pane definition.title must be a string if provided');
  }

  return def;
}
