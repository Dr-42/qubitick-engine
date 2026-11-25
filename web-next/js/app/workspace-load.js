/*
 * workspace-load.js — which saved workspace wins.
 *
 * Extracted from the inline script in web-next/index.html. The workspace exists in three
 * places at once — the local mirror, the server document, and the legacy single-project
 * shape — and a reload has to pick, quickly and correctly, because the wrong pick is a
 * layout that silently reverts.
 *
 * The rules, and why each one is not obvious:
 *
 *   LOCAL FIRST     the mirror is applied immediately, before the request goes out, so a
 *                   reload looks like nothing happened even on a slow connection. The
 *                   server copy is then adopted only if it is NEWER.
 *   NEWER WINS      by `savedAt`. A server document from an older session must not
 *                   overwrite what this browser just did.
 *   A MIRROR HAS A PROJECT   the mirror is one document, and it belongs to one project. A
 *                   mirror for a DIFFERENT project is not this project's layout, so it is
 *                   ignored — otherwise switching projects inherits the other corpus's tiles.
 *   KEYED OR BARE   `byProject` holds every project's workspace; a bare document predates
 *                   that and belongs to whichever project is active.
 *   SAFE MODE       restores nothing at all. That is what safe mode means: look, do not
 *                   touch. The default workspace the boot built is the point.
 */

/**
 * @param {object} o
 * @param {object|null} o.local        the mirrored document
 * @param {object|null} o.server       the server's `settings.workspace` (keyed or bare)
 * @param {object|null} o.legacy       `settings.layout` — the pre-workspace shape
 * @param {string} o.projectKey        which project is being served
 * @param {boolean} [o.safe]
 * @returns {{ local: object|null, remote: object|null, byProject: object|null, reason: string }}
 *          `local` is what to apply immediately, `remote` what to adopt if it is newer (the
 *          caller applies it only when it is), `byProject` the table of every project.
 */
export function chooseWorkspace({ local = null, server = null, legacy = null, projectKey, safe = false }) {
  if (safe) return { local: null, remote: null, byProject: null, reason: 'safe mode restores nothing' };

  // a mirror belongs to one project, and it is not this one
  let mine = local;
  let why = mine ? 'local' : 'none';
  if (mine && mine.project && mine.project !== projectKey) {
    mine = null;
    why = 'local belongs to another project';
  }

  // the server may hold a table of every project, or one bare document
  const byProject = (server && server.byProject) ? server.byProject : null;
  const fromServer = byProject ? (byProject[projectKey] || null) : (server || null);

  // the legacy shape: a bare layout with no workspace wrapper
  const candidates = [fromServer, legacy].filter(Boolean);
  candidates.sort((a, b) => (b.savedAt || 0) - (a.savedAt || 0));
  const remote = candidates[0] || null;

  // adopt the remote only when it is newer than what we just applied
  const adopt = remote && (!mine || (remote.savedAt || 0) > (mine.savedAt || 0));
  /* The reason names the CASE, and there are four: a mirror on its own, a mirror that beat
     the server, a server copy that beat the mirror, and a server copy with no mirror to beat.
     Saying "server is newer" when there was no mirror at all would be true and useless. */
  const reason = adopt
    ? (mine ? 'server is newer' : 'server only')
    : (mine ? 'mirror is newer' : 'nothing saved');
  return { local: mine, remote: adopt ? remote : null, byProject, reason };
}