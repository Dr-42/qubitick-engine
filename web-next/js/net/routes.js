/**
 * net/routes.js — the client's one door to the server's URLs.
 *
 * No URL is written anywhere else. `js/defaults/endpoints.js` lists every route the server
 * has, this module asks for that table once at boot, and every request names a route
 * instead of spelling it — so a URL cannot drift from what is served, and a route that
 * changes is a change in one file that every caller follows.
 *
 * The table arrives asynchronously, and two things depend on the order:
 *
 *   · asking for a route BEFORE the table is in hand is not an error to be guessed at, it
 *     is a request to wait — `ready` is the promise, and the loaders that talk to the
 *     server before the first paint await it;
 *   · asking for a route the table does not have IS an error, and it says which name: a
 *     typo used to become `undefined` in a fetch and a request to the current page.
 *
 * Usage:
 *   const routes = createRoutes();
 *   await routes.ready;                 // once, at boot
 *   fetch(routes.url('files.list'))
 */

/** Load the endpoint table. A module-level import so tools/dead-modules.mjs sees it. */
const importEndpoints = () => import('../defaults/endpoints.js');

/**
 * @param {object} [o]
 * @param {() => Promise<object>} [o.load] how the table is loaded (injected by tests)
 * @param {(msg:string, err?:Error) => void} [o.warn]
 */
export function createRoutes({ load = importEndpoints, warn = (m, e) => console.warn(m, e) } = {}) {
  let table = {};
  let readyResolve;
  const ready = new Promise((res) => { readyResolve = res; });
  let settled = false;

  const loadRoutes = async () => {
    try {
      const m = await load();
      table = m.routes ? m.routes() : {};
    } catch (e) {
      warn('[routes]', e);
      table = {};
    }
    settled = true;
    readyResolve(table);
    return table;
  };

  /** A route by name. Throws — loudly — for a name the table does not have. */
  function url(name) {
    const path = table[name];
    if (!path) throw new Error('unknown route: ' + name);
    return path;
  }

  return {
    ready,
    url,
    load: loadRoutes,
    table: () => ({ ...table }),
    loaded: () => settled,
    has: (name) => !!table[name],
  };
}