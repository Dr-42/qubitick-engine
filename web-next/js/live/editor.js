/**
 * live/editor.js — the .qbit editor surface, inside a world tile.
 *
 * CodeMirror 6 (bundled at /vendor/cm.bundle.js) with a purpose-built
 * tokenizer for the .qbit surface: keywords, \macros, comments, strings,
 * and the logic glyphs. Line numbers, active line, bracket matching,
 * search, history. Falls back to a plain textarea if the bundle is absent.
 */

let CM = null;
import { citeTag } from './proof-row.js';

export async function loadCM() {
  if (CM) return CM;
  CM = await import('/vendor/cm.bundle.js');
  return CM;
}

/* ---------- the language -------------------------------------------- */

const KEYWORDS = new Set([
  'class', 'namespace', 'export', 'notation', 'infix', 'prefix', 'postfix',
  'axiom', 'theorem', 'def', 'definition', 'Proof', 'proof', 'qed', 'where',
  'extends', 'import', 'end', 'let', 'in', 'if', 'then', 'else', 'match',
  'with', 'instance', 'structure', 'sort', 'universe', 'variable', 'sorry',
]);

const GLYPH = /^(∀|∃|⊢|⊣|↦|→|←|↔|⇒|⇔|¬|∧|∨|Δ|∩|∪|⊆|⊂|⊇|∅|λ|Σ|Π|∘|⊗|○|◆|◇|∈|∉|=|≠|≤|≥)/;

export function qbitLanguage(cm) {
  const { StreamLanguage } = cm;
  return StreamLanguage.define({
    name: 'qbit',
    token(stream) {
      if (stream.eatSpace()) return null;
      if (stream.match(/^(--|\/\/).*/)) return 'comment';
      if (stream.match(/^\\[A-Za-z][A-Za-z0-9]*/)) return 'macroName';
      if (stream.match(/^"[^"]*"?/)) return 'string';
      if (stream.match(/^\d+(\.\d+)?/)) return 'number';
      if (stream.match(/^(:=|::|->|<-|=>|\|=)/)) return 'operator';
      if (stream.match(GLYPH)) return 'operator';
      if (stream.match(/^[A-Za-z_][A-Za-z0-9_']*/)) {
        const w = stream.current();
        if (KEYWORDS.has(w)) return 'keyword';
        if (/^[A-Z]/.test(w)) return 'typeName';
        if (/^[A-Za-z]$/.test(w)) return 'variableName';
        return 'propertyName';
      }
      if (stream.match(/^[()\[\]{},.:;]/)) return 'punctuation';
      stream.next();
      return null;
    },
  });
}

function qbitHighlight(cm) {
  const { HighlightStyle, syntaxHighlighting, tags: t } = cm;
  return syntaxHighlighting(
    HighlightStyle.define([
      { tag: t.keyword, color: 'var(--violet)', fontWeight: '600' },
      { tag: t.macroName, color: 'var(--cyan)' },
      { tag: t.comment, color: 'var(--t3)', fontStyle: 'italic' },
      { tag: t.string, color: 'var(--green)' },
      { tag: t.number, color: 'var(--amber)' },
      { tag: t.typeName, color: 'var(--ink)', fontWeight: '600' },
      { tag: t.variableName, color: 'var(--t1)' },
      { tag: t.propertyName, color: 'color-mix(in oklab, var(--ink) 78%, var(--panel))' },
      { tag: t.operator, color: 'var(--violet)' },
      { tag: t.punctuation, color: 'var(--t3)' },
    ]),
  );
}

function qbitTheme(cm) {
  const { EditorView } = cm;
  return EditorView.theme(
    {
      '&': { height: '100%', backgroundColor: 'transparent', color: 'var(--t1)' },
      '&.cm-focused': { outline: 'none' },
      '.cm-scroller': {
        fontFamily: 'var(--font-mono)',
        // set by the shell's editor preferences (see --ed-font / --ed-lh)
        fontSize: 'var(--ed-font, 13px)',
        lineHeight: 'var(--ed-lh, 1.75)',
        overflow: 'auto',
      },
      '.cm-content': { padding: '14px 0 40px', caretColor: 'var(--cyan)' },
      '.cm-line': { padding: '0 18px 0 4px' },
      '.cm-gutters': {
        backgroundColor: 'transparent',
        border: 'none',
        color: 'var(--t3)',
        paddingLeft: '6px',
      },
      '.cm-lineNumbers .cm-gutterElement': {
        padding: '0 12px 0 6px',
        minWidth: '38px',
        fontVariantNumeric: 'tabular-nums',
        // a step below the body, in step with it
        fontSize: 'calc(var(--ed-font, 13px) * 0.885)',
      },
      '.cm-activeLine': { backgroundColor: 'color-mix(in oklab, var(--ink) 4%, transparent)' },
      '.cm-activeLineGutter': { backgroundColor: 'transparent', color: 'var(--cyan)' },
      '.cm-selectionBackground, ::selection': {
        backgroundColor: 'color-mix(in oklab, var(--cyan) 24%, transparent) !important',
      },
      '&.cm-focused .cm-selectionBackground': {
        backgroundColor: 'color-mix(in oklab, var(--cyan) 26%, transparent) !important',
      },
      '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--cyan)', borderLeftWidth: '1.5px' },
      '.cm-matchingBracket': {
        backgroundColor: 'color-mix(in oklab, var(--cyan) 18%, transparent)',
        outline: '1px solid color-mix(in oklab, var(--cyan) 45%, transparent)',
      },
      '.cm-foldGutter .cm-gutterElement': { color: 'var(--t3)' },
      '.cm-foldGutter .cm-gutterElement:hover': { color: 'var(--cyan)' },
      '.cm-panels': {
        backgroundColor: 'color-mix(in oklab, var(--panel) 92%, var(--bg))',
        color: 'var(--t1)',
        border: 'none',
      },
      '.cm-searchMatch': { backgroundColor: 'color-mix(in oklab, var(--amber) 22%, transparent)' },
    },
    { dark: true },
  );
}

/* ---------- mount ---------------------------------------------------- */

/**
 * CITATION LINKS: a `[A1.4]` in a proof row is a reference, so it looks like one and clicking it
 * goes there.
 *
 * The reading of what a tag IS comes from `proof-row.js` (`citeTag`), which is the same reading the
 * row's own sign and tag column come from — one implementation, so the text the editor marks as
 * clickable is exactly the text the corpus treats as a citation.
 *
 * WHAT IS NOT HERE: a hover tooltip. The vendored CodeMirror exports `Decoration` and `ViewPlugin`
 * but not `hoverTooltip` (`tools/build-cm.mjs`), so the hover affordance is the element's `title`,
 * which can only carry what is known synchronously — the citation. Resolving it (which node, which
 * statement, which file) is an engine call, and that is the CLICK.
 *
 * @param {object} cm the loaded CodeMirror namespace
 * @param {(cite:string) => void} onCite
 */
function citeLinks(cm, onCite) {
  const { Decoration, ViewPlugin, EditorView } = cm;
  /* ONE DECORATION PER TAG, not one shared mark: `mark(spec)` fixes the DOM attributes at
     construction, and `range(from, to, value)`'s third argument is the range's VALUE, not a spec —
     so a shared `Decoration.mark({class})` marks the text and carries no `data-cite`, which is
     exactly what the click handler reads. A corpus file has a handful of tags; one object each is
     nothing. */
  const build = (view) => {
    const out = [];
    for (let n = 1; n <= view.state.doc.lines; n++) {
      const line = view.state.doc.line(n);
      const tag = citeTag(line.text);
      if (!tag) continue;
      out.push(
        Decoration.mark({
          class: 'cm-cite',
          attributes: { 'data-cite': tag.cite, title: tag.cite + ' — click to open what it names' },
        }).range(line.from + tag.from, line.from + tag.to),
      );
    }
    return out.length ? Decoration.set(out, true) : Decoration.none;
  };
  return [
    ViewPlugin.fromClass(
      class {
        constructor(view) { this.decorations = build(view); }
        update(u) { if (u.docChanged || u.viewportChanged) this.decorations = build(u.view); }
      },
      { decorations: (v) => v.decorations },
    ),
    /* ON THE PRESS, NOT ON THE CLICK. A click never arrives at these elements: CodeMirror
       re-renders the line when the mousedown moves the caret (the active-line highlight), so the
       element that was pressed is detached before the click is dispatched and the click lands on
       the content element instead — which is measured behaviour, not a guess: a synthetic `click`
       dispatched at the tag works, and a real mouse click did nothing. A reference in a text
       surface is therefore followed on the press. */
    EditorView.domEventHandlers({
      mousedown: (e) => {
        if (e.button !== 0) return false;
        const el = e.target && e.target.closest ? e.target.closest('.cm-cite') : null;
        if (!el) return false;
        const cite = el.getAttribute('data-cite');
        if (!cite) return false;
        onCite(cite);
        return true;
      },
    }),
  ];
}

export async function mountEditor(host, opts = {}) {
  const cm = await loadCM();
  const {
    EditorView, EditorState, lineNumbers, highlightActiveLineGutter, highlightActiveLine,
    drawSelection, highlightSpecialChars, history, historyKeymap, undo, redo, defaultKeymap,
    indentWithTab, bracketMatching, foldGutter, foldKeymap, keymap, searchKeymap,
    highlightSelectionMatches, placeholder, Annotation,
  } = cm;

  const { Compartment } = cm;
  // wrapping and tab width are user settings, so they live in compartments the
  // shell can reconfigure without tearing the editor down
  const wrapComp = new Compartment();
  const tabComp = new Compartment();
  /* links are switchable: a read-only view of a module has no corpus to point at */
  const citeComp = new Compartment();
  // token colours are switchable: the language stays (folding, brackets, search),
  // only the palette goes
  const synComp = new Compartment();
  const wantSyntax = opts.syntax !== false;
  // a read-only view: for looking at a module rather than editing it
  const readOnly = opts.readOnly === true;
  const wantWrap = opts.wrap !== false;
  const tabSize = Number(opts.tabSize) || 2;

  /* Several views may show one document (the shell's DOCS registry). CodeMirror's
   * own advice for that is a shared dispatch path: forward the changes made in one
   * view to its siblings, tagged so a forwarded change is never forwarded on. Only
   * the first view of a document keeps a history; the others hand their undo keys
   * to it, so the document has exactly one undo history however many views it has.
   * Selection, cursor and scroll are per view and are deliberately not forwarded. */
  const syncAnnotation = Annotation.define();

  const extensions = [
    lineNumbers(),
    highlightActiveLineGutter(),
    highlightActiveLine(),
    highlightSpecialChars(),
    drawSelection(),
    history(),
    bracketMatching(),
    foldGutter(),
    highlightSelectionMatches(),
    qbitLanguage(cm),
    synComp.of(wantSyntax ? qbitHighlight(cm) : []),
    qbitTheme(cm),
    citeComp.of(opts.onCite ? citeLinks(cm, opts.onCite) : []),
    keymap.of([
      /* ENTER CAN CONVERT THE LINE. The pane decides (`opts.onEnter`): it claims the key when the
         line under the caret is a sentence with a statement above it, and only then does the
         citation search run. The decision has to be synchronous — a handler that awaited the
         engine would let the newline through first, and the row would be written under it. */
      /* ENTER IS THE CITATION, `Mod+Enter` IS THE INDUCTION. Both are offered only where an
         edit belongs (a read-only view is for reading), and both decide synchronously whether
         the key is theirs: the citation search runs after the claim, and the induction block is
         written from what is already in the document. */
      ...(readOnly ? [] : [
        { key: 'Enter', run: (v) => !!(opts.onEnter && opts.onEnter(v)) },
        { key: 'Mod-Enter', run: (v) => !!(opts.onInduce && opts.onInduce(v)) },
      ]),
      ...defaultKeymap,
      // the shell gets first refusal on undo/redo so a document keeps one history,
      // and this view's own history answers only if the document has no owner
      { key: 'Mod-z', run: () => (opts.onUndo && opts.onUndo()) || undo(view || undefined) || true },
      { key: 'Mod-y', mac: 'Mod-Shift-z', run: () => (opts.onRedo && opts.onRedo()) || redo(view || undefined) || true },
      ...historyKeymap,
      ...foldKeymap, ...searchKeymap, indentWithTab,
    ]),
    wrapComp.of(wantWrap ? EditorView.lineWrapping : []),
    tabComp.of(EditorState.tabSize.of(tabSize)),
  ];
  if (opts.placeholder) extensions.push(placeholder(opts.placeholder));
  if (readOnly) extensions.push(EditorState.readOnly.of(true), EditorView.editable.of(false));
  extensions.push(
    EditorView.updateListener.of((u) => {
      const remote = u.transactions.some((t) => t.annotation(syncAnnotation) === true);
      if (u.docChanged && opts.onChange) opts.onChange(u.state.doc.toString(), u.changes, remote);
      if (opts.onCursor && (u.selectionSet || u.docChanged)) {
        const h = u.state.selection.main.head;
        const line = u.state.doc.lineAt(h);
        opts.onCursor({ line: line.number, col: h - line.from + 1 }, remote);
      }
    }),
  );

  const view = new EditorView({
    state: EditorState.create({ doc: opts.doc || '', extensions }),
    parent: host,
  });

  return {
    view,
    getDoc: () => view.state.doc.toString(),
    setDoc(text) {
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } });
    },
    /** Apply changes made in a sibling view. Selection and scroll stay put: the
     *  range is mapped through the change rather than reset. */
    applyRemote(changes) {
      view.dispatch({ changes, annotations: syncAnnotation.of(true) });
    },
    /** Swap in a whole document (the tile is being retargeted). This is not an
     *  edit: it is tagged as sync so it is never broadcast as a user change and
     *  never marks the document dirty. */
    swapDoc(text) {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: text || '' },
        annotations: syncAnnotation.of(true),
      });
    },
    undo: () => undo(view),
    redo: () => redo(view),
    /** Retune wrapping / tab width in place (font size is a CSS variable). */
    setOpts(next = {}) {
      const effects = [];
      if (next.wrap !== undefined) {
        effects.push(wrapComp.reconfigure(next.wrap ? EditorView.lineWrapping : []));
      }
      if (next.tabSize !== undefined) {
        effects.push(tabComp.reconfigure(EditorState.tabSize.of(Number(next.tabSize) || 2)));
      }
      if (next.syntax !== undefined) {
        effects.push(synComp.reconfigure(next.syntax ? qbitHighlight(cm) : []));
      }
      if (effects.length) view.dispatch({ effects });
    },
    focus: () => view.focus(),
    destroy: () => view.destroy(),
  };
}

/** plain textarea fallback — same API, no library */
export function mountFallback(host, opts = {}) {
  const ta = document.createElement('textarea');
  ta.className = 'ed';
  ta.spellcheck = false;
  if (opts.wrap === false) { ta.style.whiteSpace = 'pre'; ta.style.overflowX = 'auto'; }
  ta.value = opts.doc || '';
  ta.placeholder = opts.placeholder || '';
  ta.addEventListener('input', () => opts.onChange && opts.onChange(ta.value));
  ta.addEventListener('keyup', () => {
    if (!opts.onCursor) return;
    const head = ta.value.slice(0, ta.selectionStart).split('\n');
    opts.onCursor({ line: head.length, col: head[head.length - 1].length + 1 });
  });
  host.append(ta);
  return {
    view: null,
    getDoc: () => ta.value,
    setDoc(t) { ta.value = t; },
    // the shell syncs a textarea view wholesale; it has no ranges to map
    applyRemote() {},
    swapDoc(t) { ta.value = t; },
    undo: () => document.execCommand && document.execCommand('undo'),
    redo: () => document.execCommand && document.execCommand('redo'),
    setOpts(next = {}) {
      if (next.wrap !== undefined) {
        ta.style.whiteSpace = next.wrap ? '' : 'pre';
        ta.style.overflowX = next.wrap ? '' : 'auto';
      }
    },
    focus: () => ta.focus(),
    destroy: () => ta.remove(),
  };
}