/**
 * panes/prefs.js — Editor prefs pane.
 *
 * The five knobs every editor tile shares: font size, line height, tab width, soft wrap, and the
 * check strip's `suggest rewrites`. One pane, one set of values, every editor in the workspace at
 * once — which is why it is a tile rather than a menu: you turn the knob and watch the text change.
 *
 * In:  {}   Out: {}   Call: `reset`
 *
 * The ranges are registry data (`REG.editor` in the page), not constants here: a slider
 * whose limits live in the pane would silently disagree with the value it is showing.
 * Each change saves the workspace (a preference you cannot keep is not a preference) and every
 * consumer is told through `ctx.applyEditorPrefs()` — the editors retune themselves, and a tile
 * with more than an editor on it (the check strip) is called back by name (js/app/prefs.js).
 */

import { callPort } from '../core/ports.js';
import { ctlRow, ctlSec, ctlNote, ctlGroup, ctlPill, ctlCheck } from '../app/ctl.js';
import { SAVE_MODES } from '../app/prefs.js';

export default {
  kind: 'prefs',
  title: 'Editor prefs',
  ports: { in: {}, out: {}, call: { reset: 'call' } },
  create(ctx) {
    const reg = (ctx.editorRange && ctx.editorRange()) || {
      fontRange: { min: 11, max: 18, step: 1 },
      lineHeightRange: { min: 1.2, max: 2.2, step: 0.05 },
      tabSizes: [2, 4, 8],
    };
    let el = null, font = null, fontV = null, lh = null, lhV = null, tab = null, wrap = null, suggest = null;

    const prefs = () => (ctx.editorPrefs ? ctx.editorPrefs() : {});

    /** Write one preference and let everything that reads it know. */
    function set(key, value) {
      if (ctx.setEditorPref) ctx.setEditorPref(key, value);
      else if (ctx.editorPrefs) Object.assign(ctx.editorPrefs(), { [key]: value });
      if (ctx.applyEditorPrefs) ctx.applyEditorPrefs();
      if (ctx.saveWorkspace) ctx.saveWorkspace();
    }

    const callReset = callPort(() => {
      set('fontSize', 13);
      set('lineHeight', 1.75);
      set('tabSize', 2);
      set('wrap', true);
      set('suggest', false);            // the shipped default: the rewrites are a tool, not a default
      paint();
      return { ok: true };
    });

    function paint() {
      const p = prefs();
      if (font) font.value = p.fontSize;
      if (fontV) fontV.textContent = p.fontSize + 'px';
      if (lh) lh.value = p.lineHeight;
      if (lhV) lhV.textContent = String(p.lineHeight);
      if (tab) tab.value = p.tabSize;
      if (wrap) wrap.checked = p.wrap !== false;
      /* the checkbox is repainted like the rest: a value changed anywhere else (a restored
         workspace, another tile, `reset`) must not leave the control showing the old one */
      if (suggest) suggest.checked = p.suggest === true;
    }

    return {
      /* No wrapper element: `ctx.controlColumns` arranges the sections inside the host
         itself (it needs to be the grid), so the pane hands it the sections and keeps the
         references to the controls it built. */
      mount(host) {
        el = host;

        font = document.createElement('input');
        font.id = 'ed-font'; font.type = 'range';
        font.min = reg.fontRange.min; font.max = reg.fontRange.max; font.step = reg.fontRange.step;
        font.style.width = '110px';
        fontV = ctlPill('ed-font-v', '13px');

        lh = document.createElement('input');
        lh.id = 'ed-lh'; lh.type = 'range';
        lh.min = reg.lineHeightRange.min; lh.max = reg.lineHeightRange.max; lh.step = reg.lineHeightRange.step;
        lh.style.width = '110px';
        lhV = ctlPill('ed-lh-v', '1.75');

        tab = document.createElement('select');
        tab.id = 'ed-tab';
        for (const n of reg.tabSizes) {
          const o = document.createElement('option');
          o.value = n; o.textContent = n;
          tab.append(o);
        }
        wrap = ctlCheck('ed-wrap', 'wrap', prefs().wrap !== false);
        /* THE CHECK STRIP'S OTHER HALF: with this on, an editor tile offers the rewrites the
           corpus can reach from where the caret is (POST /api/steps) as chips beside the row's
           verdict. Off by default: a list of possible next states is a tool, not a default. */
        suggest = ctlCheck('ed-suggest', 'suggest rewrites', prefs().suggest === true);

        /* THE SAVE MODE. It lives here rather than in the console because a control tile's column width
           is measured from its own content: a new row there moved the restored split ratio
           (measured — the same seam came back at 0.495 instead of 0.4637). A
           settings tile is not measured that way, and this is a setting. */
        const saveMode = document.createElement('select');
        saveMode.id = 's-save-mode';
        saveMode.title = 'what a save does besides writing the file';
        for (const m of SAVE_MODES) {
          const o = document.createElement('option');
          o.value = m.id;
          o.textContent = m.label;
          saveMode.appendChild(o);
        }
        saveMode.value = ctx.saveMode ? ctx.saveMode() : 'none';
        saveMode.addEventListener('change', () => { if (ctx.setSaveMode) ctx.setSaveMode(saveMode.value); });

        const sections = [
          ctlSec('save', [ctlRow('after saving', saveMode)]),
          ctlSec('code', [
            ctlRow('font', ctlGroup(font, fontV)),
            ctlRow('line', ctlGroup(lh, lhV)),
            ctlRow('tab', ctlGroup(tab, wrap, suggest)),
          ]),
          ctlNote('applies to every editor tile at once'),
        ];

        // the splitter belongs to the app: the pane offers sections, the host arranges them
        if (ctx.controlColumns) ctx.controlColumns(host, sections);
        else host.append(...sections);

        font.addEventListener('input', (e) => { set('fontSize', +e.target.value); paint(); });
        lh.addEventListener('input', (e) => { set('lineHeight', +e.target.value); paint(); });
        tab.addEventListener('change', (e) => { set('tabSize', +e.target.value); paint(); });
        wrap.addEventListener('change', (e) => { set('wrap', !!e.target.checked); paint(); });
        suggest.addEventListener('change', (e) => { set('suggest', !!e.target.checked); paint(); });

        paint();
      },
      unmount() {
        // the host clears its own body before the next mount; nothing here is a wrapper
        el = font = fontV = lh = lhV = tab = wrap = suggest = null;
      },
      refresh: paint,
      in: {}, out: {},
      call: { reset: callReset },
      get ports() { return { in: this.in, out: this.out, call: this.call }; },
    };
  },
};