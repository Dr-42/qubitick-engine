/*
 * host/canvas/themes-app.js — the theme subsystem as THIS app wires it, plus the field it paints.
 *
 * Extracted from the inline script in web-next/index.html. The state machine is
 * `js/live/theme-host.js` and the field is `js/live/field-host.js`; what was in the page was the
 * wiring between them and the app:
 *
 *   THE APPLIED VARIABLES ARE PUBLISHED AS A NODE RESULT. The palette node's `theme` output is
 *   the twelve variables as applied, so a wire from the palette to a settings tile carries the
 *   theme — and `THEME` is that mirror, kept here because the workspace document reads it (an
 *   edited palette is saved as VALUES, with the identity kept).
 *
 *   THE TILES THAT SHOW A THEME ARE REDRAWN WHEN IT CHANGES. The board and the settings tile
 *   render from the live answer; a tile that is not on screen is marked `pendingMount` instead of
 *   being built for a change nobody can see.
 *
 *   THE FIELD'S CHOICES BELONG TO THE WORKSPACE, NOT TO THE THEME. The blueprint and the banner
 *   are the user's, kept in the view prefs and re-applied after every theme — so a theme's own
 *   blueprint cannot silently win on the next reload.
 */

import { createThemeHost } from '../../live/theme-host.js';
import { createFieldHost } from '../../live/field-host.js';

/**
 * @param {object} app
 *   toast(m), render(p), fieldEl(), viewPrefs(), cam(), save(), diag(o),
 *   publishVars(vars), panes, addFreeTile(kind), paneFocus(p), dynWires(), updateWireCount(),
 *   openEditorFrom(p)
 * @param {object} deps { UI }
 */
export function createThemesApp(app, deps) {
  const { UI } = deps;
  let theme = {};                    // the twelve vars as applied (the palette node's result)
  let spaces = [];                   // blueprints.json, once the field has read it

  /* ---------------- the field ---------------- */
  const FIELD_HOST = createFieldHost({
    gridEl: () => app.fieldEl(),
    currentTheme: () => THEMES.current(),
    api: () => THEMES.api(),
    view: () => app.viewPrefs(),
    cam: () => app.cam(),
    render: (q) => app.render(q),
    tileOfKind: (kind) => app.panes.find((q) => q.kind === kind) || null,
    setSpaces: (list) => { spaces = list; },
    save: () => app.save(),
  });
  const fieldBoot = () => FIELD_HOST.boot();
  const applyFieldPrefs = () => FIELD_HOST.applyPrefs();
  /** The field instance is the host's: this is the app's one way to ask for it. */
  const theField = () => FIELD_HOST.instance();
  const fieldSpaces = () => (spaces.length ? spaces : [{ id: 'graph', name: 'graph' }, { id: 'void', name: 'none' }]);

  /* ---------------- the theme state machine ---------------- */
  const THEMES = createThemeHost({
    root: document.documentElement,
    toast: (m) => app.toast(m),
    field: theField,
    applyFieldPrefs: () => applyFieldPrefs(),
    view: () => app.viewPrefs(),
    redraw: () => refreshThemeUI(),
    diag: (o) => app.diag(o),
    onApplied: (mod, vars) => {
      theme = vars;
      // the palette node's output IS the applied theme: a wire from it carries the colours
      app.publishVars(vars);
    },
  });

  /** The tiles that SHOW a theme, re-drawn when the answer changes. */
  function refreshThemeUI() {
    for (const p of app.panes) {
      if (p.kind === 'palette' || p.kind === 'view') {
        if (p.lod === 'live') app.render(p); else p.pendingMount = true;
      }
    }
  }

  /** Open the editor beside a tile and wire it in: the editor's `theme` output to that tile's
   *  `theme` input. One click, because that is the pairing's whole point. */
  function openThemeEditor(fromPane) {
    let ed = app.panes.find((q) => q.kind === 'theme-editor');
    if (!ed) { const id = app.addFreeTile('theme-editor'); ed = app.panes.find((q) => q.id === id); }
    if (ed && fromPane) {
      const dyn = app.dynWires();
      const already = dyn.some((w) => w.from.p === ed.id && w.to.p === fromPane.id && w.to.port === 'theme');
      if (!already) {
        dyn.push({ id: 'd' + dyn.length + ':' + (Date.now().toString(36)), from: { p: ed.id, port: 'theme' }, to: { p: fromPane.id, port: 'theme' }, kind: 'value', type: 'JSON' });
        app.updateWireCount();
        app.toast('theme editor → ' + fromPane.kind);
      }
    }
    if (ed) app.paneFocus(ed);
    app.save();
    return ed && ed.id;
  }

  return {
    THEMES, FIELD_HOST, theField, fieldBoot, applyFieldPrefs, fieldSpaces,
    refreshThemeUI, openThemeEditor,
    theme: () => theme,
    themeKeys: () => THEMES.keys(),
    currentThemeMod: () => THEMES.current(),
    currentThemeName: () => THEMES.currentName(),
    applyThemeModule: (mod) => THEMES.applyModule(mod),
    applyTheme: (vars) => THEMES.applyVars(vars),
    setThemeById: (id, opts) => THEMES.pick(id, opts),
    themeBoot: () => THEMES.boot(),
    registerTheme: (mod, opts) => THEMES.register(mod, opts),
    forgetTheme: (id) => THEMES.forget(id),
    forkTheme: (mod, name) => THEMES.fork(mod, name),
    themeCards: () => THEMES.cards(),
    isShipped: (id) => THEMES.isShipped(id),
    themeMods: THEMES.mods,
    customThemes: THEMES.custom,
    readVars: () => THEMES.readVars(),
  };}
