/**
 * lib/citations.js — Shared citation data layer (M4.04).
 *
 * Ported from app.js 362–372 (citeInfo), 2546–2630 (rankCites, fitScore,
 * fitRanked, scanExpansions), and 366–372.
 * No DOM — pure data functions.
 *
 * Exports:
 *   loadCitations(ws) — loads from citations.get endpoint
 *   citeInfo(ws, ref) — memoized citation lookup
 *   rankCites(q, dict) — rank citations by query
 *   fitScore(c, qtoks) — fit scoring for a single candidate
 *   fitRanked(shape, q) — ranked fit results
 *   scanExpansions(text) — scan text for macro glyph expansions
 */

// Per-ws cache
const citationCache = new WeakMap();

// Citation regex (from app.js 364)
const CITE_RE = /\b(?:A\d+\.\d+|T\.[A-Z]{2}\.\d+|T\d+|D\.[A-Z]{2}\.\d+|BI\.\d+\.\d+)\b/g;

/**
 * Load citations dict for a workspace.
 * @param {object} ws - Workspace API
 * @returns {Promise<Array<{ref, surface, ...}>>}
 */
export async function loadCitations(ws) {
  try {
    const endpoint = ws.endpoint('citations');
    const data = await endpoint.call.invoke({});
    return data || [];
  } catch {
    return [];
  }
}

/**
 * Get citation info for a ref (memoized per ws).
 * @param {object} ws - Workspace API
 * @param {string} ref - citation reference
 * @returns {Promise<object>}
 */
export async function citeInfo(ws, ref) {
  let cache = citationCache.get(ws);
  if (!cache) {
    cache = new Map();
    citationCache.set(ws, cache);
  }
  if (cache.has(ref)) return cache.get(ref);
  try {
    const endpoint = ws.endpoint('citations');
    const j = await endpoint.call.invoke({ r: ref });
    cache.set(ref, j);
    return j;
  } catch {
    const e = { error: 'offline' };
    cache.set(ref, e);
    return e;
  }
}

/**
 * Rank citations by query against a dict.
 * @param {string} q - query string
 * @param {Array} dict - citation dict [{ref, surface, ...}]
 * @returns {Array} top 8 matching citations
 */
export function rankCites(q, dict) {
  q = (q || '').trim().toLowerCase();
  if (!q || !dict || !dict.length) return [];

  const qtoks = q.split(/[\s,;]+/).filter(t => t.length > 0);
  const scored = [];

  dict.forEach((c) => {
    const ref = (c.ref || '').toLowerCase();
    const surf = (c.surface || '').toLowerCase();
    let s = 0;

    if (ref === q) s += 100;
    else if (ref.startsWith(q)) s += 60;
    else if (ref.includes(q)) s += 30;
    if (surf.includes(q)) s += 40;

    qtoks.forEach((t) => {
      if (surf.includes(t)) s += 6;
      if (ref.includes(t)) s += 4;
    });

    if (s > 0) scored.push({ c, s });
  });

  scored.sort((a, b) => b.s - a.s);
  return scored.slice(0, 8).map((x) => x.c);
}

/**
 * Fit score for a single candidate against query tokens.
 * @param {object} c - citation candidate
 * @param {Array<string>} qtoks - query tokens
 * @returns {number} score
 */
export function fitScore(c, qtoks) {
  const ref = (c.ref || '').toLowerCase();
  const surf = (c.surface || '').toLowerCase();
  let s2 = 0;
  qtoks.forEach((t) => {
    if (surf.includes(t)) s2 += 6;
    if (ref.includes(t)) s2 += 4;
  });
  return s2;
}

/**
 * Ranked fit results for a shape/query against citation dict.
 * @param {string} shape - shape filter (prefix)
 * @param {string} q - query string
 * @param {Array} dict - citation dict
 * @returns {Array<{ref, score, c}>} ranked rows
 */
export function fitRanked(shape, q, dict) {
  if (!dict || !dict.length) return [];

  const qtoks = (shape || '').trim().toLowerCase().split(/[\s,;=\[\](){}]+/).filter(t => t.length > 0);
  const ql = (q || '').toLowerCase();

  const cand = dict;
  const pre = ql ? cand.filter((c) => c.ref.toLowerCase().startsWith(ql)) : cand.slice();

  const rows = pre.map((c) => ({ ref: c.ref, score: fitScore(c, qtoks), c }));
  rows.sort((a, b) => b.score - a.score);

  if (ql) {
    const seen = new Set(rows.map((x) => x.ref));
    const extra = cand
      .filter((c) => !seen.has(c.ref))
      .map((c) => ({ ref: c.ref, score: fitScore(c, qtoks), c }))
      .filter((x) => x.score > 0);
    extra.sort((a, b) => b.score - a.score);
    extra.slice(0, 4).forEach((x) => rows.push(x));
  }

  return rows;
}

/**
 * Scan text for macro glyph expansions.
 * @param {string} text - text to scan
 * @param {Array<{symbol, ref, ...}>} glyphs - macro entries sorted by symbol length desc
 * @returns {Array<{glyph, ref, n, entry}>} expansion rows
 */
export function scanExpansions(text, glyphs) {
  const ents = glyphs || [];
  if (!ents.length || !text) return [];

  const sorted = ents.slice().sort((a, b) => b.symbol.length - a.symbol.length);
  const out = [];
  const byRef = new Map();

  let i = 0;
  while (i < text.length) {
    let hit = null;
    for (const c of sorted) {
      if (c.symbol && text.startsWith(c.symbol, i)) { hit = c; break; }
    }
    if (hit) {
      let r = byRef.get(hit.ref);
      if (!r) {
        r = { glyph: hit.symbol, ref: hit.ref, n: 0, entry: hit.c };
        byRef.set(hit.ref, r);
        out.push(r);
      }
      r.n += 1;
      i += hit.symbol.length;
    } else {
      i += 1;
    }
  }

  return out;
}

// Export CITE_RE for use by panes that need cite detection
export { CITE_RE };
