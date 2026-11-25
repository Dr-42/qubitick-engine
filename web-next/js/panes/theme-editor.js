/**
 * panes/theme-editor.js — the theme editor, as a tile.
 *
 * A theme is DATA (a module with twelve colours, a field blueprint and a wordmark), so the
 * editor edits the data: it takes a theme module in on `theme` — wire one in, or start from
 * any theme on the board — exposes every field the module has, and emits the edited module
 * on `theme`. Wire that into the theme board's `theme` input and the settings panes follow
 * the draft as you type.
 *
 * Three rules the tile is built around, each of them a bug this form has had:
 *
 *   PUBLISH A COPY, NOT THE DRAFT. A value port suppresses an identical value and a draft is
 *   mutated in place, so handing the live object over made every edit after the first
 *   invisible downstream — the board showed the first keystroke and then nothing.
 *
 *   THE FORM IS BUILT FROM THE MODULES. The colour list comes from `ctx.themes.keys()`, the
 *   blueprint list from the field's own geometry modules, the wordmarks from the brand
 *   modules. A hard-coded list here is a theme whose new key no picker can reach.
 *
 *   SAVING FORKS, IT NEVER OVERWRITES. The shipped themes are read-only on disk, so "save"
 *   forks the draft into a custom theme with an id that cannot collide; "apply" shows the
 *   draft without saving it, because trying something out should not cost a theme.
 *
 * In:  `theme` (value)   Out: `theme` (value)   Call: none
 */

import { valuePort } from '../core/ports.js';
import { ctlRow, ctlNote, ctlBtn, ctlPill, ctlGroup } from '../app/ctl.js';
import { loadSpaces, loadBrands } from '../live/field.js';

const clone = (o) => JSON.parse(JSON.stringify(o));

export default {
  kind: 'theme-editor',
  title: 'Theme · editor',
  ports: { in: { theme: 'value' }, out: { theme: 'value' }, call: {} },

  create(ctx) {
    const themes = ctx.themes;
    const outTheme = valuePort(undefined);
    let draft = null, origin = null;
    let el = null;
    const parts = {};

    /* ---------- the draft ---------- */

    const paint = () => {
      if (!el) return;
      paintOptions();
      if (!draft) { publish(); return; }
      parts.name.value = draft.name || draft.id;
      for (const k of themes.keys()) {
        const inp = parts.colours[k];
        if (!inp) continue;
        inp.value = /^#[0-9a-f]{6}$/i.test(draft.vars[k] || '') ? draft.vars[k] : '#000000';
      }
      if (parts.field) parts.field.value = (draft.blueprint && draft.blueprint.preset) || 'graph';
      if (parts.cell) parts.cell.value = (draft.blueprint && draft.blueprint.minor) || 16;
      const brand = (draft.brand && typeof draft.brand === 'object') ? draft.brand : { mark: draft.brand || '', rotate: -7, border: true };
      if (parts.mark) parts.mark.value = brand.mark || '';
      if (parts.rotate) parts.rotate.value = brand.rotate != null ? brand.rotate : -7;
      if (parts.border) parts.border.checked = brand.border !== false;
      publish();
    };

    /** Hand the draft downstream: a COPY, and only when it actually differs. */
    const publish = () => {
      if (!el) return;
      const out = draft ? clone(draft) : null;
      outTheme.set(out);
      if (parts.json) parts.json.textContent = draft ? JSON.stringify(draft, null, 2) : '';
      if (parts.status) parts.status.textContent = draft ? (draft.source === 'custom' ? 'custom' : 'draft') : 'draft';
    };

    const paintOptions = () => {
      if (!parts.source) return;
      const cards = themes.cards();
      parts.source.innerHTML = '';
      for (const t of cards) {
        const o = document.createElement('option');
        o.value = t.id;
        o.textContent = (t.source === 'custom' ? '◆ ' : '') + (t.name || t.id);
        parts.source.appendChild(o);
      }
      if (draft) parts.source.value = draft.id;
    };

    const load = (id) => {
      const mod = themes.current && id === themes.currentId() ? themes.current() : null;
      const card = mod || (themes.cards() || []).find((t) => t.id === id);
      if (!card) return false;
      origin = clone(card);
      draft = clone(card);
      if (el) paint();
      return true;
    };

    const touch = () => { if (draft) { draft.source = 'custom'; paint(); } };

    /* ---------- the form ---------- */

    function build(body) {
      body.classList.add('ctl-host');
      Object.assign(body.style, { display: 'flex', flexDirection: 'column', overflow: 'hidden' });

      parts.source = document.createElement('select');
      parts.source.id = 'te-source';
      parts.source.title = 'start from a theme module';
      parts.name = document.createElement('input');
      parts.name.id = 'te-name';
      parts.name.type = 'text';
      parts.name.className = 'te-in';
      parts.name.placeholder = 'theme name';
      const applyBtn = ctlBtn('b-te-apply', 'apply', 'apply this draft now — it is not saved');
      const saveBtn = ctlBtn('b-te-save', 'save as theme', 'add this draft to the workspace’s own themes');
      const revertBtn = ctlBtn('b-te-revert', 'revert', 'back to the theme this draft started from');
      parts.json = document.createElement('pre');
      parts.json.className = 'code bout';
      parts.json.id = 'te-json';
      parts.status = ctlPill('te-status', 'draft');

      const form = document.createElement('div');
      form.className = 'te-form';
      const swatches = document.createElement('div');
      swatches.className = 'th-grid te-swatches';
      parts.colours = {};
      for (const k of themes.keys()) {
        const lab = document.createElement('label');
        lab.className = 'th-sw';
        const inp = document.createElement('input');
        inp.type = 'color';
        inp.className = 'th-c';
        inp.id = 'te-c-' + k;
        inp.addEventListener('input', () => { if (!draft) return; draft.vars[k] = inp.value; touch(); });
        parts.colours[k] = inp;
        const t = document.createElement('span');
        t.textContent = k;
        lab.append(inp, t);
        swatches.appendChild(lab);
      }

      parts.field = document.createElement('select');
      parts.field.id = 'te-field';
      parts.cell = document.createElement('input');
      parts.cell.type = 'number';
      parts.cell.min = '4';
      parts.cell.max = '48';
      parts.cell.step = '1';
      parts.cell.className = 'te-num';
      parts.cell.id = 'te-cell';
      parts.field.addEventListener('change', () => { if (!draft) return; draft.blueprint = { ...(draft.blueprint || {}), preset: parts.field.value }; touch(); });
      parts.cell.addEventListener('input', () => { if (!draft) return; draft.blueprint = { ...(draft.blueprint || {}), minor: Number(parts.cell.value) || 16 }; touch(); });

      parts.mark = document.createElement('select');
      parts.mark.id = 'te-mark';
      parts.rotate = document.createElement('input');
      parts.rotate.type = 'number';
      parts.rotate.step = '1';
      parts.rotate.min = '-180';
      parts.rotate.max = '180';
      parts.rotate.className = 'te-num';
      parts.rotate.id = 'te-rotate';
      parts.border = document.createElement('input');
      parts.border.type = 'checkbox';
      parts.border.id = 'te-border';
      const markLabel = document.createElement('label');
      markLabel.append(parts.border, document.createTextNode(' ruled'));

      const brandUpdate = () => {
        if (!draft) return;
        const mark = parts.mark.value;
        draft.brand = mark ? { mark, rotate: Number(parts.rotate.value) || 0, border: !!parts.border.checked } : '';
        touch();
      };
      parts.mark.addEventListener('change', brandUpdate);
      parts.rotate.addEventListener('input', brandUpdate);
      parts.border.addEventListener('change', brandUpdate);

      form.append(
        ctlNote('start from a theme, change what you like, then save it as your own'),
        ctlRow('source', parts.source),
        ctlRow('name', parts.name),
        swatches,
        ctlRow('field', ctlGroup(parts.field, parts.cell)),
        ctlRow('mark', ctlGroup(parts.mark, parts.rotate, markLabel)),
        ctlRow('write', ctlGroup(applyBtn, saveBtn, revertBtn)),
      );
      body.append(form, parts.json);

      parts.source.addEventListener('change', () => { load(parts.source.value); });
      parts.name.addEventListener('input', () => { if (!draft) return; draft.name = parts.name.value; touch(); });
      applyBtn.addEventListener('click', () => { if (!draft) return; themes.apply(clone(draft)); say('draft applied'); });
      saveBtn.addEventListener('click', () => {
        if (!draft) return;
        const mod = themes.fork(draft, parts.name.value || draft.name);
        if (!mod) return;
        themes.register(mod);
        themes.pick(mod.id);
        draft = clone(mod);
        origin = clone(mod);
        paint();
        say('theme saved: ' + (mod.name || mod.id));
      });
      revertBtn.addEventListener('click', () => {
        if (!origin) return;
        draft = clone(origin);
        paint();
        say('reverted');
      });

      /* the lists come from the modules the field and the brand files already carry */
      loadSpaces().then((sp) => {
        if (!parts.field) return;
        parts.field.innerHTML = '';
        for (const [id, s] of Object.entries(sp.spaces || {})) {
          const o = document.createElement('option');
          o.value = id;
          o.textContent = s.name || id;
          parts.field.appendChild(o);
        }
        if (draft && draft.blueprint) parts.field.value = draft.blueprint.preset || sp.default || 'graph';
      }).catch(() => {});
      loadBrands().then((b) => {
        if (!parts.mark) return;
        parts.mark.innerHTML = '';
        const none = document.createElement('option');
        none.value = '';
        none.textContent = 'none';
        parts.mark.appendChild(none);
        for (const [id, m] of Object.entries((b && b.marks) || {})) {
          const o = document.createElement('option');
          o.value = id;
          o.textContent = m.name || id;
          parts.mark.appendChild(o);
        }
        if (draft && draft.brand) parts.mark.value = (typeof draft.brand === 'string' ? draft.brand : (draft.brand.mark || ''));
      }).catch(() => {});
    }

    const say = (m) => { if (ctx.ui && ctx.ui.toast) ctx.ui.toast(m); };

    return {
      in: { theme: { kind: 'value', set(v) { if (v && typeof v === 'object' && v.vars) { draft = clone(v); draft.source = draft.source || 'custom'; paint(); } } } },
      out: { theme: outTheme },

      mount(body, p) {
        el = body;
        build(body);
        const first = (themes.currentId() && themes.cards().find((t) => t.id === themes.currentId())) || themes.cards()[0];
        if (first) load(first.id); else paintOptions();
        publish();
        if (p) p.summary = () => 'theme · ' + (draft ? (draft.name || draft.id) : '—');
      },

      unmount() {
        el = null;
        draft = null;
        origin = null;
        for (const k of Object.keys(parts)) parts[k] = null;
      },
    };
  },
};
