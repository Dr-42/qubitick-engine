export function debugEnabled(search) {
  const params = new URLSearchParams(search.startsWith('?') ? search.slice(1) : search);
  return params.get('debug') === '1';
}

export function installDebug(window, api) {
  if (debugEnabled(window.location.search)) {
    window.__ws = api;
  }
}

export const typeCheckEnabled = debugEnabled(typeof location !== 'undefined' ? location.search : '');