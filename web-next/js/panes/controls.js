/**
 * panes/controls.js — Controls pane (M4.06).
 *
 * Ribbon of action buttons that emit bus events.
 *
 * Ported from index.html 61–94 (ribbon buttons).
 *
 * In:  {}
 * Out: { ui:build, ui:slate, ui:pdf, ui:handwrite, ui:symbol, ui:settings, ui:graph, ui:fullscreen }
 * Call: {}
 */

export default {
  kind: 'controls',
  ports: {
    in: {},
    out: {
      'ui:build': 'event',
      'ui:slate': 'event',
      'ui:pdf': 'event',
      'ui:handwrite': 'event',
      'ui:symbol': 'event',
      'ui:settings': 'event',
      'ui:graph': 'event',
      'ui:fullscreen': 'event',
    },
    call: {},
  },
  create(ctx) {
    const { ws, event } = ctx;
    let el = null;

    const buttons = [
      { id: 'build', icon: '⚡', label: 'Build', event: 'ui:build' },
      { id: 'slate', icon: '✎', label: 'Slate', event: 'ui:slate' },
      { id: 'pdf', icon: '⇩', label: 'PDF', event: 'ui:pdf' },
      { id: 'handwrite', icon: '✍', label: 'Handwrite', event: 'ui:handwrite' },
      { id: 'symbol', icon: 'λ', label: 'Symbol', event: 'ui:symbol' },
      { id: 'settings', icon: '⚙', label: 'Settings', event: 'ui:settings' },
      { id: 'graph', icon: '⬡', label: 'Graph', event: 'ui:graph' },
      { id: 'fullscreen', icon: '⛶', label: 'Fullscreen', event: 'ui:fullscreen' },
    ];

    function emitButtonEvent(eventName) {
      // Emit on the pane's own out port
      event(eventName).emit({ source: 'controls' });
      // Also emit on the workspace bus
      ws.emit(eventName, { source: 'controls' });
    }

    function toggleFullscreen() {
      if (!document.fullscreenElement) {
        const req = document.documentElement.requestFullscreen;
        if (req) {
          req.call(document.documentElement).catch((err) => {
            console.warn('Fullscreen denied:', err);
          });
        }
      } else {
        const exit = document.exitFullscreen;
        if (exit) exit.call(document);
      }
    }

    function mount(container) {
      el = document.createElement('div');
      el.className = 'controls-pane';
      el.style.cssText = `
        display: flex;
        flex-wrap: nowrap;
        gap: 4px;
        padding: 4px;
        overflow-x: auto;
        -webkit-overflow-scrolling: touch;
      `;

      buttons.forEach((btn) => {
        const button = document.createElement('button');
        button.className = 'controls-btn';
        button.id = 'btn-' + btn.id;
        button.type = 'button';
        button.title = btn.label;
        button.style.cssText = `
          display: flex;
          flex-direction: column;
          align-items: center;
          gap: 2px;
          padding: 6px 10px;
          min-width: 44px;
          min-height: 44px;
          font-size: 14px;
          color: #dbe2ea;
          background: #2a3140;
          border: 1px solid #4a5568;
          border-radius: 6px;
          cursor: pointer;
          flex-shrink: 0;
        `;
        button.addEventListener('mouseenter', () => { button.style.background = '#35405a'; });
        button.addEventListener('mouseleave', () => { button.style.background = '#2a3140'; });

        const iconSpan = document.createElement('span');
        iconSpan.textContent = btn.icon;
        iconSpan.style.fontSize = '16px';
        button.appendChild(iconSpan);

        const labelSpan = document.createElement('span');
        labelSpan.className = 'lbl';
        labelSpan.textContent = btn.label;
        labelSpan.style.fontSize = '11px';
        button.appendChild(labelSpan);

        button.addEventListener('click', () => {
          if (btn.event === 'ui:fullscreen') {
            toggleFullscreen();
          }
          emitButtonEvent(btn.event);
        });

        el.appendChild(button);
      });

      container.appendChild(el);
    }

    return {
      mount,
      unmount() {
        if (el && el.parentNode) el.parentNode.removeChild(el);
        el = null;
      },
      in: {},
      out: {},
      call: {},
    };
  },
};
