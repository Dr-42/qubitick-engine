/*
 * live/field-host.js — the field behind the tiles: when it exists, and whose choices win.
 *
 * Extracted from the inline script in web-next/index.html. The field is drawn by
 * js/live/field.js from the theme's own ink, and it is the one part of the workspace whose
 * inputs arrive in an order nobody controls:
 *
 *   the THEME FILES (a fetch), the SAVED WORKSPACE (another fetch), the FIELD MODULE (a third),
 *   and the USER (a pick in the field's own picker).
 *
 * So the workspace's field choices — the blueprint and the banner — are kept in the VIEW PREFS
 * and APPLIED from one place, after every theme and after the module lands. A field that
 * applied a theme's blueprint on arrival would silently undo the user's choice on every
 * reload, and a theme that arrived before the field existed would never be applied at all.
 */

/**
 * @param {object} host
 * @param {() => object} host.gridEl              the canvas the field draws on
 * @param {() => object|null} host.currentTheme   the theme module that is on
 * @param {() => object|null} host.api            js/live/theme.js, for `blueprintOf`
 * @param {() => object} host.view                the workspace's view prefs (field, banner)
 * @param {() => {x:number,y:number,z:number}} host.cam
 * @param {() => string[]} [host.tiles]           the kinds that SHOW the field (redrawn when it lands)
 * @param {(p:object) => void} [host.render]      re-draw one tile
 * @param {() => object[]} [host.panes]
 * @param {(p:object) => object|null} [host.tileOfKind]
 * @param {(list:object[]) => void} [host.setSpaces]   the blueprint list, remembered
 * @param {() => void} [host.save]
 * @param {(m:string) => void} [host.warn]
 * @param {() => Promise<object>} [host.loadField]  how the field module is loaded (injectable)
 */
export function createFieldHost(host) {
  const warn = host.warn || ((m) => console.warn(m));
  const loadField = host.loadField || (() => import('./field.js'));
  let field = null;
  let spaces = [];

  /** The field instance, once it exists. Everything else asks. */
  const instance = () => field;

  /** The blueprint list the field published, for the pickers. */
  const spaceList = () => spaces;

  /**
   * Apply the workspace's own field choices. Harmless before the field exists, which is why
   * it can be called from anywhere (after a theme, after a restore, after the module lands).
   */
  function applyPrefs() {
    if (!field) return false;
    const v = host.view ? host.view() : {};
    if (v.field) field.setSpace(v.field);
    if (v.banner) field.setBanner(v.banner);
    field.sync(host.cam());
    return true;
  }

  /** Create the field and put the current theme on it. Resolves to the instance or null. */
  async function boot() {
    if (field) return field;
    let mod;
    try { mod = await loadField(); }
    catch (e) { warn('[field] ' + (e && e.message)); return null; }
    try {
      field = mod.createField(host.gridEl(), {
        onPickSpace: () => {
          // a pick in the field is a preference: it is saved like any other
          const v = host.view ? host.view() : null;
          if (v) v.field = field.space;
          if (host.save) host.save();
        },
      });
      const theme = host.currentTheme ? host.currentTheme() : null;
      const api = host.api ? host.api() : null;
      if (theme && api) field.setTheme(theme, api.blueprintOf(theme));
      applyPrefs();
      // the picker is built from the blueprint modules, so it can only be filled once the
      // field has read them; the tile that shows it is redrawn then
      field.spaces().then((r) => {
        spaces = (r && r.list) || [];
        if (host.setSpaces) host.setSpaces(spaces);
        const tile = host.tileOfKind ? host.tileOfKind('view') : null;
        if (tile && tile.lod === 'live' && host.render) host.render(tile);
      }).catch(() => {});
    } catch (e) {
      warn('[field] ' + (e && e.message));
      return null;
    }
    return field;
  }

  return { boot, applyPrefs, instance, spaceList, ready: () => !!field };
}