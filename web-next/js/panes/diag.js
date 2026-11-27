/**
 * panes/diag.js — Diagnostics pane: the corpus check report.
 *
 * Press the button and the engine type-checks every file and checks every formula; what
 * comes back is a REPORT, and this pane is the surface that reads it: how many files
 * verified, what failed, what warned, each with its file, its line, its code and any hints.
 * A row is clickable, because the point of a diagnostic is to get you to the file.
 *
 * In:  `diagnostics` (value: a report, so a program node can feed one in)
 * Out: `jump` (event {path,line} when a row is clicked)   Call: `rerun`, `check`
 *
 * The report shape is the contract (see the CLI and the API): `{ files, verified,
 * error_count, warning_count, failures: [{ path, diagnostics: [...] }], warnings: [...] }`.
 * Reading it is arithmetic on that shape, so the counting and row-building live in
 * `readReport` below and are unit-tested; the pane draws what they return.
 *
 * A warning is NOT a failure: `verified` may be lower than `files` while nothing is wrong,
 * and the pane says "verified 8/9" rather than calling the corpus broken.
 */

/** Where a diagnostic's file path is relative to, for display. */
export const displayPath = (f) => String(f || '').replace(/^\.?\/?project\//, '').replace(/^\.\//, '');

/** The counts a report carries, or null when there is no report yet. */
/* THE REPORT HAS ONE READER (js/app/diagnostics.js::counts), imported under this pane's own
   name: the tile and the ribbon chip show the same numbers because they ask the same function.
   The pane used to carry its own copy of this arithmetic, and the two had to agree about the
   same JSON — including about `admitted`, which the copy did not read at all. */
import { counts as readCounts } from '../app/diagnostics.js';

/* re-exported for the pane's own callers (`shell`/`instances` mount it by this name, and its unit
   test reads it): a re-export does not bind the name locally, and this pane calls it too. */
export { readCounts };

/**
 * Every diagnostic as one flat list, in report order: failures first (each carrying its
 * file), then the warnings. A report with neither is a clean corpus, not a missing report.
 */
export function readRows(report) {
  const out = [];
  for (const f of ((report && report.failures) || [])) {
    for (const d of (f.diagnostics || [])) out.push(Object.assign({ file: f.path }, d));
  }
  for (const d of ((report && report.warnings) || [])) {
    out.push(Object.assign({ file: (d.span && d.span.file) || null }, d));
  }
  return out;
}

/** One line of the report's own summary: what the tile shows at the top. */
export function summarise({ counts, error, busy }) {
  if (busy) return 'checking the corpus…';
  if (error) return 'check failed: ' + error;
  if (!counts) return 'never checked';
  return counts.verified + '/' + counts.files + ' verified'
    + (counts.errors ? ' · ' + counts.errors + ' errors' : '')
    + (counts.warnings ? ' · ' + counts.warnings + ' warnings' : '')
    + (counts.admitted ? ' · ' + counts.admitted + ' admitted' : '');
}

/** The title-bar text for a report: errors, then warnings, then clean. */
export function badge(counts) {
  if (!counts) return 'not checked';
  if (counts.errors) return counts.errors + ' errors';
  if (counts.warnings) return counts.warnings + ' warnings';
  return 'clean';
}

import { valuePort, eventPort, callPort } from '../core/ports.js';

export default {
  kind: 'diag',
  title: 'Diagnostics',
  ports: {
    in: { diagnostics: 'value' },
    out: { jump: 'event' },
    call: { rerun: 'call', check: 'call' },
  },
  create(ctx) {
    const esc = (ctx.ui && ctx.ui.esc) || ((s) => String(s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c])));

    let el = null;
    /* A report can arrive two ways: the app's own check (`ctx.check`), or a WIRE — a
       program node that built a report can feed one in. The wired report wins, because a
       pane that is being driven should show what it was driven with. */
    let wired = null;
    const inDiagnostics = valuePort(null);
    const outJump = eventPort();
    inDiagnostics.subscribe((v) => { wired = v || null; paint(); });
    const state = () => {
      const s = ctx.check ? ctx.check() : { busy: false, report: null, error: null, at: 0, took: 0 };
      return wired ? { busy: false, report: wired, error: null, at: s.at, took: s.took } : s;
    };

    function paint(p) {
      if (!el) return;
      const s = state();
      const rows = readRows(s.report);
      const counts = readCounts(s.report);

      el.textContent = '';
      const head = document.createElement('div');
      head.className = 'diag-head';
      const what = document.createElement('span');
      what.innerHTML = summarise({ counts, error: s.error, busy: s.busy });
      const took = document.createElement('i');
      took.textContent = s.at && ctx.durText ? ctx.durText(s.took) : '';
      const btn = document.createElement('button');
      btn.textContent = s.busy ? 'running…' : 'check corpus';
      btn.disabled = !!s.busy;
      btn.addEventListener('click', (e) => {
        e.stopPropagation();
        if (ctx.runCheck) ctx.runCheck();
      });
      head.append(what, took, btn);
      el.appendChild(head);

      if (!rows.length) {
        const empty = document.createElement('div');
        empty.className = 'diag-empty';
        empty.textContent = counts
          ? 'Nothing to report: every file type-checked and the corpus’s LaTeX is well formed.'
          : 'Press “check corpus”. This runs the same build the CLI runs, so it takes about a minute and reports errors and warnings with their file and line.';
        el.appendChild(empty);
      } else {
        const list = document.createElement('div');
        list.className = 'diag-list';
        for (const d of rows) {
          const sev = d.severity || 'error';
          const row = document.createElement('div');
          row.className = 'diag-row ' + (sev === 'error' ? 'error' : 'warning');
          const loc = displayPath(d.file) + ((d.span && d.span.line) ? ':' + d.span.line : '');
          const where = (d.span && d.span.lean_line) ? ' (lean:' + d.span.lean_line + ')' : '';
          row.innerHTML = '<span class="sev">' + (sev === 'error' ? '✗' : '⚠') + '</span><span>'
            + (d.file ? '<span class="loc">' + esc(loc || '—') + '</span>' + where + ' · ' : '')
            + '<span class="code">' + esc(d.code || '') + '</span> · <span class="msg">' + esc(d.message || '') + '</span></span>'
            + ((d.hints && d.hints.length) ? '<span class="hint">' + esc(d.hints.join(' · ')) + '</span>' : '');
          if (d.file) {
            row.addEventListener('click', () => {
              // the jump is published as well as acted on, so a wire can follow the reader
              outJump.emit({ path: displayPath(d.file), line: (d.span && d.span.line) || null });
              if (ctx.openFile) ctx.openFile(displayPath(d.file));
            });
          }
          list.appendChild(row);
        }
        el.appendChild(list);
      }
      if (p) {
        p.summary = () => badge(counts);
        if (p._summary && p.card && p.card.lastChild) p.card.lastChild.textContent = p.summary();
      }
    }

    return {
      mount(host, p) {
        el = document.createElement('div');
        el.className = 'diag';
        host.appendChild(el);
        paint(p);
      },
      unmount() {
        if (el && el.parentNode) el.parentNode.removeChild(el);
        el = null;
      },
      refresh: () => paint(),
      in: { diagnostics: inDiagnostics },
      out: { jump: outJump },
      call: {
        check: () => { if (ctx.runCheck) ctx.runCheck(); return { ok: true }; },
        rerun: () => { if (ctx.runCheck) ctx.runCheck(); return { ok: true }; },
      },
      get ports() { return { in: this.in, out: this.out, call: this.call }; },
    };
  },
};