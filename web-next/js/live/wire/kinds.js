/**
 * live/wire/kinds.js — the node contract.
 *
 * A kind is a pane's data half: which ports it exposes, which types travel on
 * them, and which parameters it lets you turn. It is deliberately a plain data
 * descriptor, so a third-party node is one `registerKind()` call and no code
 * changes anywhere else in the shell.
 *
 *   registerKind('graph', {
 *     title: 'Graph',
 *     category: 'view',
 *     ports: [P('selection','out','value','NodeId'),
 *             P('highlight','in','value','NodeId')],
 *     params: [{ k:'layout', type:'select', opts:['auto','layered','force'] }],
 *   });
 *
 * Two older shapes are still accepted and normalised on the way in, because
 * index.html grew up with them:
 *
 *   inputs:  [['highlight','NodeId']]              // [name, type] pair
 *   outputs: [{ name:'theme', type:'CSS' }]        // descriptor object
 *
 * Port kinds are the three the runtime already knows (core/ports.js):
 *
 *   value  replays its latest on connect, deduped downstream
 *   event  a stream, no replay, every emit lands
 *   call   a method you can invoke, never a wire source (see core/wire.js)
 *
 * Types are nominal with an explicit widening list: `NodeId -> Text -> any`.
 * Nothing here knows about the DOM except `paramControl()`, which builds one
 * control element on demand.
 */

/* ── types ───────────────────────────────────────────────────────────────── */

/**
 * The type vocabulary. `to` lists what a value of this type may be delivered
 * as — the same idea as a coercion lattice, kept deliberately shallow: a
 * corpus id is text, text is text, and `any` is the escape hatch.
 */
export const TYPES = new Map([
  ['NodeId', { desc: 'a corpus node id', tone: 'cyan', to: ['Text', 'any'] }],
  ['Path', { desc: 'a corpus-relative file path', tone: 'amber', to: ['Text', 'any'] }],
  ['Cursor', { desc: '{ line, col } inside a buffer', tone: 'violet', to: ['any'] }],
  ['Text', { desc: 'utf-8 payload — buffer, log line, source', tone: 'green', to: ['any'] }],
  ['CSS', { desc: 'a stylesheet fragment', tone: 'violet', to: ['Text', 'any'] }],
  ['num', { desc: 'IEEE-754 scalar', tone: 'cyan', to: ['any'] }],
  ['Bool', { desc: 'on or off — a switch a wire can throw', tone: 'green', to: ['Text', 'num', 'any'] }],
  ['any', { desc: 'untyped — checked where it is used', tone: 't2', to: ['NodeId', 'Path', 'Cursor', 'Text', 'CSS', 'num', 'any'] }],
  ['Project', { desc: 'a corpus project the engine can serve: { id, name }', tone: 'cyan', to: ['Text', 'any'] }],
]);

/** Register an extra type (a plugin's own nominal type, or a widening alias). */
export function defineType(id, { desc = '', tone = 't2', to = ['any'] } = {}) {
  TYPES.set(String(id), { desc, tone, to: [...to] });
  return TYPES.get(String(id));
}

export const typeTone = (type) => (TYPES.get(type) || {}).tone || 't2';

/**
 * May a value produced as `from` be delivered on a `to` port?
 * Anything can be read as `any`; `any` can be offered anywhere (checked at
 * runtime); otherwise the producer's widening list decides. Unknown types
 * (a plugin type nobody widened) only accept themselves.
 */
export function typeOk(from, to) {
  if (!from || !to) return true;           // untyped ports always connect
  if (from === to || to === 'any' || from === 'any') return true;
  const t = TYPES.get(from);
  return !!(t && Array.isArray(t.to) && t.to.includes(to));
}

/* ── ports ───────────────────────────────────────────────────────────────── */

/** A port descriptor. `kind` is the runtime axis, `type` the data axis. */
export function P(id, dir, kind, type, label) {
  if (dir !== 'in' && dir !== 'out') throw new TypeError(`port ${id}: dir must be in|out`);
  if (!PORT_KINDS.includes(kind)) throw new TypeError(`port ${id}: kind ${kind} is not one of ${PORT_KINDS.join('/')}`);
  return { id, dir, kind, type: type || 'any', label: label || id };
}

export const PORT_KINDS = ['value', 'event', 'call'];

const isPair = (p) => Array.isArray(p);

/**
 * Normalise one loose port form into the canonical descriptor. `fallbackDir`
 * supplies the direction for shapes that do not carry one (the `[name, type]`
 * pairs and the `{ name, type }` objects that index.html grew up with); a
 * descriptor that carries its own `dir` keeps it.
 */
export function normalisePort(raw, fallbackDir) {
  if (!raw) return null;
  if (isPair(raw)) return P(raw[0], fallbackDir, 'value', raw[1]);
  if (raw.dir) {
    return {
      id: raw.id || raw.name,
      dir: raw.dir,
      kind: raw.kind || 'value',
      type: raw.type || 'any',
      label: raw.label || raw.id || raw.name,
    };
  }
  return P(raw.id || raw.name, fallbackDir, raw.kind || 'value', raw.type, raw.label);
}

/** Normalise a whole list; the direction comes from the list it came in. */
export function normalisePorts(list, dir) {
  return (list || []).map((p) => normalisePort(p, dir)).filter(Boolean);
}

/* ── kinds ───────────────────────────────────────────────────────────────── */

export const KINDS = new Map();

/**
 * Register (or amend) a node kind. Re-registering keeps whatever the earlier
 * descriptor set, so index.html can attach a mount to a kind whose ports are
 * declared here without repeating them.
 */
export function registerKind(id, def = {}) {
  const key = String(id);
  const prev = KINDS.get(key) || {};
  // `ports` is the canonical form; `inputs`/`outputs` are the shape index.html
  // uses. Either one replaces the port set; a re-register with neither keeps
  // whatever was declared before (index.html attaching a mount, for example).
  let declared = null;
  if (def.ports || def.inputs || def.outputs) {
    declared = [
      ...normalisePorts(def.ports, null),
      ...normalisePorts(def.inputs, 'in'),
      ...normalisePorts(def.outputs, 'out'),
    ];
  }
  const merged = dedupePorts(declared || prev.ports || []);
  const kind = {
    id: key,
    title: def.title || prev.title || key,
    category: def.category || prev.category || 'view',
    desc: def.desc || prev.desc || '',
    tags: def.tags || prev.tags || [],
    ports: merged,
    params: def.params || prev.params || [],
    defaults: def.defaults || prev.defaults || null,
    // runtime hooks (only used by kinds that declare them)
    tick: def.tick || prev.tick || null,
    onEvent: def.onEvent || prev.onEvent || null,
    stateful: def.stateful != null ? def.stateful : !!prev.stateful,
    lod: def.lod || prev.lod || null,
    // shell side
    mount: def.mount || prev.mount || null,
    state: def.state || prev.state || undefined,
  };
  KINDS.set(key, kind);
  return kind;
}

function dedupePorts(ports) {
  const seen = new Set();
  const out = [];
  for (const p of ports) {
    const key = p.dir + ':' + p.id;
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(p);
  }
  return out;
}

export const getKind = (id) => KINDS.get(String(id)) || null;
export const allKinds = () => [...KINDS.values()];
export const kindsByCategory = (cat) => allKinds().filter((k) => k.category === cat);
export const kindPorts = (id, dir) => (getKind(id)?.ports || []).filter((p) => !dir || p.dir === dir);
export const portOf = (id, portId) => kindPorts(id).find((p) => p.id === portId) || null;

/** Initial parameter state for a kind (explicit defaults, else each param's own). */
export function paramDefaults(kind) {
  const k = typeof kind === 'string' ? getKind(kind) : kind;
  if (!k) return {};
  const state = typeof k.defaults === 'function' ? { ...k.defaults() } : { ...(k.defaults || {}) };
  for (const pd of k.params || []) {
    if (state[pd.k] === undefined) state[pd.k] = pd.value !== undefined ? pd.value : defaultFor(pd);
  }
  return state;
}

function defaultFor(pd) {
  switch (pd.type) {
    case 'toggle': return false;
    case 'select': return pd.opts && pd.opts.length ? (pd.opts[0].v !== undefined ? pd.opts[0].v : pd.opts[0]) : null;
    case 'range': return pd.min !== undefined ? pd.min : 0;
    case 'number': return 0;
    default: return '';
  }
}

const fmtNum = (v, dp = 2) => (Number.isFinite(+v) ? (+v).toFixed(dp) : '—');
export const formatParam = (pd, v) => (pd.fmt ? pd.fmt(v) : pd.type === 'range' || pd.type === 'number' ? fmtNum(v, pd.dp ?? 2) : String(v)) + (pd.unit ? ' ' + pd.unit : '');

/* ── parameter controls ──────────────────────────────────────────────────── */

/**
 * One control for one parameter. Returns the element with a `.sync()` method so
 * a caller can push state back in (a preset load, an undo) without rebuilding.
 *
 * Kept DOM-lazy: importing this module in node must not touch `document`.
 */
export function paramControl(state, pd, onChange) {
  if (typeof document === 'undefined') throw new Error('paramControl needs a DOM');
  const doc = document;
  const span = (cls, text) => {
    const s = doc.createElement('span');
    s.className = cls;
    if (text != null) s.textContent = text;
    return s;
  };

  if (pd.type === 'toggle') {
    const row = doc.createElement('div');
    row.className = 'ctl ctl-tog' + (state[pd.k] ? ' on' : '');
    row.append(span('ctl-l', pd.label), span('ctl-sw'));
    row.addEventListener('click', () => {
      state[pd.k] = !state[pd.k];
      row.classList.toggle('on', !!state[pd.k]);
      onChange && onChange(pd.k, state[pd.k]);
    });
    row.sync = () => row.classList.toggle('on', !!state[pd.k]);
    return row;
  }

  const wrap = doc.createElement('label');
  wrap.className = 'ctl';
  const top = doc.createElement('div');
  top.className = 'ctl-top';
  top.append(span('ctl-l', pd.label));
  const readout = span('ctl-v');
  if (pd.type === 'range' || pd.type === 'number') top.append(readout);
  wrap.append(top);

  if (pd.type === 'select') {
    const sel = doc.createElement('select');
    for (const o of pd.opts || []) {
      const op = doc.createElement('option');
      const isObj = o && typeof o === 'object';
      op.value = isObj ? o.v : o;
      op.textContent = isObj ? o.label ?? o.v : o;
      sel.append(op);
    }
    sel.value = state[pd.k];
    sel.addEventListener('change', () => { state[pd.k] = sel.value; onChange && onChange(pd.k, sel.value); });
    wrap.append(sel);
    wrap.sync = () => { sel.value = state[pd.k]; };
    return wrap;
  }

  if (pd.type === 'text' || pd.type === 'number') {
    const inp = doc.createElement('input');
    inp.type = pd.type === 'number' ? 'number' : 'text';
    if (pd.type === 'number' && pd.step != null) inp.step = pd.step;
    inp.value = state[pd.k];
    inp.addEventListener('input', () => {
      state[pd.k] = pd.type === 'number' ? +inp.value || 0 : inp.value;
      onChange && onChange(pd.k, state[pd.k]);
    });
    wrap.append(inp);
    wrap.sync = () => { inp.value = state[pd.k]; };
    return wrap;
  }

  // range (the default): a slider with a live read-out
  const inp = doc.createElement('input');
  inp.type = 'range';
  if (pd.min != null) inp.min = pd.min;
  if (pd.max != null) inp.max = pd.max;
  inp.step = pd.step != null ? pd.step : 0.01;
  inp.value = state[pd.k];
  const paint = () => {
    readout.textContent = formatParam(pd, +inp.value);
    const min = +(inp.min || 0), max = +(inp.max || 1);
    const p = ((+inp.value - min) / (max - min || 1)) * 100;
    inp.style.setProperty('--fill', p.toFixed(1) + '%');
  };
  inp.addEventListener('input', () => {
    state[pd.k] = +inp.value;
    paint();
    onChange && onChange(pd.k, state[pd.k], true);
  });
  inp.addEventListener('change', () => onChange && onChange(pd.k, state[pd.k], false));
  wrap.append(inp);
  paint();
  wrap.sync = () => { inp.value = state[pd.k]; paint(); };
  return wrap;
}

/* ── the built-in kinds ──────────────────────────────────────────────────── */

/**
 * Declared here, mounted in index.html. Ports are the contract; the mount is
 * the shell's business, so a kind can exist (and be wired) before it can be
 * drawn — which is exactly the state the wire layer needs to bootstrap in.
 */
export const BUILTIN = {
  tree: { title: 'Corpus tree', category: 'source', ports: [P('selection', 'out', 'value', 'NodeId')], desc: 'the corpus outline; emits the selected node' },
  graph: { title: 'Graph', category: 'view', ports: [P('highlight', 'in', 'value', 'NodeId'), P('selection', 'out', 'value', 'NodeId')], desc: 'dependency graph, driven by a highlighted node' },
  stats: { title: 'Stats', category: 'view', ports: [] },
  symbols: { title: 'Symbols', category: 'view', ports: [P('scope', 'in', 'value', 'Path'), P('symbol', 'out', 'value', 'Text')] },
  editor: { title: 'Editor', category: 'source', ports: [P('file', 'out', 'value', 'Path'), P('cursor', 'out', 'value', 'Cursor'), P('buffer', 'out', 'value', 'Text')] },
  goal: { title: 'Goal', category: 'view', ports: [P('cursor', 'in', 'value', 'Cursor'), P('file', 'in', 'value', 'Path')] },
  output: { title: 'Output', category: 'view', ports: [P('log', 'in', 'event', 'Text')] },
  render: { title: 'Lean · Render', category: 'view', ports: [P('buffer', 'in', 'value', 'Text')] },
  ir: { title: 'IR · computed', category: 'computed', ports: [P('buffer', 'in', 'value', 'Text')] },
  lean: { title: 'Lean 4 · computed', category: 'computed', ports: [P('file', 'in', 'value', 'Path'), P('source', 'out', 'value', 'Text')] },
  handwrite: { title: 'Handwriting', category: 'view', ports: [] },
  constant: { title: 'Constant · data', category: 'data', ports: [P('value', 'out', 'value', 'Text')] },
  'js-node': { title: 'JS Program · behaviour', category: 'behaviour', ports: [P('content', 'in', 'value', 'Text'), P('result', 'out', 'value', 'any')] },
  palette: { title: 'Theme · settings', category: 'settings', ports: [P('theme', 'out', 'value', 'CSS')] },
  /* THE PROJECT AND ITS CORPUS. A project node names ONE project and publishes it; a corpus node
     is handed a project and serves that project's corpus on four ports. So the whole interface
     follows one wire: wire a project into a corpus and everything downstream is that project;
     take the wire out and the corpus has nothing to serve. This is the kind pair the app itself
     is booted on (js/world/home.js builds them wired). */
  project: { title: 'Project', category: 'source', ports: [P('project', 'out', 'value', 'Project')], desc: 'one corpus project the engine can serve' },
  corpus: {
    title: 'Corpus', category: 'source',
    ports: [P('project', 'in', 'value', 'Project'),
            P('files', 'out', 'value', 'any'), P('graph', 'out', 'value', 'any'),
            P('stats', 'out', 'value', 'any'), P('symbols', 'out', 'value', 'any')],
    desc: 'the corpus of whatever project is wired in: files, graph, stats, symbols',
  },
  clock: { title: 'Clock · behaviour', category: 'behaviour', ports: [P('tick', 'out', 'event', 'num')], stateful: true },
  track: { title: 'Track · behaviour', category: 'behaviour', ports: [P('tick', 'in', 'value', 'num'), P('meta', 'out', 'value', 'any')] },
};

/** Install every built-in kind. Safe to call twice. */
export function registerBuiltinKinds() {
  for (const [id, def] of Object.entries(BUILTIN)) registerKind(id, def);
  return KINDS;
}

registerBuiltinKinds();