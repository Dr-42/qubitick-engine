// palette.js — Pane palette for design mode (M3.04)
import { newId } from "../core/ids.js";

const PREFIX_TABLE = {
  logo: "logo", controls: "ctl", nodes: "nodes", tree: "tree",
  editor: "ed", goal: "goal", ir: "ir", lean: "lean",
  render: "render", output: "out", diag: "diag", graph: "graph",
  slate: "slate", "symbol-add": "symadd", handwrite: "hw",
  "settings-theme": "theme", "settings-typography": "typo",
  "settings-layout": "setlay", "layout-json": "ljson",
  "layout-outline": "lout", "script-editor": "script",
  stats: "stats", "new-node": "newnode",
};

function prefixFor(kind) {
  for (const [pattern, prefix] of Object.entries(PREFIX_TABLE)) {
    if (pattern === "x-*") {
      if (kind.startsWith("x-")) return "x";
    } else if (kind === pattern) {
      return prefix;
    }
  }
  return kind.replace(/[^a-z0-9]/gi, "").slice(0, 4).toLowerCase() || "pane";
}

export function createPalette(opts) {
  const { root, registry, store, lock, getViewport, getRootRects, debug = false } = opts;
  const panel = document.createElement("div");
  panel.className = "palette";
  panel.style.cssText =
    "position:fixed; top:0; right:0; width:260px; height:100%;"
    + "background:#1c2130; border-left:1px solid #2a2f3e;"
    + "z-index:1400; display:flex; flex-direction:column;"
    + "box-sizing:border-box; padding:8px; overflow:hidden;";

  const search = document.createElement("input");
  search.type = "text";
  search.placeholder = "Search panes…";
  search.style.cssText =
    "width:100%; height:28px; padding:4px 8px; margin-bottom:8px;"
    + "background:#2a2f3e; border:1px solid #3b465e; border-radius:4px;"
    + "color:#d6e2ee; font-size:12px; box-sizing:border-box;";

  const list = document.createElement("div");
  list.className = "palette-list";
  list.style.cssText =
    "flex:1; overflow-y:auto; display:flex; flex-direction:column; gap:4px;";

  panel.appendChild(search);
  panel.appendChild(list);
  root.appendChild(panel);

  let open = false;

  function isUnlocked() {
    return lock && lock.get() === false;
  }

  function getKinds() {
    const all = registry ? registry.kinds() : [];
    return all.filter(k => {
      if (k === "x-probe" && !debug) return false;
      return true;
    });
  }

  function render() {
    list.innerHTML = "";
    const filter = search.value.toLowerCase();

    for (const kind of getKinds()) {
      if (filter && !kind.toLowerCase().includes(filter)) continue;

      const div = document.createElement("div");
      div.className = "palette-item";
      div.draggable = true;
      div.dataset.kind = kind;
      div.style.cssText =
        "display:flex; align-items:center; gap:6px; padding:6px 8px;"
        + "background:#2a2f3e; border:1px solid #3b465e; border-radius:4px;"
        + "color:#d6e2ee; font-size:12px; cursor:pointer; user-select:none;";

      const label = document.createElement("span");
      label.textContent = kind;
      label.style.cssText = "flex:1;";
      div.appendChild(label);

      div.addEventListener("click", (e) => {
        e.stopPropagation();
        if (!isUnlocked()) return;
        const vp = getViewport ? getViewport() : { w: 800, h: 600 };
        const id = newId(prefixFor(kind), new Set(Object.keys(store.get().panes || {})));
        const x = Math.max(0, (vp.w - 300) / 2);
        const y = Math.max(0, (vp.h - 200) / 2);
        store.beginGesture();
        store.dispatch({ type: "pane.add", id, kind });
        store.dispatch({ type: "tree.insert", pane: id, to: { newRoot: { id, x, y, w: 300, h: 200 } } });
        store.endGesture();
      });

      div.addEventListener("dragstart", (e) => {
        e.stopPropagation();
        e.dataTransfer.setData("text/plain", kind);
        e.dataTransfer.effectAllowed = "move";
        div.style.opacity = "0.5";
      });
      div.addEventListener("dragend", (e) => {
        e.stopPropagation();
        div.style.opacity = "1";
      });

      list.appendChild(div);
    }
  }

  function update() {
    const shouldBeOpen = open && isUnlocked();
    panel.style.display = shouldBeOpen ? "flex" : "none";
    if (shouldBeOpen) render();
  }

  function toggle() {
    if (!isUnlocked()) return;
    open = !open;
    update();
  }

  function close() {
    open = false;
    update();
  }

  function isOpen() {
    return open;
  }

  search.addEventListener("input", () => render());

  function onKeyDown(e) {
    if (e.altKey && e.key.toLowerCase() === "p") {
      e.preventDefault();
      toggle();
    }
    if (e.key === "Escape" && open) {
      e.preventDefault();
      close();
    }
  }
  document.addEventListener("keydown", onKeyDown);

  // Listen for palette-toggle button from design bar
  root.addEventListener("ui:palette-toggle", () => toggle());

  panel.addEventListener("dragover", (e) => e.preventDefault());
  panel.addEventListener("drop", (e) => e.preventDefault());

  update();

  return {
    destroy() {
      document.removeEventListener("keydown", onKeyDown);
      root.removeEventListener("ui:palette-toggle", toggle);
      panel.remove();
    },
    toggle,
    isOpen,
    close,
  };
}

export function paletteDrop(store, kind, to) {
  if (!store || !kind || !to) return false;
  const existingIds = new Set(Object.keys(store.get().panes || {}));
  const id = newId(prefixFor(kind), existingIds);
  store.beginGesture();
  const r1 = store.dispatch({ type: "pane.add", id, kind });
  if (!r1.ok) {
    store.endGesture();
    return false;
  }
  const r2 = store.dispatch({ type: "tree.insert", pane: id, to });
  store.endGesture();
  return r2.ok;
}
