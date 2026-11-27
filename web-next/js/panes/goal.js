/**
 * panes/goal.js — Goal pane.
 *
 * What the cursor is standing on: the node it belongs to, where in the line and column
 * the caret sits, and the gödel range for the corpus. Fed by the editor's `cursor`, `file`
 * and `buffer` ports, which the host connects by name (js/app/pane-host.js), so the same
 * module works in the canvas app and in the shell — and every one of the three describes
 * the TILE's document, so the buffer asked about and the path reported are the same one.
 *
 * In:  `cursor` (value {line, col, path}), `file` (value path), `buffer` (value text)
 * Out: `goalAt` (value {line, node})
 * Call: `refresh`
 *
 * WHAT IT SHOWS IS THE OBLIGATION AT THE CARET, and `POST /api/goal` is one call for all of it:
 * the statement being proved, where the proof stands, the caret row's verdict (the same check the
 * editor's strip runs), what the corpus still has to bridge — with the `gap` when it cannot — and
 * the rewrites that apply from here. Those were four questions; a goal pane that asked four times
 * per keystroke is a goal pane nobody keeps open.
 *
 * When the engine cannot answer (a host with no route, a buffer that does not parse yet) the pane
 * falls back to what it can prove itself: the node, the caret and the corpus's gödel range.
 */

import { valuePort, callPort } from '../core/ports.js';

const basename = (path) => String(path || '').split('/').pop().replace(/\.qbit$/, '') || '—';

export default {
  kind: 'goal',
  title: 'Goal',
  ports: {
    in: { cursor: 'value', buffer: 'value', file: 'value' },
    out: { goalAt: 'value' },
    call: { refresh: 'call' },
  },
  create(ctx) {
    const esc = (ctx.ui && ctx.ui.esc) || ((s) => String(s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c])));

    let el = null;
    let body = null;
    let current = { line: 1, col: 1, path: '' };
    let buffer = '';
    let goal = null;

    const portGoalAt = valuePort(null);
    const portCursor = valuePort({ line: 1, col: 1, path: '' });
    const portBuffer = valuePort('');
    const portFile = valuePort('');

    let settle = 0;
    portCursor.subscribe((v) => {
      current = { line: 1, col: 1, ...(v || {}) };
      if (v && v.path) current.path = v.path;
      paint();
      /* the obligation is a corpus answer: ask once the caret settles, not on every move */
      clearTimeout(settle);
      settle = setTimeout(() => { refresh(); }, 250);
    });
    portBuffer.subscribe((v) => {
      buffer = v || '';
      clearTimeout(settle);
      settle = setTimeout(() => { refresh(); }, 400);
    });
    portFile.subscribe((v) => {
      // a file is the path, once the editor says which one the caret is in
      if (!current.path || current.path !== v) current = { ...current, path: v || '' };
      paint();
    });

    /* ---- the row diff: what the step from one expression to the next CHANGED ----
     *
     * "the row before it, and added/removed parts colored" is a claim about two strings, so it
     * is computed on two strings: a longest-common-subsequence over whitespace-delimited tokens,
     * with the tokens only one side has marked. MathML would be nicer; the corpus's own
     * expression text is what the engine answers with, and colouring IT is honest about what
     * was compared. */
    function diffTokens(a, b) {
      const A = String(a || '').trim().split(/\s+/).filter(Boolean);
      const B = String(b || '').trim().split(/\s+/).filter(Boolean);
      const n = A.length, m = B.length;
      // lengths of the common subsequence of A[i..] and B[j..]
      const dp = Array.from({ length: n + 1 }, () => new Array(m + 1).fill(0));
      for (let i = n - 1; i >= 0; i--) {
        for (let j = m - 1; j >= 0; j--) {
          dp[i][j] = A[i] === B[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
        }
      }
      const out = [];
      let i = 0, j = 0;
      while (i < n && j < m) {
        if (A[i] === B[j]) { out.push({ t: 'same', v: A[i] }); i++; j++; }
        else if (dp[i + 1][j] >= dp[i][j + 1]) { out.push({ t: 'del', v: A[i] }); i++; }
        else { out.push({ t: 'add', v: B[j] }); j++; }
      }
      while (i < n) out.push({ t: 'del', v: A[i++] });
      while (j < m) out.push({ t: 'add', v: B[j++] });
      return out;
    }

    /** The two rows as one line of coloured tokens. */
    function diffHtml(before, after) {
      const parts = diffTokens(before, after);
      return parts.map((p) => {
        const v = esc(p.v);
        if (p.t === 'del') return '<span class="g-del">' + v + '</span>';
        if (p.t === 'add') return '<span class="g-add">' + v + '</span>';
        return '<span class="g-same">' + v + '</span>';
      }).join(' ');
    }

    /** Insert a citation at the editor's caret. The Goal panel OFFERS a citation; the editor
     *  owns the caret, so the panel asks it — `editor.insert` is that call (js/panes/editor.js). */
    function insertCite(cite) {
      const e = ctx.ws && ctx.ws.pane ? ctx.ws.pane('editor') : null;
      const call = e && e.call && e.call.insert;
      if (call && typeof call.invoke === 'function') {
        try { call.invoke('[' + cite + ']'); } catch (err) { console.warn('[goal] insert:', err.message); }
      }
    }

    /** What the corpus made of this position, as the tile shows it. */
    function answerHtml(g) {
      if (!g) return localHtml();
      const rows = [];
      if (g.admitted) {
        rows.push('<b>' + esc(g.node || '') + '</b> <span class="g-chip local">admitted</span>');
        rows.push('<span class="gdim">stated, not proved — the emitter makes it an axiom</span>');
        return rows.join('<br>');
      }
      /* THE OBLIGATION: what the statement asks for, and where the proof stands now */
      if (g.target) rows.push('<span class="g-turn">⊢</span> <span class="m">' + esc(g.target) + '</span>');
      /* THE STEP, with what it added and what it removed coloured: the row before the caret's
         row, then the caret's row, diffed. `g.current` is where the proof stands and
         `g.target` is where it has to get to, so the two rows are exactly this pair. */
      if (g.current && g.current !== g.target) {
        rows.push('<span class="gdim">from</span> <span class="m">' + diffHtml(g.current, g.target) + '</span>');
      }
      /* WHAT REMAINS: finishable (with the citations), or the leaves that differ */
      const rem = g.remaining || {};
      if (rem.reached && (rem.citations || []).length) {
        const cites = rem.citations.map((c) => c.cite || '?').filter(Boolean);
        rows.push('<span class="gdim">finishable</span> by ' + esc([...new Set(cites)].join(', ')));
      } else if (rem.reached) {
        rows.push('<span class="gdim">closed</span>');
      } else if (rem.gap && rem.gap.length) {
        const missing = rem.gap.map((pair) => esc(pair[0] || '∅') + ' → ' + esc(pair[1] || '∅'));
        rows.push('<span class="gdim">missing</span> ' + missing.join(', '));
      }
      /* THE ROW THE CARET IS ON, judged by the same check the editor's strip runs */
      if (g.verdict) {
        const v = g.verdict.verdict;
        const label = { confirmed: 'confirmed', unconfirmed: 'not confirmed', local: 'local law', 'unknown-cite': 'unknown cite' }[v] || v;
        rows.push('<span class="g-chip ' + esc(v) + '">' + esc(label) + '</span>'
          + (g.verdict.cite ? ' <span class="gdim">' + esc(g.verdict.cite) + '</span>' : ''));
      }
      /* WHAT APPLIES from where the proof stands. Each is a BUTTON: a citation the panel offers
         is one keystroke away from being written, and a list you have to retype is a list you
         read once. */
      if (g.steps && g.steps.length) {
        rows.push('<span class="gdim">next</span>');
        for (const st of g.steps.slice(0, 4)) {
          rows.push('<button class="g-cite" type="button" data-cite="' + esc(st.cite || '') + '"'
            + ' title="insert [' + esc(st.cite || '') + '] at the caret">'
            + esc(st.cite || '?') + '</button> → ' + esc(st.expr || ''));
        }
      }
      if (!rows.length) return localHtml();
      /* the node the ENGINE named, not the app's active file: a goal tile may be looking at a
         document another tile opened, and the answer says which node it answered about */
      rows.push('<span class="gdim">L' + (g.line || current.line) + ' · ' + esc(g.node || basename(current.path))
        + ' · ← editor.cursor</span>');
      return rows.join('<br>');
    }

    /** The local answer: the node, the caret and the corpus's gödel range. */
    function localHtml() {
      const data = ctx.data || {};
      const s = data.stats || {};
      const node = basename(current.path);
      const range = s.godel ? ('gödel range ' + s.godel.min + '..' + s.godel.max) : '';
      return 'node <b>' + esc(node) + '</b><br>'
        + 'cursor <span class="m">L' + current.line + ' C' + current.col + '</span><br>'
        + '⊢ ' + esc(range)
        + '<br><span class="gdim">← editor.cursor</span>';
    }

    /** The published payload of `goalAt`: where the caret is, which node, and the buffer
     *  it is in. `node` is the addition (the tile's own label); the other three were
     *  already the contract and a wire downstream may be reading any of them.
     *  A DECLARATION, not a const: the port subscriptions below fire on subscribe, which
     *  is during this call, so anything `paint` reads has to be hoisted. */
    function goalAt() {
      return {
        line: current.line,
        node: basename(current.path),
        buffer,
        cursor: { path: current.path, line: current.line, col: current.col },
      };
    }

    function paint() {
      if (body) {
        body.innerHTML = answerHtml(goal);
        /* the citation buttons are re-created on every paint, so the listener goes on the body */
        for (const b of body.querySelectorAll('.g-cite')) {
          b.addEventListener('click', () => insertCite(b.dataset.cite));
        }
      }
      portGoalAt.set(goalAt());
    }

    /** Ask the engine for the goal at this position; keep the local answer if there is none. */
    let asking = 0;
    async function refresh() {
      const answer = { ok: true, ...goalAt() };
      const url = ctx.url && ctx.url('goal.post');
      if (!url || !buffer.trim()) { paint(); return answer; }
      const mine = ++asking;
      try {
        const r = await fetch(url, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ path: current.path, content: buffer, line_no: current.line }),
        });
        const j = await r.json().catch(() => ({}));
        /* A LATER CURSOR WINS: the answer for the position the caret has left must not overwrite
           the one for where it is (the same trap the IR and Lean panes carry). */
        if (mine !== asking) return answer;
        goal = j && j.goal ? j.goal : null;
        if (goal && goal.node) answer.node = goal.node;
      } catch (e) {
        // a host with no route, or a buffer the engine cannot read yet: the local answer stands
        goal = null;
      }
      paint();
      return answer;
    }

    const callRefresh = callPort(refresh);

    return {
      mount(host) {
        el = document.createElement('div');
        el.className = 'goal';
        body = el;
        host.appendChild(el);
        paint();
      },
      unmount() {
        if (el && el.parentNode) el.parentNode.removeChild(el);
        el = body = null;
      },
      /* the registry reads in/out/call off the instance; the canvas host accepts either
         shape, so the same module serves both */
      in: { cursor: portCursor, buffer: portBuffer, file: portFile },
      out: { goalAt: portGoalAt },
      call: { refresh: callRefresh },
      get ports() { return { in: this.in, out: this.out, call: this.call }; },
    };
  },
};