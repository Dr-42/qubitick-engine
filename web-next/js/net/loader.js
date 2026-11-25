import { createScope } from '../core/scope.js';
import { builtInWiring } from '../defaults/wiring/index.js';

/**
 * Script loader for wiring overrides and custom panes.
 *
 * @param {object} deps
 * @param {object} deps.ws - workspace API (for scoped() and emit)
 * @param {object} deps.storage - storage adapter (getItem, setItem)
 * @param {object} deps.scope - parent scope
 * @param {boolean} deps.safe - safe-boot mode
 * @param {function} deps.importModule - dynamic import function
 * @param {string} deps.url - base URL for wiring scripts
 */
export function createLoader({
  ws,
  storage,
  scope,
  safe = false,
  importModule,
  url = '/js/defaults/wiring/',
} = {}) {
  const loadedScripts = new Map(); // name -> { scope, url }

  /**
   * Which module a wiring name resolves to.
   *
   * A preset that SHIPS is in the table (js/defaults/wiring/index.js): three names, three
   * statically imported modules, and the tree can see all of them. Building the path
   * instead (`<url><name>.js`) works in a browser and hides the edge from everything else —
   * the five preset modules were loaded on every boot and looked like modules nothing loads.
   *
   *  1. If !safe and storage has key `wiring:<name>` → import from blob (a user's own script,
   *     and it may shadow a shipped name: that is what the key is for)
   *  2. Else if the name is a built-in → the table
   *  3. Else → the URL the caller configured, which is how a script from elsewhere is loaded
   *
   * @returns {Promise<{module: object, url: string}>}
   */
  async function resolveWiring(name) {
    if (!safe && storage) {
      const stored = storage.getItem(`wiring:${name}`);
      if (stored && typeof stored === 'string') {
        const blobUrl = URL.createObjectURL(new Blob([stored], { type: 'text/javascript' }));
        try {
          return { module: await importModule(blobUrl), url: blobUrl };
        } finally {
          URL.revokeObjectURL(blobUrl);
        }
      }
    }
    const builtIn = builtInWiring(name);
    if (builtIn) return { module: { default: builtIn }, url: url + name + '.js' };
    const moduleUrl = url + name + '.js';
    return { module: await importModule(moduleUrl + '?v=' + Date.now()), url: moduleUrl };
  }

  async function loadWiring(name) {
    if (loadedScripts.has(name)) {
      return loadedScripts.get(name);
    }

    const childScope = createScope();
    scope.add(() => childScope.dispose());

    let module;
    let moduleUrl;

    try {
      const resolved = await resolveWiring(name);
      module = resolved.module;
      moduleUrl = resolved.url;

      module.default(ws.scoped(childScope));
    } catch (err) {
      childScope.dispose();
      if (ws && ws.emit) {
        ws.emit('script:error', { name, error: err });
      }
      console.error(`Error loading wiring '${name}':`, err);
      throw err;
    }

    loadedScripts.set(name, { scope: childScope, url: moduleUrl });
    return module;
  }

  /**
   * Load custom panes from storage keys `customPane:*`.
   * Returns empty array if safe mode.
   */
  async function loadCustomPanes() {
    if (safe) {
      return [];
    }

    const panes = [];
    const storageKeys = (storage.keys ? storage.keys() : Object.keys(storage || {})).filter((k) => k.startsWith('customPane:'));

    for (const key of storageKeys) {
      const name = key.slice('customPane:'.length);
      const stored = storage.getItem(key);
      if (!stored || typeof stored !== 'string') {
        continue;
      }

      const childScope = createScope();
      scope.add(() => childScope.dispose());

      try {
        const blob = new Blob([stored], { type: 'text/javascript' });
        const blobUrl = URL.createObjectURL(blob);
        try {
          const module = await importModule(blobUrl);
          module.default(ws.scoped(childScope));
          panes.push({ name, scope: childScope, url: blobUrl });
        } finally {
          URL.revokeObjectURL(blobUrl);
        }
      } catch (err) {
        childScope.dispose();
        if (ws && ws.emit) {
          ws.emit('script:error', { name, error: err });
        }
        console.error(`Error loading custom pane '${name}':`, err);
      }
    }

    return panes;
  }

  /**
   * Reload a wiring script by name.
   * Disposes the previous scope first, then loads fresh with cache-bust.
   */
  async function reload(name) {
    const existing = loadedScripts.get(name);
    if (existing) {
      existing.scope.dispose();
      loadedScripts.delete(name);
    }

    // Cache-bust by appending a new timestamp
    const cacheBustedUrl = (existing ? existing.url : url + name + '.js') + '?v=' + Date.now();

    const childScope = createScope();
    scope.add(() => childScope.dispose());

    let module;
    try {
      /* the same resolution as the first load: a stored override is re-read, a shipped
         preset comes from the table (and does not need a cache-busting query at all) */
      module = (await resolveWiring(name)).module;
      module.default(ws.scoped(childScope));
    } catch (err) {
      childScope.dispose();
      if (ws && ws.emit) {
        ws.emit('script:error', { name, error: err });
      }
      console.error(`Error reloading wiring '${name}':`, err);
      throw err;
    }

    loadedScripts.set(name, { scope: childScope, url: cacheBustedUrl });
    return module;
  }

  /**
   * Dispose all loaded scripts and their scopes.
   */
  function disposeAll() {
    for (const entry of loadedScripts.values()) {
      entry.scope.dispose();
    }
    if (scope && typeof scope.dispose === 'function') {
      scope.dispose(); // cascades disposal to every loaded script's child scope
    }
    loadedScripts.clear();
  }

  /**
   * List all loaded script names.
   */
  function list() {
    return [...loadedScripts.keys()];
  }

  return { loadWiring, loadCustomPanes, reload, disposeAll, list };
}