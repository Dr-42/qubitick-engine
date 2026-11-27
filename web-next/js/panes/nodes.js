/**
 * panes/nodes.js — Nodes pane (M4.09).
 *
 * Corpus node explorer with grouping by strata/era, node inspector,
 * filter, and selection events.
 *
 * Ported from app.js 2223-2330 (strataKey, renderSidebar, selectNode,
 * applyFilter) and index.html 95-136 (node list, node-detail, filter).
 *
 * In:  `corpus` (value {nodes, edges...}), `highlight` (value node id)
 * Out: `selection` (value node id)
 * Call: `focus(id)`
 */

export default {
  kind: 'nodes',
  title: 'Nodes',
  ports: {
    in: { corpus: 'value', highlight: 'value' },
    /* an EVENT, for the same reason as the tree's: `ctx.event('selection').emit(id)` announces a
       click, and a value port has no `emit`. */
    out: { selection: 'event' },
    call: { focus: 'call' },
  },
  create(ctx) {
    const { ws, event } = ctx;
    let el = null;
    let listEl = null;
    let detailEl = null;
    let filterEl = null;
    let countEl = null;
    let corpus = { nodes: [], edges: [] };
    let selectedId = '';
    const collapsedGroups = new Set();
    let filterText = '';

    function strataKey(n) {
      const d = (n && n.data) || {};
      if (d.era && d.level) return d.era + ' \u00b7 ' + d.level;
      const m = /Qubitick\.(Era\d+)\.(Strata\d+)\./.exec(d.file || '');
      return m ? m[1] + ' \u00b7 ' + m[2] : 'Unstratified';
    }

    function emitSelection(id) {
      selectedId = id;
      event('selection').emit(id);
      ws.emit('selection', { path: id });
      render();
    }

    function focus(id) {
      selectedId = id;
      const node = corpus.nodes.find(n => n.data.id === id);
      if (node) {
        const k = strataKey(node);
        collapsedGroups.delete(k);
      }
      render();
    }

    function render() {
      if (!listEl || !detailEl || !filterEl || !countEl) return;

      const q = filterText.toLowerCase();

      const groups = new Map();
      corpus.nodes.forEach(n => {
        const k = strataKey(n);
        if (!groups.has(k)) groups.set(k, []);
        groups.get(k).push(n);
      });

      countEl.textContent = corpus.nodes.length;

      listEl.innerHTML = '';
      const ul = document.createElement('ul');
      ul.className = 'node-list';

      [...groups.keys()].sort().forEach(k => {
        const items = groups.get(k);
        const isCollapsed = collapsedGroups.has(k);

        const head = document.createElement('li');
        head.className = 'node-group' + (isCollapsed ? ' collapsed' : '');
        head.dataset.group = k;
        head.title = isCollapsed ? 'expand group' : 'collapse group';
        head.style.cssText = `
          cursor: pointer;
          font-weight: 600;
          padding: 2px 4px;
          border-bottom: 1px solid var(--line);
          display: flex;
          justify-content: space-between;
          align-items: center;
        `;

        const caret = document.createElement('span');
        caret.className = 'ng-caret';
        caret.textContent = isCollapsed ? '\u25b8' : '\u25be';
        caret.style.cssText = 'margin-right: 4px; font-size: 10px;';

        const label = document.createElement('span');
        label.className = 'ng-label';
        label.textContent = k;
        label.style.flex = '1';

        const cnt = document.createElement('span');
        cnt.className = 'ng-count';
        cnt.textContent = items.length;
        cnt.style.cssText = 'font-size: 10px; opacity: 0.7;';

        head.append(caret, label, cnt);

        head.addEventListener('click', () => {
          if (collapsedGroups.has(k)) {
            collapsedGroups.delete(k);
          } else {
            collapsedGroups.add(k);
          }
          render();
        });

        ul.appendChild(head);

        if (!isCollapsed) {
          items.forEach(n => {
            const isActive = n.data.id === selectedId;
            const matchesFilter = !q || n.data.id.toLowerCase().includes(q);

            const li = document.createElement('li');
            li.className = 'node-item' + (isActive ? ' active' : '') + (matchesFilter ? '' : ' hidden');
            li.dataset.id = n.data.id;
            li.dataset.owner = k;
            li.title = n.data.id;
            li.style.cssText = `
              padding: 1px 4px;
              cursor: pointer;
              font-size: 11px;
              display: flex;
              align-items: center;
              gap: 4px;
            `;

            const dot = document.createElement('span');
            dot.className = 'node-dot ' + (n.data.type || '');
            dot.textContent = '\u25cf';
            dot.style.cssText = 'font-size: 8px;';

            const labelSpan = document.createElement('span');
            labelSpan.className = 'node-label';
            labelSpan.textContent = n.data.label || n.data.id;
            labelSpan.style.flex = '1';
            labelSpan.style.overflow = 'hidden';
            labelSpan.style.textOverflow = 'ellipsis';
            labelSpan.style.whiteSpace = 'nowrap';

            const typeSpan = document.createElement('span');
            typeSpan.className = 'node-type';
            typeSpan.textContent = n.data.type || '';
            typeSpan.style.cssText = 'font-size: 9px; opacity: 0.6;';

            li.append(dot, labelSpan, typeSpan);

            li.addEventListener('click', () => {
              emitSelection(n.data.id);
            });

            ul.appendChild(li);
          });
        }

        const visibleItems = items.filter(n => !q || n.data.id.toLowerCase().includes(q));
        if (q && visibleItems.length === 0) {
          head.style.display = 'none';
        } else {
          head.style.display = '';
        }
      });

      listEl.appendChild(ul);

      const node = corpus.nodes.find(n => n.data.id === selectedId);
      detailEl.innerHTML = '';

      if (!node) {
        const empty = document.createElement('div');
        empty.className = 'node-detail-empty';
        empty.textContent = 'select a node to inspect its dependencies';
        empty.style.cssText = `
          padding: 8px;
          font-size: 11px;
          color: var(--muted);
          text-align: center;
        `;
        detailEl.appendChild(empty);
        return;
      }

      const deps = corpus.edges.filter(e => e.data.source === selectedId).map(e => e.data.target);
      const uses = corpus.edges.filter(e => e.data.target === selectedId).map(e => e.data.source);

      const titleDiv = document.createElement('div');
      titleDiv.className = 'nd-title';
      titleDiv.style.cssText = `
        font-weight: 600;
        font-size: 12px;
        margin-bottom: 6px;
        display: flex;
        justify-content: space-between;
        align-items: baseline;
      `;
      titleDiv.innerHTML = '◈ ' + (node.data.label || node.data.id) +
        ' <span style="color: var(--muted); font-size: 10px">[' + (node.data.type || '') +
        (node.data.era ? ' · ' + node.data.era + ' · tier ' + node.data.tier : '') + ']</span>';
      detailEl.appendChild(titleDiv);

      const depsDiv = document.createElement('div');
      depsDiv.className = 'nd-row';
      depsDiv.style.cssText = 'margin-bottom: 4px; font-size: 11px;';
      depsDiv.innerHTML = '<span style="font-weight: 600">depends on:</span> ' +
        (deps.length ? deps.map(d => '<span class="chip" style="background: var(--line); padding: 1px 4px; border-radius: 3px; font-size: 10px; margin-right: 4px; cursor: pointer;">' + d + '</span>').join('') :
         '<b style="color: var(--muted)">∅</b>') + '';
      detailEl.appendChild(depsDiv);

      const usesDiv = document.createElement('div');
      usesDiv.className = 'nd-row';
      usesDiv.style.cssText = 'font-size: 11px;';
      usesDiv.innerHTML = '<span style="font-weight: 600">used by:</span> ' +
        (uses.length ? uses.map(d => '<span class="chip" style="background: var(--line); padding: 1px 4px; border-radius: 3px; font-size: 10px; margin-right: 4px; cursor: pointer;">' + d + '</span>').join('') :
         '<b style="color: var(--muted)">∅</b>') + '';
      detailEl.appendChild(usesDiv);

      detailEl.querySelectorAll('.chip').forEach(c => {
        c.addEventListener('click', (e) => {
          e.stopPropagation();
          focus(c.textContent);
        });
      });
    }

    function mount(container) {
      el = document.createElement('div');
      el.className = 'nodes-pane';
      el.style.cssText = `
        display: flex;
        flex-direction: column;
        height: 100%;
        font-size: 11px;
        font-family: monospace;
      `;

      const header = document.createElement('div');
      header.className = 'nodes-header';
      header.style.cssText = `
        display: flex;
        justify-content: space-between;
        align-items: center;
        padding: 4px 6px;
        border-bottom: 1px solid var(--line);
      `;

      const title = document.createElement('div');
      title.className = 'nodes-title';
      title.textContent = 'Nodes';
      title.style.cssText = 'font-weight: 600; font-size: 12px;';

      filterEl = document.createElement('input');
      filterEl.type = 'text';
      filterEl.placeholder = '/ filter nodes…';
      filterEl.spellcheck = false;
      filterEl.autocomplete = 'off';
      filterEl.style.cssText = `
        flex: 1;
        font-size: 10px;
        padding: 2px 4px;
        border: 1px solid var(--line);
        border-radius: 3px;
        background: var(--bg-inset);
        color: var(--ink);
        margin-left: 8px;
      `;
      filterEl.addEventListener('input', (e) => {
        filterText = e.target.value;
        render();
      });

      countEl = document.createElement('span');
      countEl.className = 'node-count';
      countEl.textContent = '0';
      countEl.style.cssText = 'font-size: 10px; opacity: 0.7;';

      header.append(title, filterEl, countEl);
      el.appendChild(header);

      const body = document.createElement('div');
      body.className = 'nodes-body';
      body.style.cssText = `
        display: flex;
        flex: 1;
        overflow: hidden;
      `;

      listEl = document.createElement('div');
      listEl.className = 'nodes-list';
      listEl.style.cssText = `
        flex: 1;
        overflow-y: auto;
        border-right: 1px solid var(--line);
      `;

      detailEl = document.createElement('div');
      detailEl.className = 'nodes-detail';
      detailEl.style.cssText = `
        width: 200px;
        overflow-y: auto;
        padding: 6px;
        background: var(--bg-inset);
      `;

      body.appendChild(listEl);
      body.appendChild(detailEl);
      el.appendChild(body);

      container.appendChild(el);
      render();
    }

    return {
      mount,
      unmount() {
        if (el && el.parentNode) el.parentNode.removeChild(el);
        el = null;
        listEl = null;
        detailEl = null;
        filterEl = null;
        countEl = null;
      },
      in: {
        corpus(v) {
          corpus = v || { nodes: [], edges: [] };
          render();
        },
        highlight(v) {
          if (v) focus(v);
        },
      },
      out: {},
      call: {
        focus,
      },
    };
  },
};
