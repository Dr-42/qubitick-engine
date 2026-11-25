/*
 * app/diagnostics.js — the corpus check: one report, two faces, and a rule about when it runs.
 *
 * Extracted from the inline script in web-next/index.html. A corpus check is a WHOLE-CORPUS
 * build: the server walks every file and it takes about a minute. Three consequences, and all
 * three are why this is a module rather than a few lines in a pane:
 *
 *   NEVER START ONE BY LOOKING. Opening the diagnostics tile must not begin a minute of work;
 *   the rail's chip and the tile's own button start it, and both read the same stored report.
 *   (A panel that starts a build because you glanced at it is a trap, and this module is
 *   where that promise is kept.)
 *
 *   ONE RUN AT A TIME. Two clicks are two builds unless they share the run — the second
 *   caller gets the first one's promise, so the report cannot arrive out of order.
 *
 *   THE REPORT IS THE SERVER'S. `verified`, `failures[]` (each file's own error diagnostics)
 *   and `warnings[]` (each carrying its own file and line) are the shape the CLI prints. This
 *   module flattens them into the rows a tile and a chip can both read, and derives the one
 *   line the chip shows — including the case where a build FAILED, which is a different
 *   answer from a build that found errors.
 */

/** The no-op report: what "never checked" looks like. */
export const emptyReport = () => ({ busy: false, at: 0, took: 0, report: null, error: null, diagnostics: [] });

/** A path as a diagnostic names it, without the corpus prefix the server walks. */
export const diagPath = (f) => String(f || '').replace(/^\.?\/?project\//, '').replace(/^\.\//, '');

/** The numbers the chip shows, or null when nothing has been checked.
 *
 * THE ONE READER OF THIS REPORT. `js/panes/diag.js` had its own copy of this arithmetic and the two
 * had to agree about the same JSON; they now cannot disagree, because there is one function and the
 * pane imports it.
 *
 * `admitted` IS READ, AND THAT IS THE POINT. The engine reports how much of the corpus it ASSUMED
 * (`verified: 329, admitted: 2` on the shipped corpus) and says so per node in a `proof.admitted`
 * warning. A reader that shows `verified/files` and stops reports a corpus with unproved theorems
 * as clean — which is exactly the reading the engine's own report exists to prevent
 * (crates/build/src/pipeline.rs::admitted_diagnostics). */
export function counts(report) {
  if (!report) return null;
  return {
    files: report.files || 0,
    verified: report.verified || 0,
    admitted: report.admitted || 0,
    errors: report.error_count || 0,
    warnings: report.warning_count || 0,
  };
}

/** `· 2 admitted`, or nothing when the corpus assumed nothing. */
const admittedText = (c) => (c.admitted ? ' · ' + c.admitted + ' admitted' : '');

/**
 * Every diagnostic as one row: an error carries the file it failed in, a warning carries its
 * own span. Both get `file` so a reader can jump.
 */
export function rows(report) {
  const out = [];
  for (const f of (report && report.failures) || []) {
    for (const d of f.diagnostics || []) out.push(Object.assign({ file: f.path }, d));
  }
  for (const d of (report && report.warnings) || []) {
    out.push(Object.assign({ file: (d.span && d.span.file) || null }, d));
  }
  return out;
}

/**
 * What the rail's chip says, as data — so the word and the colour are decided in one place
 * and a test can read them without a DOM.
 * @returns {{ text:string, tone:'idle'|'busy'|'bad'|'warn'|'good', title:string }}
 */
export function chip(state, { dur = durText } = {}) {
  if (state.busy) return { text: 'checking...', tone: 'busy', title: 'a whole-corpus check is running' };
  if (state.error) return { text: 'check failed', tone: 'bad', title: String(state.error) };
  const c = counts(state.report);
  if (!c) return { text: 'not checked', tone: 'idle', title: 'check the whole corpus' };
  const w = c.warnings ? ' · ' + c.warnings + (c.warnings === 1 ? ' warning' : ' warnings') : '';
  const took = state.took ? dur(state.took) : null;
  return {
    text: c.verified + '/' + c.files + ' verified' + w + admittedText(c),
    /* a corpus that assumes things is not 'good', whatever its warning count says */
    tone: c.errors ? 'bad' : (c.warnings || c.admitted) ? 'warn' : 'good',
    title: took ? 'last check ' + took + ' · click to re-run' : 'click to re-run',
  };
}

/** The one line a toast carries when a check finishes. */
export function summary(state) {
  if (state.error) return 'check: ' + state.error;
  const c = counts(state.report);
  if (!c) return 'check: nothing to report';
  return 'check: ' + c.verified + ' of ' + c.files + ' verified'
    + (c.errors ? ', ' + c.errors + ' errors' : '')
    + (c.admitted ? ', ' + c.admitted + ' admitted' : '');
}

/** A duration in words, the way the chip says it. */
function durText(ms) {
  const s = Math.round((ms || 0) / 100) / 10;
  return s < 60 ? s + 's' : Math.round(s / 60) + 'm';
}

/**
 * @param {object} host
 * @param {(name:string) => string} host.url
 * @param {(u:string, init:object) => Promise<object>} host.fetch  a fetch that resolves to
 *        `{ ok, status, json() }`
 * @param {() => void} [host.paint]     the chip is repainted, and every tile that shows it
 * @param {(m:string) => void} [host.toast]
 * @param {() => number} [host.now]     the clock, for the duration
 */
export function createDiagnostics(host) {
  const now = host.now || (() => (typeof performance !== 'undefined' ? performance.now() : Date.now()));
  const state = emptyReport();
  let run = null;
  const paint = () => { if (host.paint) host.paint(); };

  /** Run a whole-corpus check. Concurrent calls share the run. */
  function check() {
    if (run) return run;
    state.busy = true;
    state.error = null;
    paint();
    const t0 = now();
    run = (async () => {
      try {
        const r = await host.fetch(host.url('build.post'), {
          method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{}',
        });
        /* the body may not be JSON at all (a proxy's 502, an older server): the status is
           still the honest answer, so a parse failure is an empty report rather than a throw */
        const j = (r.json ? await r.json().catch(() => ({})) : {}) || {};
        if (!r.ok) {
          state.error = j.error || (j.diagnostics && j.diagnostics[0] && j.diagnostics[0].message) || ('HTTP ' + r.status);
          state.diagnostics = j.diagnostics || [];
        } else {
          state.report = j;
          state.diagnostics = [];
        }
      } catch (e) {
        state.error = e.message;
      }
      state.busy = false;
      state.at = Date.now();
      state.took = Math.round(now() - t0);
      paint();
      if (host.toast) host.toast(summary(state));
      run = null;
      return state.report;
    })();
    return run;
  }

  return {
    state,
    check,
    counts: () => counts(state.report),
    rows: () => rows(state.report),
    chip: () => chip(state),
    summary: () => summary(state),
    busy: () => state.busy,
  };
}