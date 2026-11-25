/**
 * panes/logo.js — Logo pane (M4.05).
 *
 * Compact pane showing file path + connection badge.
 * Fits a 48 px HUD.
 *
 * Ported from index.html 27–60 (brand/filepath area) and app.js 2933–2940 (setConnBadge).
 *
 * In:  { file: value (path string), connection: value ('online'|'offline'|'connecting') }
 * Out: {}
 * Call: {}
 */

export default {
  kind: 'logo',
  title: 'Logo',
  ports: {
    in: { file: 'value', connection: 'value' },
    out: {},
    call: {},
  },
  create(ctx) {
    let el = null;
    let fileEl = null;
    let badgeEl = null;
    let currentFile = '';
    let currentConn = 'connecting';

    function render() {
      if (!el) return;
      if (fileEl) {
        fileEl.textContent = currentFile || '—';
        fileEl.title = currentFile || 'no file';
      }
      if (badgeEl) {
        badgeEl.textContent = connLabel(currentConn);
        badgeEl.className = 'logo-badge ' + currentConn;
      }
    }

    function connLabel(conn) {
      switch (conn) {
        case 'online': return 'live';
        case 'offline': return 'offline · mock';
        case 'connecting':
        default: return 'connecting…';
      }
    }

    function mount(container) {
      el = document.createElement('div');
      el.className = 'logo-pane';
      el.style.cssText = `
        display: flex;
        align-items: center;
        gap: 6px;
        padding: 4px 8px;
        font-size: 11px;
        font-family: monospace;
        height: 40px;
        box-sizing: border-box;
      `;

      const icon = document.createElement('span');
      icon.textContent = '▣';
      icon.style.cssText = 'font-size: 14px;';
      el.appendChild(icon);

      fileEl = document.createElement('span');
      fileEl.className = 'logo-file';
      fileEl.style.cssText = 'overflow: hidden; text-overflow: ellipsis; white-space: nowrap;';
      fileEl.textContent = currentFile || '—';
      el.appendChild(fileEl);

      badgeEl = document.createElement('span');
      badgeEl.className = 'logo-badge ' + currentConn;
      badgeEl.style.cssText = `
        padding: 1px 6px;
        border-radius: 3px;
        font-size: 10px;
        font-weight: 600;
      `;
      badgeEl.textContent = connLabel(currentConn);
      el.appendChild(badgeEl);

      container.appendChild(el);
      render();
    }

    return {
      mount,
      unmount() {
        if (el && el.parentNode) el.parentNode.removeChild(el);
        el = null;
        fileEl = null;
        badgeEl = null;
      },
      in: {
        file(v) {
          currentFile = String(v || '');
          render();
        },
        connection(v) {
          currentConn = String(v || 'connecting');
          render();
        },
      },
      out: {},
      call: {},
    };
  },
};
