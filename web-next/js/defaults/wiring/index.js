/**
 * defaults/wiring/index.js — the built-in wiring presets, by name.
 *
 * A layout document names its wiring ("standard", "power", "minimal") and the loader
 * used to fetch it by BUILDING A URL — `/js/defaults/wiring/<name>.js` — and handing it
 * to `import()`. That works in a browser and hides the presets from everything else: a
 * static read of the tree cannot see a path that is assembled at runtime, so the five
 * modules behind those names looked like five modules nothing loads. They were loaded
 * every boot; only the edge was invisible.
 *
 * So the names live here, statically imported. The loader asks this table first and
 * falls back to the URL for a wiring script that is not built in — which is the case
 * the URL path exists for: a user's own script, stored under `wiring:<name>` or served
 * from somewhere else.
 *
 * Adding a preset is adding a row here. A name that is not here is not a preset.
 */

import standard from './standard.js';
import power from './power.js';
import minimal from './minimal.js';

/** name → wiring function (each takes the workspace it wires into). */
export const BUILT_IN = { standard, power, minimal };

/** The names a layout may ask for. */
export const WIRING_NAMES = Object.keys(BUILT_IN);

/** Is this a built-in wiring script? */
export const isBuiltInWiring = (name) => Object.prototype.hasOwnProperty.call(BUILT_IN, String(name));

/**
 * The built-in wiring script for a name, or null.
 * @param {string} name
 * @returns {Function|null}
 */
export function builtInWiring(name) {
  return isBuiltInWiring(name) ? BUILT_IN[String(name)] : null;
}
