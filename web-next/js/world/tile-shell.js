/*
 * tile-shell.js — the markup of a tile: its header, its body, its card.
 *
 * Extracted from the inline script in web-next/index.html. A tile is three elements in a
 * fixed order, and the header is a small table of which ACTIONS a kind gets. That table is
 * the interesting part, because it is where a pane's affordances are decided:
 *
 *   ◎ pin     only a kind that shows corpus text can be pinned to one file
 *   ⌖ link    only a kind that reads the corpus can keep its selection to itself
 *   ‹/› src   every kind: "edit source" opens the module that builds it
 *   ✕ del     every kind except the ones in `noClose` — the workspace's own control
 *             surface has to survive its own ✕
 *
 * Keeping the decision here (a small table, checked by a test) means a kind cannot
 * accidentally advertise an action that does nothing, and it is why `mkPane` below the
 * host is a chain of `if`s rather than a second copy of this markup.
 *
 * The functions return markup and elements, never state: `body` and `card` are handed back
 * for the caller to keep, and nothing here reads the app.
 */

/** The actions every tile has, in the order they appear. */
const ALWAYS = ['fit', 'focus', 'full', 'src'];
const TITLES = {
  pin: 'pin this tile to its own file',
  link: 'link this tile’s selection',
  fit: 'fit: frame this tile',
  focus: 'focus: centre the camera here',
  full: 'fullscreen: this tile fills the viewport',
  src: 'edit source: open the file this tile shows',
  del: 'close this tile',
};
const GLYPHS = {
  pin: '◎', link: '⌖', fit: '⤢', focus: '⊙', full: '⛶', src: '‹/›', del: '✕',
};

/**
 * Which actions a kind gets, in order.
 * @param {object} o  { kind, docs, links, noClose }
 */
export function actionsFor({ kind, docs = false, links = false, noClose = new Set() } = {}) {
  const out = [];
  if (docs) out.push('pin');
  if (links) out.push('link');
  // a separator after the contextual actions, so they read as a group
  out.push(...ALWAYS);
  if (!noClose.has(kind)) out.push('del');
  return out;
}

/** One action button, with the glyph and the sentence a hover shows. */
export function actionButton(action, doc = document) {
  const b = doc.createElement('button');
  b.dataset.a = action;
  b.type = 'button';
  b.title = TITLES[action] || '';
  // the source button is the two-glyph ‹/›, which is markup rather than a character
  if (action === 'src') b.innerHTML = '&#8249;/&#8250;';
  else b.textContent = GLYPHS[action] || action;
  return b;
}

/**
 * Build a tile's three elements.
 * @returns {{ el, head, body, card }}
 */
export function buildShell({ id, kind, title, actions, doc = document }) {
  const el = doc.createElement('div');
  el.className = 'pane lod-live';
  el.dataset.kind = kind;
  el.dataset.pane = id;

  const head = doc.createElement('div');
  head.className = 'pane-head';
  const name = doc.createElement('span');
  name.textContent = title;
  const group = doc.createElement('span');
  group.className = 'actions';
  /* A rule opens each group: the camera actions, then the tool actions. The first rule is
     always there — that is the shipped look, and it is what keeps the header reading as
     [contextual] | [camera] | [tool] rather than as one long row. */
  for (const a of actions) {
    if (a === 'fit' || a === 'src') group.appendChild(sep(doc));
    group.appendChild(actionButton(a, doc));
  }
  const lod = doc.createElement('span');
  lod.className = 'lod';
  head.append(name, group, lod);

  const body = doc.createElement('div');
  body.className = 'pane-body';
  const card = doc.createElement('div');
  card.className = 'pane-card';
  const b = doc.createElement('b');
  b.textContent = title;
  card.append(b, doc.createElement('span'));

  el.append(head, body, card);
  return { el, head, body, card };
}

function sep(doc) {
  const s = doc.createElement('span');
  s.className = 'hsep';
  return s;
}