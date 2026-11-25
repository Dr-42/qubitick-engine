export function createStorage({
  load,
  save,
  local,
  debounceMs = 400,
  retries = 3,
  retryMs = 1000,
  timers = { setTimeout, clearTimeout },
} = {}) {
  const { setTimeout, clearTimeout } = timers;
  let data = {};
  let mode_ = 'server';
  let debounceTimer = null;
  let pendingSave = false;
  let savePromise = Promise.resolve();

  const ready = (async () => {
    try {
      const loaded = await load();
      if (loaded && typeof loaded === 'object' && !Array.isArray(loaded)) {
        data = { ...loaded };
        mode_ = 'server';
      } else {
        mode_ = 'local';
        const raw = local.getItem('qbx:settings');
        if (raw) {
          try {
            data = { ...JSON.parse(raw) };
          } catch {
            data = {};
          }
        }
      }
    } catch {
      mode_ = 'local';
      const raw = local.getItem('qbx:settings');
      if (raw) {
        try {
          data = { ...JSON.parse(raw) };
        } catch {
          data = {};
        }
      }
    }
  })();

  function mirrorToLocal() {
    if (mode_ === 'local' || true) {
      try {
        local.setItem('qbx:settings', JSON.stringify(data));
      } catch {
        // localStorage may be unavailable
      }
    }
  }

  async function saveWithRetry(obj) {
    let lastError = null;
    for (let i = 0; i <= retries; i++) {
      try {
        await save(obj);
        return;
      } catch (err) {
        lastError = err;
        if (i < retries) {
          await new Promise((resolve) => setTimeout(resolve, retryMs));
        }
      }
    }
    console.error('save failed after retries', lastError);
  }

  function get(key) {
    return ready.then(() => data[key]);
  }

  function put(key, value) {
    data = { ...data, [key]: value };
    mirrorToLocal();

    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(async () => {
      debounceTimer = null;
      if (pendingSave) return;
      pendingSave = true;
      try {
        await saveWithRetry({ ...data });
      } finally {
        pendingSave = false;
      }
    }, debounceMs);

    return ready.then(() => undefined);
  }

  function snapshot() {
    return { ...data };
  }

  async function flush() {
    await ready;
    if (debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    }
    if (pendingSave) {
      await savePromise;
    } else {
      await saveWithRetry({ ...data });
    }
  }

  function mode() {
    return mode_;
  }

  return { ready, get, put, snapshot, flush, mode };
}
/**
 * The browser MIRROR: one key, parsed JSON, and a setter that refuses to throw.
 *
 * Separate from `createStorage` on purpose. That one is a settings document with server/local
 * arbitration, retries and a mode; this one is the "restore the workspace instantly on reload"
 * copy — a single key written before the request and read before the server answers. Both touch
 * browser storage, so both live in this module: the lint rule that keeps `localStorage` out of
 * every other file is only honest if the one place that owns it is a real place.
 *
 * @param {string} key
 * @param {object} [store] defaults to the browser's
 */
export function createMirror(key, store) {
  const s = store || (typeof localStorage !== 'undefined' ? localStorage : null);
  return {
    key,
    get() {
      try { const raw = s && s.getItem(key); return raw ? JSON.parse(raw) : null; }
      catch (e) { return null; }
    },
    set(key2, value) {
      try { if (s) s.setItem(key2, value); } catch (e) { /* quota, private mode: the mirror is a convenience */ }
    },
    remove(key2) {
      try { if (s) s.removeItem(key2); } catch (e) { /* already gone */ }
    },
  };
}
