/**
 * panes/tree.js — Tree pane (M4.08).
 *
 * Nested file tree explorer with collapsible folders.
 *
 * Ported from index.html 131-132 (tree-list) and app.js 2159-2222 (tree functions).
 *
 * In:  `files` (value string[]), `current` (value path)
 * Out: `selection` (value path)
 * Call: `reveal(path)` (expands folders + scrolls into view)
 */

export default {
  kind: 'tree',
  title: 'Tree',
  ports: {
    in: { files: 'value', current: 'value' },
    out: { selection: 'value' },
    call: { reveal: 'call' },
  },
  create(ctx) {
    const { ws, event } = ctx;
    let el = null;
    let treeEl = null;
    let files = [];
    let currentPath = '';
    const collapsed = new Set();

    // Build nested tree structure from flat file paths
    function buildTree(filePaths) {
      const root = { dirs: new Map(), files: [], path: '' };
      for (const p of filePaths) {
        const parts = String(p).split('/');
        let cur = root;
        for (let i = 0; i < parts.length - 1; i++) {
          if (!cur.dirs.has(parts[i])) {
            cur.dirs.set(parts[i], { dirs: new Map(), files: [], path: parts.slice(0, i + 1).join('/') });
          }
          cur = cur.dirs.get(parts[i]);
        }
        cur.files.push({ name: parts[parts.length - 1], path: p });
      }
      return root;
    }

    let tree = buildTree(files);

    function render() {
      if (!treeEl) return;
      treeEl.innerHTML = '';

      const ul = document.createElement('ul');
      ul.className = 'tree-root';

      function buildTreeUL(node, depth) {
        const ul = document.createElement('ul');
        if (depth > 0) ul.className = 'tree-sub';

        [...node.dirs.entries()].sort((a, b) => a[0] < b[0] ? -1 : 1).forEach(([k, sub]) => {
          const key = 'tree:' + sub.path;
          const isCollapsed = collapsed.has(key);

          const li = document.createElement('li');
          li.className = 'tree-dir' + (isCollapsed ? ' collapsed' : '');
          li.dataset.tdir = sub.path;

          const caret = document.createElement('span');
          caret.className = 'ng-caret';
          caret.textContent = isCollapsed ? '\u25b8' : '\u25be';

          const lbl = document.createElement('span');
          lbl.className = 'td-label';
          lbl.textContent = k + '/'; // eslint-disable-line no-useless-escape

          const cnt = document.createElement('span');
          cnt.className = 'ng-count';
          cnt.textContent = String(countFiles(sub));

          li.append(caret, lbl, cnt);

          if (!isCollapsed) {
            ul.appendChild(buildTreeUL(sub, depth + 1));
          }

          li.addEventListener('click', (e) => {
            if (e.target.closest('.tree-file')) return;
            e.stopPropagation();
            if (collapsed.has(key)) {
              collapsed.delete(key);
            } else {
              collapsed.add(key);
            }
            render();
          });

          ul.appendChild(li);
        });

        node.files.slice().sort((a, b) => a.name < b.name ? -1 : 1).forEach((f) => {
          const isActive = f.path === currentPath;

          const li = document.createElement('li');
          li.className = 'tree-file' + (isActive ? ' active' : '');
          li.dataset.tfile = f.path;
          li.title = f.path;

          const dot = document.createElement('span');
          dot.className = 'tdot';

          const lbl = document.createElement('span');
          lbl.className = 'node-label';
          lbl.textContent = f.name;

          li.append(dot, lbl);

          li.addEventListener('click', (e) => {
            e.stopPropagation();
            currentPath = f.path;
            event('selection').emit(currentPath);
            ws.emit('selection', { path: currentPath });
            render();
          });

          ul.appendChild(li);
        });

        return ul;
      }

      function countFiles(node) {
        let n = node.files.length;
        node.dirs.forEach((d) => { n += countFiles(d); });
        return n;
      }

      ul.appendChild(buildTreeUL(tree, 0));
      treeEl.appendChild(ul);
    }

    function mount(container) {
      el = document.createElement('div');
      el.className = 'tree-pane';
      el.style.cssText = `
        padding: 4px;
        font-size: 11px;
        font-family: monospace;
        overflow: auto;
        height: 100%;
        box-sizing: border-box;
      `;

      treeEl = document.createElement('div');
      treeEl.className = 'tree-content';
      el.appendChild(treeEl);

      container.appendChild(el);
      render();
    }

    return {
      mount,
      unmount() {
        if (el && el.parentNode) el.parentNode.removeChild(el);
        el = null;
        treeEl = null;
      },
      in: {
        files(v) {
          files = v || [];
          tree = buildTree(files);
          render();
        },
        current(v) {
          currentPath = v || '';
          render();
        },
      },
      out: {},
      call: {
        reveal(path) {
          const parts = String(path).split('/');
          let curPath = '';
          for (let i = 0; i < parts.length - 1; i++) {
            curPath = curPath ? curPath + '/' + parts[i] : parts[i];
            collapsed.delete('tree:' + curPath);
          }
          currentPath = path;
          render();
        },
      },
    };
  },
};
