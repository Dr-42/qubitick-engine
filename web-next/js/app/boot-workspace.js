/*
 * app/boot-workspace.js — the two-pass restore, and the order it has to happen in.
 *
 * Extracted from the inline script in web-next/index.html. The rules are small and all of
 * them are about ORDER, which is why they were easy to get wrong in a page where the code
 * was interleaved with the boot:
 *
 *   1. the ROUTE table first       nothing can be fetched before the routes are in hand;
 *   2. the PROJECT list second     the workspace is keyed by project, so a project that
 *                                  arrives late picks up the wrong one's tiles;
 *   3. SAFE MODE restores nothing  the default workspace the boot built is the point of it;
 *   4. the LOCAL MIRROR applies at once — that is what makes a reload look like nothing
 *      happened, and it is the only thing that works against a server that predates the
 *      workspace endpoint;
 *   5. the SERVER document is adopted only if it is NEWER (every save carries `savedAt`),
 *      so a stale server copy cannot undo work done in this session;
 *   6. the ACTIVE FILE's text is fetched after the layout names it — a restore names a file
 *      long before its body has ever been read;
 *   7. the console tiles are made LAST, because a layout saved before the console became
 *      tiles has none, and a workspace must always be able to control itself.
 *
 * A failure at any of the network steps is not fatal: an old server simply answers nothing
 * and the mirror stands.
 */

import { chooseWorkspace } from './workspace-load.js';

/**
 * @param {object} host
 * @param {Promise<any>} host.routesReady        resolves when the route table is in hand
 * @param {() => Promise<any>} host.refreshProjects
 * @param {() => string} host.projectKey
 * @param {boolean} host.safe
 * @param {() => object|null} host.mirror        the local mirror document, already parsed
 * @param {(doc:object) => void} host.apply      apply a workspace document
 * @param {(table:object) => void} host.setLayouts   remember the per-project table
 * @param {(name:string) => string} host.url
 * @param {(u:string, init?:object) => Promise<object>} host.fetch
 * @param {() => object|null} host.activeDoc
 * @param {(path:string) => Promise<any>} host.loadText
 * @param {(path:string) => void} host.setActive
 * @param {() => void} host.ensureConsole
 * @param {() => number} [host.paneCount]
 * @param {(o:object) => void} [host.diag]       the boot diagnostics object
 */
export async function bootWorkspace(host) {
  const diag = (o) => { if (host.diag) host.diag(o); };
  const panes = () => (host.paneCount ? host.paneCount() : 0);

  await host.routesReady;
  await host.refreshProjects();

  // safe mode restores nothing: the default workspace the boot built is the point
  if (host.safe) {
    diag({ safe: true, restored: false });
    host.ensureConsole();
    return { restored: false, reason: 'safe mode' };
  }

  let local = host.mirror ? host.mirror() : null;
  diag({
    local: !!local,
    localPanes: local && local.layout ? ((local.layout.roots || []).length + 'r/' + (local.layout.free || []).length + 'f') : undefined,
    panesBefore: panes(),
  });
  const first = chooseWorkspace({ local, projectKey: host.projectKey(), safe: false });
  local = first.local;
  if (local) host.apply(local);
  diag({ panesAfterLocal: panes() });

  let reason = first.reason;
  try {
    const r = await host.fetch(host.url('settings.get'));
    if (r && r.ok) {
      const j = (await r.json()) || {};
      const settings = j.settings || {};
      const remoteDoc = settings.workspace || null;
      // a keyed document holds every project's workspace; a bare one is the older,
      // single-project shape and belongs to whichever project is active
      if (remoteDoc && remoteDoc.byProject) {
        host.setLayouts(remoteDoc.byProject);
        diag({ projects: Object.keys(remoteDoc.byProject).length });
      }
      // the legacy shape: a bare layout, no workspace wrapper
      const legacy = settings.layout ? { layout: settings.layout } : null;
      const pick = chooseWorkspace({ local, server: remoteDoc, legacy, projectKey: host.projectKey(), safe: false });
      diag({ remote: !!pick.remote, remoteSavedAt: pick.remote && pick.remote.savedAt, pick: pick.reason });
      reason = pick.reason;
      if (pick.remote) {
        host.apply(pick.remote);
        diag({ afterRemote: panes() });
      }
    }
  } catch (e) { /* an older server, or none: the mirror stands */ }

  // the restored active file may have been named before its text was ever fetched
  const active = host.activeDoc && host.activeDoc();
  if (active) {
    if (host.loadText) await host.loadText(active.path);
    host.setActive(active.path);
  }
  // a layout saved before the console became tiles has none: give it them
  host.ensureConsole();
  /* the same rule for the corpus: a workspace with no project/corpus pair would boot with an
     empty interface and no way to fill it (js/world/home.js::ensureCorpus) */
  if (host.ensureCorpus) host.ensureCorpus();
  return { restored: true, reason };
}