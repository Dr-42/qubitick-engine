/**
 * live/tree.js — the corpus tree: folders, kinds, counts, filter.
 * Pure DOM. No framework. Emits picks through the caller's callback.
 */

const KIND_OF = [
  [/\/Axioms\//, { id: 'axiom', label: 'AXIOM', hue: 'violet' }],
  [/\/Definitions\//, { id: 'def', label: 'DEF', hue: 'amber' }],
  [/\/Theorems\//, { id: 'thm', label: 'THM', hue: 'green' }],
  [/\/Calibrations\//, { id: 'cal', label: 'CAL', hue: 'cyan' }],
];

export function kindOf(path) {
  for (const [re, k] of KIND_OF) if (re.test(path)) return k;
  return { id: 'file', label: 'FILE', hue: 'muted' };
}

const base = (p) => String(p).split('/').pop().replace(/\.qbit$/, '');
const dirOf = (p) => {
  const s = String(p);
  return s.slice(0, s.lastIndexOf('/'));
};

/** group flat paths into a folder tree: {name, path, files[], dirs[]} */
export function groupPaths(paths) {
  const root = { name: 'corpus', path: '', files: [], dirs: [] };
  for (const p of paths) {
    const parts = String(p).split('/').filter(Boolean);
    let node = root;
    for (let i = 0; i < parts.length - 1; i++) {
      const name = parts[i];
      let next = node.dirs.find((d) => d.name === name);
      if (!next) {
        next = { name, path: parts.slice(0, i + 1).join('/'), files: [], dirs: [] };
        node.dirs.push(next);
      }
      node = next;
    }
    node.files.push({ path: p, name: base(p), kind: kindOf(p) });
  }
  const sortRec = (n) => {
    n.dirs.sort((a, b) => a.name.localeCompare(b.name));
    n.files.sort((a, b) => a.name.localeCompare(b.name));
    n.dirs.forEach(sortRec);
  };
  sortRec(root);
  return root;
}

const countFiles = (n) => n.files.length + n.dirs.reduce((a, d) => a + countFiles(d), 0);

/**
 * mountTree(host, paths, opts)
 *   opts.onPick(path)
 *   opts.openPrefix — folder path prefix to expand on first paint
 *   opts.selected   — path currently selected
 */
export function mountTree(host, paths, opts = {}) {
  host.innerHTML = '';
  const wrap = document.createElement('div');
  wrap.className = 'tv';

  // header: count + filter
  const head = document.createElement('div');
  head.className = 'tv-head';
  head.innerHTML = `<span class="tv-title">corpus</span><span class="tv-count">${paths.length}</span>`;
  const filter = document.createElement('input');
  filter.className = 'tv-filter';
  filter.type = 'search';
  filter.placeholder = 'filter nodes…';
  filter.spellcheck = false;
  wrap.append(head, filter);

  const scroller = document.createElement('div');
  scroller.className = 'tv-body';
  wrap.append(scroller);

  const tree = groupPaths(paths);
  const open = new Set();
  // open the shallow path of the first file (= the stratum the app opened)
  if (opts.openPrefix) {
    const parts = opts.openPrefix.split('/').filter(Boolean).slice(0, -1);
    for (let i = 0; i < parts.length; i++) open.add(parts.slice(0, i + 1).join('/'));
  }

  const rows = [];
  function paint() {
    scroller.innerHTML = '';
    const q = filter.value.trim().toLowerCase();
    const matches = (f) => !q || f.name.toLowerCase().includes(q) || f.path.toLowerCase().includes(q);
    const renderDir = (node, depth) => {
      const visible = node.files.filter(matches);
      const subDirs = node.dirs.filter((d) => q ? subtreeMatches(d, matches) : true);
      if (q && !visible.length && !subDirs.length) return;
      if (depth > 0) {
        const row = document.createElement('div');
        row.className = 'tv-dir';
        row.style.paddingLeft = 8 + (depth - 1) * 12 + 'px';
        row.dataset.path = node.path;
        const isOpen = q ? true : open.has(node.path);
        row.innerHTML =
          `<span class="tv-caret${isOpen ? ' open' : ''}">▸</span>` +
          `<span class="tv-name">${node.name}</span>` +
          `<span class="tv-n">${countFiles(node)}</span>`;
        row.addEventListener('click', () => {
          if (open.has(node.path)) open.delete(node.path); else open.add(node.path);
          paint();
        });
        scroller.append(row);
        if (!isOpen) return;
      }
      for (const d of subDirs) renderDir(d, depth + 1);
      for (const f of visible) {
        const row = document.createElement('div');
        row.className = 'tv-file' + (f.path === opts.selected ? ' sel' : '');
        row.style.paddingLeft = 14 + depth * 12 + 'px';
        row.dataset.path = f.path;
        row.dataset.kind = f.kind.id;
        row.innerHTML =
          `<i class="tv-dot"></i><span class="tv-name">${f.name}</span>` +
          `<span class="tv-kind">${f.kind.label}</span>`;
        row.addEventListener('click', () => {
          opts.selected = f.path;
          scroller.querySelectorAll('.tv-file.sel').forEach((x) => x.classList.remove('sel'));
          row.classList.add('sel');
          opts.onPick && opts.onPick(f.path);
        });
        scroller.append(row);
      }
    };
    renderDir(tree, 0);
    if (!scroller.childElementCount) {
      scroller.innerHTML = '<div class="tv-empty">no node matches</div>';
    }
  }

  let t = 0;
  filter.addEventListener('input', () => { clearTimeout(t); t = setTimeout(paint, 70); });
  paint();
  host.append(wrap);
  return { repaint: paint, setSelected(p) { opts.selected = p; paint(); } };
}

function subtreeMatches(node, matches) {
  if (node.files.some(matches)) return true;
  return node.dirs.some((d) => subtreeMatches(d, matches));
}