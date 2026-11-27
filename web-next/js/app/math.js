/*
 * math.js — how a `.qbit` line's TEXT becomes markup.
 *
 * Extracted from the inline script in web-next/index.html, next to qbit-lines.js:
 * that module says what a line IS (header, label, prose, math row), this one says
 * how to draw it. Split because the two fail differently — a misclassified line is
 * a reasoning bug with a unit test, a KaTeX failure is a rendering bug.
 *
 * It also owns `nameHtml`, because the same question comes up outside the render tile: a project's
 * name is authored data and the charter writes its own as TeX, so the tiles that PRINT a name have
 * to be able to draw one (js/panes/project.js, corpus.js, projects.js).
 *
 * The rule that matters: **a span KaTeX cannot read is shown as code, never dropped.**
 * A mistyped glyph has to stay visible in the render tile, because that is where the
 * reader notices it (the engine warns about the same glyph separately, from
 * crates/build/src/latex.rs). Hence `<span class="lraw">` for a display formula that
 * throws, and `<code>` for an inline span that throws.
 *
 * KaTeX itself is loaded from /vendor on first use, not at boot: the page must be
 * able to draw its tiles before the typesetting library arrives, and `loadKatex()`
 * returns the same promise to everyone who asks.
 */
/* Relative, not the page-absolute `/js/app/util.js`: the same file has to resolve
   under `node --test` as well as in the browser, and a leading slash only means
   something to the server. Browsers resolve this one just as well. */
import { esc } from './util.js';

let katexPromise = null;

/* Every read of the typesetter goes through here, never a bare `window`. A node-only check imports
   this module to assert the text-to-markup rules, where `window` exists only if something put it
   there; a bare `window.katex` would be a ReferenceError and the rule would be uncheckable. */
const katexLib = () => (typeof window === 'undefined' ? null : window.katex || null);

/** Resolve to `window.katex`, loading the stylesheet + script once. null on failure. */
export function loadKatex() {
  const here = katexLib();
  if (here) return Promise.resolve(here);
  /* A context with no document to put a <script> in (a node test, a static scan) can never
     receive the library. Say so, rather than throw from a mount: the caller's raw-source path is
     the honest answer there, and it is the same path a failed download takes. */
  if (typeof document === 'undefined' || !document.head) return Promise.resolve(null);
  if (katexPromise) return katexPromise;
  katexPromise = new Promise((res) => {
    if (!document.querySelector('link[href="/vendor/katex.min.css"]')) {
      const l = document.createElement('link'); l.rel = 'stylesheet'; l.href = '/vendor/katex.min.css'; document.head.appendChild(l);
    }
    const s = document.createElement('script'); s.src = '/vendor/katex.min.js';
    s.onload = () => res(katexLib()); s.onerror = () => { katexPromise = null; res(null); }; document.head.appendChild(s);
  });
  return katexPromise;
}

/** Run `fn` when the typesetter is in hand — or when it turned out not to be.
 *
 *  A formula drawn from its source is worth more than a tile that never draws, so a pane that
 *  typesets on the side calls this from `mount` instead of awaiting inside its paint: the first
 *  frame comes from the raw-source fallback and the repaint lands when the library does. */
export function whenKatex(fn) { loadKatex().then(fn, fn); }

/** A whole formula. Display mode is lenient: a bad glyph must not blank the tile. */
export function mathHtml(tex, display) {
  const katex = katexLib();
  if (katex) {
    try { return katex.renderToString(tex, { displayMode: !!display, throwOnError: false, strict: false }); }
    catch (e) { return '<span class="lraw">' + esc(tex) + '</span>'; }
  }
  return '<span class="lraw">' + esc(tex) + '</span>';
}

/** A NAME that may be typeset — which is most of what the project and corpus tiles print.
 *
 *  A project's name is authored data, and the charter writes its own as TeX
 *  (`P. Qubitick — \text{The} \mathfrak{Q} \text{Corpus}`), so a readout that escapes it shows
 *  the reader markup. But only a name that CARRIES TeX goes to KaTeX: a plain directory name's
 *  underscore would come back as a subscript, and `solver_2` rendered as `solver₂` is a wrong
 *  name, not a rendering detail. Everything else is escaped text, exactly as before.
 */
export function nameHtml(name, display) {
  const s = String(name === null || name === undefined ? '' : name);
  return carriesTex(s) ? mathHtml(s, !!display) : esc(s);
}

/** Does authored text CARRY TeX? The one predicate behind `nameHtml` and `citeHtml`: both take
 *  text the corpus wrote and decide whether it is mark-up for the typesetter or a name to print
 *  as typed. (`$` counts — a span may be delimited rather than commanded.) */
const carriesTex = (s) => /[\\$]/.test(s);

/** A CITATION label, as the render tile prints it beside the row it justifies.
 *
 *  Two kinds live in one bracket. `A1.1`, `T.1` and the induction hypothesis `^i` are NAMES: they
 *  are printed as typed, and `^i` must not go to KaTeX at all (a bare superscript is an error, and
 *  a lenient KaTeX would show its complaint inside the tile). `\ulcornerA1.1 b b a\urcorner` and
 *  `\equiv` are TeX — the corner quotes around an instantiated law are how the author wrote the
 *  citation, so they are typeset instead of being shown as backslashes. */
export function citeHtml(label, display) {
  const s = String(label === null || label === undefined ? '' : label);
  return carriesTex(s) ? mathHtml(citeTex(s), !!display) : esc(s);
}

/** The corpus writes an instantiated, reversed citation as `\ulcornerA1.4 a\urcorner` — the command
 *  GLUED to the name. That is a textual convention the engine reads by string prefix
 *  (`crates/parse/src/node.rs`, `strip_prefix("\\ulcorner")`), and it is NOT LaTeX: a command name
 *  is a run of letters, so `\ulcornerA1` is an unknown command and KaTeX renders the backslashes
 *  with an error node — the label came out as `\ulcornerA1.4a┐`. Spaced, with the name in text
 *  mode, it typesets as the corner quotes the author meant. */
function citeTex(label) {
  /* ONE LABEL CAN CITE SEVERAL LAWS — `[\ulcornerA1.4 a\urcorner, A1.1]` is the two-citation row
     that expands a nullity — so every item is normalised, not just a label that is one tag. */
  return String(label)
    .split(',')
    .map((item) => {
      const t = item.trim();
      const m = /^\\ulcorner([\s\S]*)\\urcorner$/.exec(t);
      /* SPACED, and the name in TEXT mode. `\ulcornerA1.4` glued is an unknown command
         (`\ulcornerA1`), and `\text{}` around the corner CHARACTERS is rejected by this KaTeX
         ("Can't use function '\mathop' in text mode") — so the commands spell the corners and only
         the name is text. */
      return m ? '\\ulcorner\\text{' + m[1].trim() + '}\\urcorner' : t;
    })
    .filter((t) => t !== '')
    .join(',\\ ');
}




/* prose, where `a backticked span` is inline mathematics. KaTeX runs in strict mode
   here (throwOnError) precisely so the catch is reachable and the span falls through
   to code. */
export function proseHtml(text) {
  return String(text).split(/(`[^`]*`)/).map(seg => {
    const m = /^`([\s\S]*)`$/.exec(seg);
    if (!m) return esc(seg);
    const tex = m[1];
    const katex = katexLib();
    if (katex) {
      try { return katex.renderToString(tex, { displayMode: false, throwOnError: true, strict: false }); }
      catch (e) { /* falls through to code */ }
    }
    return '<code>' + esc(tex) + '</code>';
  }).join('');
}
