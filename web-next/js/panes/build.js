/**
 * panes/build.js — the corpus build, as a panel.
 *
 * `diag` answers "what is wrong, and where"; this answers "did it build, and what does the
 * corpus still take on faith" — the two numbers a corpus author needs before believing a
 * green check: how many files verified, and how many theorems are ADMITTED.
 *
 * In:  `diagnostics` (value: a build report)   Out: `jump`        Call: `rerun`
 *
 * The report shape is the same one `POST /api/build` returns and `panes/diag.js` reads
 * (`{files, verified, error_count, warning_count, admitted, failures, warnings}`). The
 * admitted theorems are read from the warnings whose code is `proof.admitted` — the engine
 * says "stated, not proved" that way (crates/build/src/pipeline.rs), so this panel shows the
 * corpus's own axiom footprint as the report carries it, rather than inventing a second one.
 */

import { counts as readCounts } from '../app/diagnostics.js';

/** Every admitted theorem in a report, as `{name, path}`. */
export function admittedOf(report) {
  if (!report) return [];
  const out = [];
  const rows = [].concat(report.warnings || []);
  for (const w of rows) {
    if (!w || w.code !== 'proof.admitted') continue;
    const said = String(w.message || '');
    const name = (said.match(/`([^`]+)`/) || [])[1] || said;
    out.push({ name, path: w.span && w.span.file ? w.span.file : '' });
  }
  return out;
}

/** The one-line summary: what the check found. */
export function summaryOf(report) {
  const c = readCounts(report);
  if (!c) return 'not checked yet';
  const bits = [`${c.verified}/${c.files} verified`];
  if (c.errors) bits.push(`${c.errors} error${c.errors === 1 ? '' : 's'}`);
  if (c.admitted) bits.push(`${c.admitted} admitted`);
  if (c.warnings) bits.push(`${c.warnings} warning${c.warnings === 1 ? '' : 's'}`);
  return bits.join(' · ');
}

export const buildPane = {
  kind: 'build',
  title: 'Build',
  ports: {
    in: { diagnostics: 'value' },
    out: { jump: 'event' },
    call: { rerun: 'call' },
  },

  create(ctx) {
    let wired = null;      // a report delivered on the value port
    let el = null;
    let unsub = null;

    const current = () => wired || (ctx.check ? (ctx.check() || {}).report : null);

    function paint() {
      if (!el) return;
      const report = current();
      const c = readCounts(report);
      el.innerHTML = '';

      const head = document.createElement('div');
      head.style.cssText = 'display:flex;align-items:center;gap:8px;padding:6px 8px;border-bottom:1px solid var(--hair);';
      const run = document.createElement('button');
      run.type = 'button';
      run.textContent = 'build';
      run.title = 'Run the corpus check (Ctrl+Enter)';
      run.style.cssText = 'font:10px/1 var(--font-mono);letter-spacing:.08em;text-transform:uppercase;padding:3px 8px;border:1px solid var(--hair);border-radius:3px;background:transparent;color:var(--ink);cursor:pointer;';
      run.addEventListener('click', () => { if (ctx.runCheck) ctx.runCheck(); });
      head.appendChild(run);

      const line = document.createElement('span');
      line.style.cssText = 'font:10px/1.3 var(--font-mono);color:var(--muted);';
      line.textContent = summaryOf(report);
      head.appendChild(line);
      el.appendChild(head);

      if (!c) {
        const p = document.createElement('p');
        p.style.cssText = 'padding:10px 12px;font:11px/1.5 var(--font-mono);color:var(--dim);margin:0;';
        p.textContent = 'Press “build” to type-check the corpus. The same run the CLI does, over every file.';
        el.appendChild(p);
        return;
      }

      /* the axiom footprint: what the corpus still takes on faith. Under this architecture
         that is a fact about the GROUND plus the corpus's own admissions, and the report
         carries the corpus's half. */
      const admitted = admittedOf(report);
      const sec = document.createElement('div');
      sec.style.cssText = 'padding:8px 12px;';
      const h = document.createElement('div');
      h.style.cssText = 'font:9.5px/1 var(--font-mono);letter-spacing:.14em;text-transform:uppercase;color:var(--dim);margin-bottom:6px;';
      h.textContent = 'admitted · ' + admitted.length;
      sec.appendChild(h);
      if (!admitted.length) {
        const none = document.createElement('div');
        none.style.cssText = 'font:11px/1.4 var(--font-mono);color:var(--green,#34d399);';
        none.textContent = 'none: every theorem in this corpus carries a proof.';
        sec.appendChild(none);
      } else {
        const ul = document.createElement('ul');
        ul.style.cssText = 'list-style:none;margin:0;padding:0;display:flex;flex-direction:column;gap:2px;';
        for (const a of admitted) {
          const li = document.createElement('li');
          li.style.cssText = 'font:11px/1.5 var(--font-mono);color:var(--amber,#fbbf24);cursor:pointer;';
          li.textContent = a.name;
          if (a.path) li.title = a.path;
          li.addEventListener('click', () => {
            const out = ctx.event && ctx.event.jump;
            if (out) out({ path: a.path, line: null });
          });
          ul.appendChild(li);
        }
        sec.appendChild(ul);
      }
      el.appendChild(sec);
    }

    return {
      mount(element) {
        el = element;
        el.style.overflow = 'auto';
        const port = ctx.value && ctx.value.diagnostics;
        if (port && typeof port.subscribe === 'function') {
          unsub = port.subscribe((v) => { wired = v; paint(); });
        }
        paint();
      },
      unmount() {
        if (unsub) { try { unsub(); } catch { /* gone */ } unsub = null; }
        el = null;
      },
      summary: () => summaryOf(current()),
    };
  },
};

export default buildPane;