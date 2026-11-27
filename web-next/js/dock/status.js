/**
 * dock/status.js — the status strip, and the build it reports.
 *
 * Two things live here because they are one thing: the CHECK and the numbers it produced.
 * Every face that shows the corpus's state — the strip, the Build panel's summary, the
 * command palette's "run check" entry — reads `createBuildService`, so no two of them can
 * disagree about how many files verified.
 *
 * THE REPORT IS THE CLI'S SHAPE (crates/api/src/handlers.rs::post_build): `{ files,
 * verified, error_count, warning_count, admitted, failures: [{path, diagnostics}], warnings }`.
 * `admitted` is a WARNING-class fact and is never folded into `verified`: a theorem nobody
 * proved is stated, not proved, and the strip says both numbers.
 */

/** The shape a "nothing has run yet" answer has, so callers never special-case null. */
export function idleState() {
  return { busy: false, report: null, error: null, at: 0, took: 0 };
}

/**
 * Create the build service.
 *
 * @param {object} opts
 * @param {function} opts.fetch - fetch
 * @param {string} [opts.path] - the build route
 * @param {object} [opts.bus] - bus to emit `dock:build` on when a report lands
 * @returns {object} { state, run, onChange }
 */
export function createBuildService({ fetch: doFetch, path = '/api/build', bus } = {}) {
  let state = idleState();
  const subs = new Set();

  const publish = () => { for (const fn of subs) { try { fn(state); } catch (err) { console.error('build subscriber:', err); } } };

  function onChange(fn) {
    subs.add(fn);
    fn(state);
    return () => subs.delete(fn);
  }

  async function run() {
    if (state.busy) return state.report;
    const t0 = performance.now();
    state = Object.assign({}, state, { busy: true, error: null });
    publish();
    try {
      const res = await doFetch(path, { method: 'POST', headers: { 'content-type': 'application/json' }, body: '{}' });
      if (!res.ok) {
        const text = await res.text().catch(() => '');
        throw new Error('HTTP ' + res.status + (text ? ': ' + text.slice(0, 200) : ''));
      }
      const report = await res.json();
      state = { busy: false, report, error: null, at: Date.now(), took: Math.round(performance.now() - t0) };
    } catch (err) {
      state = { busy: false, report: null, error: err.message || String(err), at: Date.now(), took: Math.round(performance.now() - t0) };
    }
    publish();
    /* NOT `build:settled`. That event is the shared wiring's "somebody else finished a build,
       refresh what you show" (js/defaults/wiring/_flows.js), and its handler asks the
       Diagnostics panel to RERUN — which is this function. Emitting it here made one press of
       "check" start a check, finish it, start another, finish it… each one a full corpus build,
       and the strip sat on "checking…" forever while the report it already had went stale.
       `dock:build` is the notification; whoever wants to re-run asks for it explicitly. */
    if (bus) {
      try { bus.emit('dock:build', state.report); } catch { /* no bus, no event */ }
    }
    return state.report;
  }

  return { get state() { return state; }, run, onChange };
}

/** The numbers the strip shows, from a report that may not exist yet. */
export function buildCounts(report) {
  if (!report) return null;
  const files = Number(report.files) || 0;
  const verified = Number(report.verified) || 0;
  const errors = Number(report.error_count) || 0;
  const warnings = Number(report.warning_count) || 0;
  const admitted = Number(report.admitted) || 0;
  return { files, verified, errors, warnings, admitted };
}

/**
 * Create the status strip.
 *
 * @param {object} opts
 * @param {object} opts.doc - the live dock document
 * @param {object} opts.build - the build service
 * @param {object} opts.actions - { setPreset, reset, toggleDock, runBuild, setTheme, panels }
 * @param {string} opts.presetKey - the preset the layout came from
 * @param {object} opts.themes - { list, current }, for the selector
 * @returns {object} { el, update }
 */
export function createStatusStrip({ doc, build, actions, presetKey, themes }) {
  const strip = document.createElement('footer');
  strip.className = 'dock-status';
  strip.setAttribute('role', 'contentinfo');

  const seg = (parent, cls) => {
    const s = document.createElement('span');
    s.className = 'st-seg' + (cls ? ' ' + cls : '');
    parent.appendChild(s);
    return s;
  };

  /* the preset selector: a select, because it must be reachable by keyboard and by name */
  const presetSel = document.createElement('select');
  presetSel.className = 'st-preset';
  presetSel.title = 'Layout preset (Ctrl+Alt+1/2/3)';
  for (const [key, label] of [['minimal', 'Minimal'], ['standard', 'Standard'], ['power', 'Power'], ['custom', 'Custom']]) {
    const o = document.createElement('option');
    o.value = key;
    o.textContent = label;
    presetSel.appendChild(o);
  }
  presetSel.value = presetKey || 'standard';
  presetSel.addEventListener('change', () => actions.setPreset(presetSel.value));
  strip.appendChild(presetSel);

  const stateSeg = seg(strip, 'st-build');
  stateSeg.textContent = 'idle';
  stateSeg.title = 'Corpus check: Ctrl+Enter';

  const countsSeg = seg(strip, 'st-counts');
  countsSeg.textContent = '—';

  const spacer = document.createElement('span');
  spacer.className = 'st-spacer';
  strip.appendChild(spacer);

  const btn = (label, title, fn) => {
    const b = document.createElement('button');
    b.type = 'button';
    b.textContent = label;
    b.title = title;
    b.addEventListener('click', fn);
    strip.appendChild(b);
    return b;
  };

  btn('check', 'Run the corpus check (Ctrl+Enter)', () => actions.runBuild());
  btn('reset', 'Reset this layout to its preset (Ctrl+Alt+R)', () => actions.reset());

  /* the theme selector. It writes the same theme files the canvas reads — this is not a
     second theme system, it is the same one, worn here. */
  if (themes && themes.list && themes.list.length) {
    const themeSel = document.createElement('select');
    themeSel.className = 'st-theme';
    themeSel.title = 'Theme';
    for (const t of themes.list) {
      const o = document.createElement('option');
      o.value = t.id;
      o.textContent = t.id;
      themeSel.appendChild(o);
    }
    themeSel.value = themes.current || '';
    themeSel.addEventListener('change', () => actions.setTheme(themeSel.value));
    strip.appendChild(themeSel);
  }

  btn('canvas', 'Open the canvas workspace', () => { window.location.href = '/index.html'; });

  function update() {
    const s = build.state;
    if (s.busy) { stateSeg.textContent = 'checking…'; stateSeg.className = 'st-seg st-build st-warn'; }
    else if (s.error) { stateSeg.textContent = 'check failed'; stateSeg.className = 'st-seg st-build st-bad'; stateSeg.title = s.error; }
    else if (!s.report) { stateSeg.textContent = 'never checked'; stateSeg.className = 'st-seg st-build'; }
    else {
      const c = buildCounts(s.report);
      stateSeg.textContent = c.errors ? 'errors' : (c.admitted ? 'admitted' : 'verified');
      stateSeg.className = 'st-seg st-build ' + (c.errors ? 'st-bad' : (c.admitted ? 'st-warn' : 'st-ok'));
      countsSeg.textContent = `${c.verified}/${c.files} ok · ${c.errors} err · ${c.admitted} admitted`;
      countsSeg.title = `${c.warnings} warning(s)`;
      return;
    }
    countsSeg.textContent = '—';
  }

  build.onChange(() => update());
  update();

  return { el: strip, update, presetSelect: presetSel };
}