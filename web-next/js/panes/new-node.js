/**
 * panes/new-node.js — New Node pane (M4.16).
 *
 * Modal pane for creating a new corpus node with era/strata/dir/name
 * fields, live path preview, and template-based creation.
 *
 * Ported from app.js 1645-1762 (createFile, populateHierarchy,
 * updateNewFilePreview, openNewFile, closeNewFile) and index.html
 * 347-405 (new-file modal markup).
 *
 * EVERYTHING IT INVENTS COMES FROM THE CORPUS IT IS CREATING IN. The pane used to decide all of
 * it itself, out of the harness's own corpus: the era list was `Era0`, the strata list
 * `Strata0|1|2`, the file-name token `BT`, the number always `1` (so the second new theorem
 * overwrote the first), and the body was the harness's algebra with a citation to `A1.4`. A
 * project is the user's: so the lists are the directories its files are in, the token is the one
 * its files already use (or, when they use none, the project's own name), the number is the next
 * free one in that directory, and the body is a STUB that says what to write. A stub that parses
 * is the right thing to hand an author — the header grammar is real (`T.<token>.<n>`,
 * `D.<token>.<n>`, `A<n>`; `D10.` is `unknown node header`), and the statement is theirs.
 *
 * In:  `parent` (value path)
 * Out: `node` (value node id), `created` (event path)
 * Call: `open()`
 */

import { valuePort, eventPort, callPort } from '../core/ports.js';
/* THE ARITHMETIC LIVES IN live/ (where proof-row.js, induce.js and row-check.js do): the pane keeps
   the form, and this decides what the preview says. */
import { layout as layoutOf, planNode, stubFor } from '../live/new-node-plan.js';

export default {
  kind: 'new-node',
  title: 'New Node',
  ports: {
    in: { parent: 'value' },
    out: { node: 'value', created: 'event' },
    call: { open: 'call' },
  },
  create(ctx) {
    const { ws, event } = ctx;
    let el = null;
    let modal = null;
    let backdrop = null;
    let onEscape = null;
    let offParent = null;
    let eraSel = null;
    let strataSel = null;
    let dirSel = null;
    let nameInp = null;
    let previewEl = null;
    /* A REAL PORT, not a read-only stub: the pane's whole promise is "open on the directory this
       node belongs in", and a port with `subscribe` and no `set` can never be told that directory
       by any host. */
    const portParent = valuePort('');

    // Out ports
    const portNode = valuePort(null);
    const portCreated = eventPort();
    const callOpen = callPort(doOpen);

    /* WHICH HOST IS RUNNING THIS PANE. The engine host opens a pane as a MODAL — `open()`, Escape to
       close, a backdrop that swallows a click — and gives it `ctx.ws`. The canvas has no modal
       surface and no socket: it gives a tile, so the same form lives IN the tile, always open. */
    const engineHost = !!(ctx && ctx.ws);

    /** The corpus's files, as the app holds them (`js/host/canvas/data.js` reads the tile's ports). */
    const corpusFiles = () => {
      const files = (ctx.data && ctx.data.files) || [];
      return files.map((f) => (typeof f === 'string' ? f : (f && (f.path || f.name)) || '')).filter(Boolean);
    };

    /** Every path the pane knows of, as evidence for the layout: the corpus, and the path it was
     *  opened on (the engine host hands a pane its `parent` and no `ctx.data` at all, and a path
     *  that already says `Era1/Strata3/Theorems` is a directory that exists). */
    const evidence = () => {
      const out = corpusFiles();
      const from = parentPath();
      if (from) out.push(from);
      return out;
    };

    /** The lists the selects offer, derived from the corpus's own paths (js/live/new-node-path.js). */
    let hierarchy = layoutOf([]);
    let lastEvidence = -1;

    /** The served project's id, when the app can answer for one (the engine host cannot). */
    function projectId() {
      try { return (ctx.projects && ctx.projects().active && ctx.projects().active.id) || ''; } catch (e) { return ''; }
    }

    /** The lists themselves, without asking for a repaint — `updateStrata` does that. */
    function layoutFor(files) {
      hierarchy = layoutOf(files);
      if (!eraSel) return;
      eraSel.innerHTML = hierarchy.eras.map(e => `<option value="${e}">${e}</option>`).join('');
    }

    function populateHierarchy(files) {
      layoutFor(files);
      if (!eraSel) return;
      updateStrata();
    }

    function updateStrata() {
      const strata = hierarchy.strataOf(eraSel.value);
      strataSel.innerHTML = strata.map(s => `<option value="${s}">${s}</option>`).join('');
      updatePreview();
    }

    function updatePreview() {
      /* THE CORPUS ARRIVES AFTER THE TILE DOES. A canvas tile is mounted before its app has fetched
         the corpus (`DATA.files` fills when the corpus tile's own ports are read, js/host/canvas/
         data.js::corpusFrom), and no port tells this pane that it happened — so the selects are
         rebuilt when the EVIDENCE has changed, which is the same thing the preview is about to be
         computed from. Without it the form of a freshly mounted tile offered one era and one strata
         the corpus did not have. */
      const files = evidence();
      if (files.length !== lastEvidence) { lastEvidence = files.length; layoutFor(files); }
      const result = planNode({
        files,
        era: eraSel.value || 'Era0',
        strata: strataSel.value || 'Strata0',
        dir: dirSel.value,
        projectId: projectId(),
        name: nameInp.value,
      });
      const { fullPath, nodeId } = result;

      previewEl.innerHTML = `File: <b style="color:var(--ink)">${fullPath}</b><br>Node ID: <b style="color:var(--ink)">${nodeId}</b>`;
      return result;
    }

    const parentPath = () => String(portParent.get() || '');

    /** Fill the form from the corpus this tile can see. The MODAL calls it on open (and clears
     *  the name); a tile calls it when its `parent` port arrives — which is why the name is only
     *  cleared when the caller asks, or a late path would wipe what the author has typed. */
    function prime({ resetName = true } = {}) {
      populateHierarchyFromParent();
      const parts = parentPath().split('/');
      if (parts.length >= 3) {
        const curEra = parts[0]; const curStrata = parts[1]; const curDir = parts[2];
        if (eraSel && Array.from(eraSel.options).some(o => o.value === curEra)) {
          eraSel.value = curEra;
          eraSel.dispatchEvent(new Event('change'));
        }
        if (strataSel && Array.from(strataSel.options).some(o => o.value === curStrata)) {
          strataSel.value = curStrata;
        }
        if (dirSel && Array.from(dirSel.options).some(o => o.value === curDir)) {
          dirSel.value = curDir;
        }
      }
      nameInp.value = '';
      updatePreview();
      /* THE SHOW/HIDE WAS A CLASS WITH NO RULE. `backdrop.classList.add('open')` toggled a name no
         stylesheet defines (nothing in web-next/ mentions `modal-backdrop`), over an inline
         `display: none` — so this pane's form could not be seen in EITHER host, whatever called
         `open()`. The display is set directly, and only where there is something to dismiss. */
      if (engineHost) {
      }
      if (resetName) nameInp.value = '';
      updatePreview();
    }

    function doOpen() {
      prime();
      if (engineHost) {
        backdrop.style.display = 'flex';
        nameInp.focus();
      }
    }

    function doClose() {
      if (engineHost) backdrop.style.display = 'none';
    }

    function populateHierarchyFromParent() {
      populateHierarchy(evidence());
    }

    function doCreate() {
      const result = updatePreview();
      if (result.cleanName === 'New') {
        // Show error - name required
        return;
      }

      /* THE BODY OF THE NEW NODE, and why it is what it is: js/live/new-node-plan.js::stubFor. */
      const tpl = stubFor(result);

      const written = (path) => {
        portNode.set(result.nodeId);
        portCreated.emit(path);
        /* the author gets the node they just made, in the editor */
        if (ctx.openFile) ctx.openFile(path);
      };

      /* WHOEVER OWNS THE ROUTE WRITES IT. In the canvas that is the app (js/app/node-services.js,
         which knows `POST /api/file` and answers with the server's own words when it refuses); in
         the engine host it is the socket endpoint, as before. A pane that assembled the URL itself
         would be a second place a route string lives. */
      if (ctx.createNode) {
        ctx.createNode({ path: result.fullPath, content: tpl }).then((path) => { if (path) written(path); });
        return;
      }
      try {
        const endpoint = ws.endpoint('file');
        endpoint.call.invoke({ path: result.fullPath, content: tpl }).then(() => {
          written(result.fullPath);
          ws.emit('files:changed', { path: result.fullPath });
          doClose();
        });
      } catch (err) {
        console.error('Create failed:', err);
      }
    }

    function mount(containerEl) {
      el = containerEl;
      el.style.cssText = `display: flex; flex-direction: column; height: 100%; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;`;

      // Backdrop
      backdrop = document.createElement('div');
      backdrop.id = 'newfile-backdrop';
      backdrop.className = 'modal-backdrop';
      backdrop.style.cssText = engineHost
        ? `
        position: fixed;
        inset: 0;
        background: rgba(0,0,0,0.6);
        display: none;
        align-items: center;
        justify-content: center;
        z-index: 1000;
        padding: 24px;
        box-sizing: border-box;
      `
        /* IN A TILE the form IS the tile: no backdrop, nothing to dismiss, and nothing fixed —
           a fixed element inside one tile would cover the whole canvas. */
        : 'position: static; display: block; padding: 0; box-sizing: border-box;';

      modal = document.createElement('div');
      modal.className = 'modal';
      modal.style.cssText = `
        background: var(--bg-panel);
        border: 1px solid var(--line);
        border-radius: 6px;
        padding: 24px;
        min-width: ${engineHost ? '420px' : '0'};
        max-width: 100%;
        max-height: 90vh;
        overflow-y: auto;
      `;

      // Header
      const h2 = document.createElement('h2');
      h2.textContent = '+ New Node';
      h2.style.cssText = 'margin: 0 0 8px 0; font-size: 18px; color: var(--ink);';

      const sub = document.createElement('div');
      sub.className = 'sub';
      sub.style.cssText = 'color: var(--muted); font-size: 11px; margin-bottom: 16px;';
      sub.textContent = 'Creates a new node where the corpus keeps them, with the next free ID and a TODO to fill in.';

      // Era/Strata/Dir row
      const row = document.createElement('div');
      row.style.cssText = 'display: flex; gap: 10px; margin-bottom: 12px;';

      const eraWrap = document.createElement('div');
      eraWrap.style.cssText = 'flex: 1;';
      const eraLabel = document.createElement('label');
      eraLabel.textContent = 'Era';
      eraLabel.style.cssText = 'display: block; margin-top: 0; font-size: 11px; color: var(--muted);';
      eraSel = document.createElement('select');
      eraSel.id = 'nf-era';
      eraSel.className = 'inp';
      eraSel.style.cssText = 'width: 100%;';
      eraSel.addEventListener('change', updateStrata);
      eraWrap.append(eraLabel, eraSel);

      const strataWrap = document.createElement('div');
      strataWrap.style.cssText = 'flex: 1;';
      const strataLabel = document.createElement('label');
      strataLabel.textContent = 'Strata';
      strataLabel.style.cssText = 'display: block; margin-top: 0; font-size: 11px; color: var(--muted);';
      strataSel = document.createElement('select');
      strataSel.id = 'nf-strata';
      strataSel.className = 'inp';
      strataSel.style.cssText = 'width: 100%;';
      strataSel.addEventListener('change', updatePreview);
      strataWrap.append(strataLabel, strataSel);

      const dirWrap = document.createElement('div');
      dirWrap.style.cssText = 'flex: 1;';
      const dirLabel = document.createElement('label');
      dirLabel.textContent = 'Category';
      dirLabel.style.cssText = 'display: block; margin-top: 0; font-size: 11px; color: var(--muted);';
      dirSel = document.createElement('select');
      dirSel.id = 'nf-dir';
      dirSel.className = 'inp';
      dirSel.style.cssText = 'width: 100%;';
      dirSel.innerHTML = `<option value="Theorems">Theorems</option><option value="Definitions">Definitions</option><option value="Axioms">Axioms</option>`;
      dirSel.addEventListener('change', updatePreview);
      dirWrap.append(dirLabel, dirSel);

      row.append(eraWrap, strataWrap, dirWrap);

      // Name input
      const nameLabel = document.createElement('label');
      nameLabel.textContent = 'Name (e.g. DeltaCancel)';
      nameLabel.style.cssText = 'display: block; margin-bottom: 4px; font-size: 11px; color: var(--muted);';
      nameInp = document.createElement('input');
      nameInp.id = 'nf-name';
      nameInp.type = 'text';
      nameInp.spellcheck = false;
      nameInp.autocomplete = 'off';
      nameInp.placeholder = 'NewThing';
      nameInp.style.cssText = `
        width: 100%;
        padding: 6px 8px;
        font-size: 11px;
        font-family: monospace;
        background: var(--bg-inset);
        border: 1px solid var(--line);
        border-radius: 4px;
        color: var(--ink);
      `;
      nameInp.addEventListener('input', updatePreview);

      // Preview
      previewEl = document.createElement('div');
      previewEl.id = 'nf-preview';
      previewEl.style.cssText = `
        margin-top: 12px;
        padding: 8px;
        background: var(--bg-inset);
        border: 1px solid var(--line);
        border-radius: 4px;
        font-family: monospace;
        font-size: 11px;
        color: var(--cyan);
        word-break: break-all;
      `;
      previewEl.textContent = 'Preview: ...';

      // Actions
      const actions = document.createElement('div');
      actions.className = 'modal-actions';
      actions.style.cssText = `display: flex; gap: 8px; justify-content: flex-end; margin-top: 16px;`;

      const cancelBtn = document.createElement('button');
      cancelBtn.className = 'btn ghost';
      cancelBtn.id = 'btn-nf-cancel';
      cancelBtn.textContent = 'Cancel';
      cancelBtn.style.cssText = `
        padding: 6px 16px;
        font-size: 11px;
        background: var(--bg-inset);
        border: 1px solid var(--line);
        border-radius: 4px;
        color: var(--ink);
        cursor: pointer;
      `;
      cancelBtn.addEventListener('click', doClose);

      const createBtn = document.createElement('button');
      createBtn.className = 'btn build';
      createBtn.id = 'btn-nf-create';
      createBtn.textContent = '⚡ Create';
      createBtn.style.cssText = `
        padding: 6px 16px;
        font-size: 11px;
        background: var(--cyan);
        border: none;
        border-radius: 4px;
        color: var(--ink);
        cursor: pointer;
        font-weight: 600;
      `;
      createBtn.addEventListener('click', doCreate);

      actions.append(cancelBtn, createBtn);

      modal.append(h2, sub, row, nameLabel, nameInp, previewEl, actions);
      backdrop.appendChild(modal);
      el.appendChild(backdrop);

      /* A MODAL'S MANNERS, IN THE HOST THAT HAS A MODAL. The Escape listener also had to go
         somewhere: it was added to `document` and never removed, so a closed tile kept a key
         handler for the life of the page. */
      /* THE FORM FILLS ITSELF FROM THE CORPUS IT CAN SEE: a value port replays on subscribe, so this
         runs with whatever `parent` the host has now — and again when the host sets one. IT GOES
         LAST, because priming needs the fields: at the top of `mount()` they do not exist yet, and
         the port swallows the throw. */
      offParent = portParent.subscribe(() => prime({ resetName: false }));

      if (engineHost) {
        backdrop.addEventListener('click', (e) => {
          if (e.target === backdrop) doClose();
        });
        onEscape = (e) => {
          if (e.key === 'Escape' && backdrop && backdrop.style.display === 'flex') doClose();
        };
        document.addEventListener('keydown', onEscape);
      }
    }

    return {
      mount,
      unmount() {
        if (offParent) { offParent(); offParent = null; }
        if (onEscape) { document.removeEventListener('keydown', onEscape); onEscape = null; }
        if (backdrop && backdrop.parentNode) backdrop.parentNode.removeChild(backdrop);
        el = null;
        modal = null;
        backdrop = null;
        eraSel = null;
        strataSel = null;
        dirSel = null;
        nameInp = null;
        previewEl = null;
      },
      in: {
        parent: portParent,
      },
      out: {
        node: portNode,
        created: portCreated,
      },
      call: {
        open: callOpen,
      },
    };
  },
};
