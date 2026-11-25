/**
 * live/field.js — the blueprint behind the tiles.
 *
 * The field is an engineering drawing, not wallpaper: a minor rule grid, a heavier
 * major grid every five, a long tick every twenty-five, and (on a plate) a border,
 * corner registration marks and a title block. So the eye can read distance, the
 * drawing has a scale bar; so the field belongs to the palette, every ink here is
 * colour-mixed from the theme's own accent and ink rather than hardcoded.
 *
 * Two inputs decide what it looks like:
 *   · the *space*  — geometry, from js/defaults/blueprints.json (user-overridable)
 *   · the *theme*  — ink and alphas, from the theme module's `blueprint` block
 * so a theme can ship a field as part of its identity without touching CSS.
 */
import { loadJson } from '../lib/data.js';

const SPACES_URL = '/js/defaults/blueprints.json';
const BRANDS_URL = '/js/defaults/brands.json';

/* fallbacks: the field must draw even if a data module is missing or edited badly */
const SPACES_FALLBACK = { default: 'graph', spaces: { graph: { name: 'Graph paper' }, void: { name: 'Deep field' } } };
const BRANDS_FALLBACK = { default: 'qubitick', marks: { qubitick: { tex: '⟨qu(b(i(t|t)ick))⟩', word: 'qubitick' }, none: {} } };

/* the layer stack, front to back as painted */
const LAYERS = ['hatch', 'minor', 'major', 'tick', 'plate'];

let spacesCache = null;
let brandsCache = null;

/** The blueprint spaces (geometry modules) and their default. */
export async function loadSpaces() {
  if (spacesCache) return spacesCache;
  try {
    const j = await loadJson(SPACES_URL);
    spacesCache = { default: j.default || 'graph', spaces: j.spaces || {}, resolve: j.resolve || {} };
  } catch (e) {
    console.warn('[field] blueprints:', e.message);
    spacesCache = SPACES_FALLBACK;
  }
  return spacesCache;
}

/** The wordmark modules. */
export async function loadBrands() {
  if (brandsCache) return brandsCache;
  try {
    const j = await loadJson(BRANDS_URL);
    brandsCache = { default: j.default || 'qubitick', marks: j.marks || {} };
  } catch (e) {
    console.warn('[field] brands:', e.message);
    brandsCache = BRANDS_FALLBACK;
  }
  return brandsCache;
}

/* ---------- colour ---------- */
function rgbOf(hex) {
  const h = String(hex || '').replace('#', '');
  const n = h.length === 3 ? h.split('').map((c) => c + c).join('') : h;
  if (!/^[0-9a-f]{6}$/i.test(n)) return [128, 128, 128];
  return [0, 2, 4].map((i) => parseInt(n.slice(i, i + 2), 16));
}
const rgba = (hex, a) => {
  const [r, g, b] = rgbOf(hex);
  return `rgba(${r}, ${g}, ${b}, ${Math.max(0, Math.min(1, a)).toFixed(3)})`;
};
/** mix two hex colours in plain sRGB (enough for hairlines, no lib needed) */
function mix(a, b, t) {
  const A = rgbOf(a), B = rgbOf(b);
  const h = (i) => Math.round(A[i] + (B[i] - A[i]) * t).toString(16).padStart(2, '0');
  return `#${h(0)}${h(1)}${h(2)}`;
}
const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));

/* ---------- the wordmark, as markup ----------
 * Composed rather than typeset: the KaTeX form would cost a 400 kB library and a
 * reflow at boot for a decorative mark. The glyphs are the same ones KaTeX would
 * pick (serif, tall angle brackets), so it reads as maths without loading any. */
function markHtml(parts = {}) {
  const qu = parts.qu || 'qu';
  const rest = parts.rest != null ? parts.rest : 'b(i(t|t)ick)';
  // the first application is tinted, the rest is ink: two tones, one mark
  const [head, ...tail] = rest.split('');
  return `<span class="qk-lt">⟨</span>`
    + `<span class="qk-a">${esc(qu)}${esc(head || '')}</span>`
    + `<span class="qk-b">${esc(tail.join(''))}</span>`
    + `<span class="qk-gt">⟩</span>`;
}
/** Split the drawn form: `⟨qu(b(i(t|t)ick))⟩` -> { qu:'qu', rest:'b(i(t|t)ick)' } */
function splitPlain(plain) {
  const inner = String(plain || '').replace(/^[⟨<]+|[⟩>]+$/g, '');
  if (!inner) return { qu: '', rest: '' };
  return { qu: inner.slice(0, 2), rest: inner.slice(2) };
}
function esc(s) {
  return String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
}

/* Split `qu(b(i(t|t)ick))` into "qu" + "b(i(t|t)ick)" without caring about depth:
 * the mark is decorative, so the split is by convention, not by parsing LaTeX. */
function splitMark(tex, word) {
  const flat = String(tex || '').replace(/\\left|\\right|\\mathrm|\\bigl|\\bigr|\\mid|\\,|\s/g, '');
  const inner = flat.replace(/^[⟨<]+|[⟩>]+$/g, '');
  if (!inner) return { qu: word || 'qubitick', rest: '' };
  const qu = inner.slice(0, 2);
  return { qu, rest: inner.slice(2) };
}

/**
 * Build the field. It owns the DOM inside the host (#grid) and exposes the two
 * things the app changes: the theme, and the camera.
 *
 * @param {HTMLElement} host - the #grid element
 * @param {{onPickSpace?: (id: string) => void}} [opts]
 */
export function createField(host, opts = {}) {
  if (!host) throw new Error('createField needs a host element');
  host.classList.add('bp');
  host.dataset.bp = 'graph';

  /* layers, in paint order */
  const layer = {};
  for (const name of LAYERS) {
    const el = document.createElement('div');
    el.className = `bp-layer bp-${name}`;
    host.appendChild(el);
    layer[name] = el;
  }
  /* the tick layer is two rulers along the sheet's edges: a mark per minor cell, a
     longer one per major, a long one per station — the way a drawing is read */
  for (const axis of ['x', 'y']) {
    const r = document.createElement('i');
    r.className = `bp-rule bp-rule-${axis}`;
    layer.tick.appendChild(r);
  }

  /* the sheet's title block: the wordmark, what the field is, and its scale — the
     furniture a drawing carries so you can read it without a legend */
  const block = document.createElement('div');
  block.className = 'bp-block';
  block.setAttribute('aria-hidden', 'true');
  const blockMark = document.createElement('div');
  blockMark.className = 'bp-block-mark';
  const rows = document.createElement('div');
  rows.className = 'bp-block-rows';
  const scale = document.createElement('div');
  scale.className = 'bp-scale';
  scale.innerHTML = '<i></i><b></b>';
  block.append(blockMark, rows, scale);
  host.appendChild(block);

  /* the banner: the wordmark at drawing scale, printed as a stamp — rotated and
     framed the way a drawing carries a revision block. Sized, turned and ruled by
     the theme, and adjustable from the view tile */
  const banner = document.createElement('div');
  banner.className = 'bp-banner';
  banner.setAttribute('aria-hidden', 'true');
  const bannerMark = document.createElement('div');
  bannerMark.className = 'bp-banner-mark';
  banner.append(bannerMark);
  host.appendChild(banner);

  /* the corner mark: the mark alone, at page-mark scale, top-right of the sheet */
  const corner = document.createElement('div');
  corner.className = 'bp-corner';
  corner.setAttribute('aria-hidden', 'true');
  host.appendChild(corner);

  /* the field reads its own data modules, so a caller never has to know that a
     wordmark is a file: create it, hand it a theme, and it fills in when it can */
  const ready = Promise.all([loadBrands(), loadSpaces()])
    .then(() => { if (theme) drawBrand(theme); return true; })
    .catch(() => false);

  let theme = null;         // the theme module currently drawn
  let bp = null;            // its resolved blueprint block
  let spaceId = null;       // the geometry in use
  let brand = null;             // null: follow the theme's own brand block
  let bannerOverride = null;    // what the view tile asked for, if anything
  let zoom = 1;
  let camX = 0, camY = 0;

  /** Paint the theme's ink into the field's custom properties.
   * @param {object} mod - the theme module
   * @param {object} blueprint - its resolved blueprint block (from theme.js) */
  function ink(mod, blueprint) {
    theme = mod;
    bp = blueprint || (mod && mod.blueprint) || {};
    const v = (mod && mod.vars) || {};
    const accent = v[(bp && bp.accent) || 'cyan'] || v.cyan || v.ink || '#888888';
    const inkc = v[(bp && bp.ink) || 'ink'] || v.ink || '#dddddd';
    const dark = (mod && mod.mode) !== 'light';
    const s = host.style;
    // rules: accent on dark fields reads as drafting ink; on light fields the
    // ink itself is what a drawing is made of, so lean on it instead
    const ruleBase = dark ? accent : mix(inkc, accent, 0.55);
    s.setProperty('--bp-mc', rgba(ruleBase, bp.minorAlpha));
    s.setProperty('--bp-Mc', rgba(mix(ruleBase, inkc, dark ? 0.2 : 0.4), bp.majorAlpha));
    s.setProperty('--bp-tc', rgba(mix(ruleBase, inkc, dark ? 0.35 : 0.6), bp.tickAlpha));
    s.setProperty('--bp-dc', rgba(ruleBase, bp.dotAlpha));
    s.setProperty('--bp-hc', rgba(accent, bp.hatch));
    s.setProperty('--bp-bc', rgba(mix(accent, inkc, 0.35), bp.border));
    s.setProperty('--bp-lc', rgba(mix(inkc, accent, 0.25), Math.min(1, bp.label + 0.1)));
    s.setProperty('--bp-sc', rgba(accent, Math.min(1, bp.tickAlpha + 0.28)));
    s.setProperty('--bp-ic', rgba(inkc, 0.5));
    s.setProperty('--bp-mark', rgba(mix(accent, inkc, dark ? 0.25 : 0.45), dark ? 0.075 : 0.06));
    s.setProperty('--bp-mark-a', rgba(accent, dark ? 0.13 : 0.1));
    s.setProperty('--bp-block-ink', rgba(inkc, dark ? 0.46 : 0.55));
    s.setProperty('--bp-acc', rgba(accent, 0.95));
  }

  /** Choose the geometry: a theme's default, or the user's pick. */
  function preset(id, { silent = false } = {}) {
    spaceId = id || (bp && bp.preset) || 'graph';
    host.dataset.bp = spaceId;
    if (!silent && opts.onPickSpace) opts.onPickSpace(spaceId);
  }

  /** Render the wordmark into the sheet's title block and its corner mark. */
  function drawBrand(mod) {
    const cfg = brand === null ? (bp && bp.brand) : brand;
    const id = cfg && (cfg.mark || cfg);
    const marks = (brandsCache && brandsCache.marks) || {};
    const m = id && marks[id] && marks[id].plain ? marks[id] : null;
    host.dataset.brand = m ? 'on' : 'off';
    if (!m) {
      blockMark.innerHTML = '';
      rows.innerHTML = '';
      corner.innerHTML = '';
      bannerMark.innerHTML = '';
      return;
    }
    /* `plain` when the module has one, so no LaTeX command can reach the screen */
    const parts = m.plain ? splitPlain(m.plain) : splitMark(m.tex, m.word);
    blockMark.innerHTML = markHtml(parts);
    corner.innerHTML = markHtml(splitPlain(m.short || m.plain));
    bannerMark.innerHTML = markHtml(parts);
    rows.innerHTML = `<span>field · ${esc(host.dataset.bp || 'graph')}</span>`
      + `<span>sheet 1/1</span>`;
    styleBanner(cfg);
  }

  /**
   * Turn and rule the banner. The theme supplies the defaults; the view tile can
   * override them, so the same field can carry a stamped banner or a plain one.
   * @param {{rotate?: number, border?: boolean, scale?: number, place?: string, opacity?: number}} cfg
   */
  function styleBanner(cfg) {
    const c = cfg || {};
    const held = bannerOverride || {};
    const rotate = pick(held.rotate, c.rotate, -7);
    const scaled = pick(held.scale, c.scale, 1);
    const bordered = pick(held.border, c.border, true);
    const place = held.place || c.place || 'center';
    const s = host.style;
    s.setProperty('--bp-banner-rot', `${Number(rotate).toFixed(1)}deg`);
    s.setProperty('--bp-banner-scale', String(Number(scaled).toFixed(2)));
    s.setProperty('--bp-banner-alpha', String(Number(pick(held.opacity, c.opacity, 1)).toFixed(2)));
    host.dataset.banner = bordered ? 'ruled' : 'bare';
    host.dataset.bannerPlace = place;
  }
  /** the caller's overrides win over the theme; undefined means "let the theme" */
  function pick(override, themed, fallback) {
    if (override != null) return override;
    if (themed != null) return themed;
    return fallback;
  }

  /** Set the banner's turn/scale/border from outside the theme (the view tile). */
  function setBanner(over) {
    bannerOverride = Object.assign({}, bannerOverride, over || {});
    styleBanner(bp && bp.brand);
    return getBanner();
  }
  function getBanner() {
    const c = (bp && bp.brand) || {};
    const cs = getComputedStyle(host);
    return {
      rotate: parseFloat(cs.getPropertyValue('--bp-banner-rot')) || 0,
      scale: parseFloat(cs.getPropertyValue('--bp-banner-scale')) || 1,
      border: host.dataset.banner === 'ruled',
      place: host.dataset.bannerPlace || 'center',
      opacity: parseFloat(cs.getPropertyValue('--bp-banner-alpha')) || 1,
      fromTheme: !bannerOverride,
    };
  }

  /** The scale bar: a round world length that lands between 60 and 140 screen px. */
  function drawScale() {
    const steps = [1, 2, 5, 10, 20, 50, 100, 200, 500, 1000];
    const want = 90;                        // px
    let len = steps[0];
    for (const s of steps) {
      if (s * zoom <= 140) len = s;
      if (s * zoom > want) break;
    }
    const px = Math.max(18, Math.min(160, len * zoom));
    scale.style.setProperty('--bp-scale-w', `${px.toFixed(0)}px`);
    const label = scale.querySelector('b');
    if (label) label.textContent = len >= 1000 ? `${len / 1000}k u` : `${len} u`;
  }

  /** Re-place every rule for the current camera. One function, so the layers cannot drift. */
  function sync(cam) {
    if (cam) { camX = cam.x || 0; camY = cam.y || 0; zoom = cam.z || 1; }
    if (!bp) return;
    const step = clamp(bp.minor * zoom, 9, 34);   // minor cell in screen px
    const major = step * 5;
    const tick = major * 5;
    const s = host.style;
    s.setProperty('--bp-minor', `${step.toFixed(2)}px`);
    s.setProperty('--bp-major', `${major.toFixed(2)}px`);
    s.setProperty('--bp-tick', `${tick.toFixed(2)}px`);
    const mod = (v, m) => `${(((v % m) + m) % m).toFixed(2)}px`;
    s.setProperty('--bp-mx', mod(camX, step));
    s.setProperty('--bp-my', mod(camY, step));
    s.setProperty('--bp-Mx', mod(camX, major));
    s.setProperty('--bp-My', mod(camY, major));
    s.setProperty('--bp-tx', mod(camX, tick));
    s.setProperty('--bp-ty', mod(camY, tick));
    s.setProperty('--bp-hatch', `${(step * 5).toFixed(2)}px`);
    // the wordmark belongs to the field: as the camera pulls back the field is what
    // you see, so the mark may come up; at working zoom the tiles are the subject
    host.dataset.zoom = zoom < 0.4 ? 'far' : 'near';
    // density: a minor rule per 3px is noise, so it steps back and lets the major
    // grid carry the field; far out, the tick grid is what survives
    host.dataset.density = step <= 7 ? 'dense' : step >= 19 ? 'coarse' : 'mid';
    drawScale();
  }

  /** Apply a theme module: ink, geometry default, wordmark, then re-place. */
  function setTheme(mod, blueprint) {
    bannerOverride = null;          // a new theme means a new banner, unless overridden again
    ink(mod, blueprint);
    preset(null, { silent: true });     // the theme names its own drawing
    drawBrand(mod);
    sync();
    return { space: spaceId };
  }

  /** The user's pick of geometry, persisted by the caller. */
  function setSpace(id) {
    if (!id) return spaceId;
    preset(id, { silent: true });
    drawBrand(theme);
    return spaceId;
  }

  /** Space modules, for the view tile's picker. */
  async function spaces() {
    const { default: def, spaces: all } = await loadSpaces();
    return { default: def, list: Object.entries(all).map(([id, s]) => ({ id, name: s.name || id, note: s.note || '' })) };
  }

  return {
    el: host,
    ready,
    setTheme,
    setSpace,
    setBanner,
    banner: getBanner,
    spaces,
    sync,
    get space() { return spaceId; },
    get theme() { return theme; },
    /** what the field contributed, for the saved workspace */
    describe() { return { space: spaceId, brand: host.dataset.brand === 'on' }; },
    destroy() { host.innerHTML = ''; host.classList.remove('bp'); },
  };
}