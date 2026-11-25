/**
 * panes/graph.js — Graph pane (M4.26).
 *
 * Canvas 2D renderer using graph3d.js for 3D physics/projection.
 * Pointer drag rotates; +/− buttons zoom; frame loop only while not
 * settled or dragging; sleeps at 0 fps when idle.
 *
 * Ported from app.js 1864–2155 (initGraph3D, drawGraph3D, event wiring).
 *
 * In:  `highlight` (value node id), `graphData` (value {nodes,edges})
 * Out: `selection` (value node id)
 * Call: `resetCamera`
 */

import { valuePort, eventPort, callPort } from '../core/ports.js';
import {
  createGraphState, stepPhysics, rotate, pan, zoomCam, project, hit,
  typeColor, shadeColor,
} from '../lib/graph3d.js';

export default {
  kind: 'graph',
  title: 'Graph',
  ports: {
    in: { highlight: 'value', graphData: 'value' },
    out: { selection: 'value' },
    call: { resetCamera: 'call' },
  },
  create(paneCtx) {
    const { ws, event } = paneCtx;
    let el = null;
    let canvas = null;
    let ctx = null;
    let inspectorWrap = null;
    let inspectorId = null;
    let inspectorName = null;
    let inspectorType = null;
    let inspectorBody = null;
    let state = null;
    let animFrame = null;
    let isDragging = false;
    let dragButton = -1;
    let lastMouseX = 0;
    let lastMouseY = 0;
    let currentPath = '';
    let rafScheduled = false;

    // Ports
    const portHighlight = valuePort(null);
    const portGraphData = valuePort(null);
    const portSelection = eventPort();
    const callResetCamera = callPort(doResetCamera);

    portHighlight.subscribe((nodeId) => {
      if (!state) return;
      state.selectedNode = nodeId ? state.nodes.find((n) => n.id === nodeId) : null;
      scheduleDraw();
    });

    portGraphData.subscribe((data) => {
      if (!data || !data.nodes) return;
      currentPath = '';
      state = createGraphState(data.nodes, data.edges, {
        repulsion: state?.physics?.repulsion ?? 40000,
        linkDist: state?.physics?.linkDist ?? 120,
        gravity: state?.physics?.gravity ?? 0.0015,
      });
      // Restore camera
      if (state) {
        state.rotX = -0.2;
        state.rotY = 0.5;
        state.zoom = 1.0;
        state.targetZoom = 1.0;
        state.offsetX = 0;
        state.offsetY = 0;
        state.targetOffsetX = 0;
        state.targetOffsetY = 0;
      }
      scheduleDraw();
      wakeLoop();
    });

    function esc(s) {
      return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
    }

    function wakeLoop() {
      if (rafScheduled) return;
      rafScheduled = true;
      if (typeof requestAnimationFrame !== 'undefined') {
        animFrame = requestAnimationFrame(loop);
      } else {
        setTimeout(loop, 16);
      }
    }

    function scheduleDraw() {
      wakeLoop();
    }

    function loop() {
      rafScheduled = false;
      if (!state || !ctx || !canvas) return;

      // Step physics unless settled and not dragging
      let settled = true;
      if (!state.settled || isDragging) {
        const result = stepPhysics(state, 16);
        settled = result.settled;
      }

      // Smooth camera interpolation
      state.zoom += (state.targetZoom - state.zoom) * 0.15;
      state.offsetX += (state.targetOffsetX - state.offsetX) * 0.15;
      state.offsetY += (state.targetOffsetY - state.offsetY) * 0.15;

      draw();

      // Continue loop if not settled or dragging
      if (!settled || isDragging) {
        wakeLoop();
      }
    }

    function draw() {
      if (!ctx || !canvas || !state) return;

      const w = canvas.width;
      const h = canvas.height;
      const T0 = { dark: true, ui: { acc: '#00ffff', bg: '#0a0a0f' } };

      ctx.clearRect(0, 0, w, h);

      // Background gradient
      const bgCenter = T0.dark ? shadeColor(T0.ui.bg, 5) : T0.ui.bg;
      const bgEdge = T0.dark ? '#000000' : shadeColor(T0.ui.bg, -5);
      const bgGrad = ctx.createRadialGradient(w / 2, h / 2, 0, w / 2, h / 2, Math.max(w, h));
      bgGrad.addColorStop(0, bgCenter);
      bgGrad.addColorStop(1, bgEdge);
      ctx.fillStyle = bgGrad;
      ctx.fillRect(0, 0, w, h);

      // Floor grid
      ctx.strokeStyle = T0.dark ? 'rgba(0, 255, 255, 0.15)' : 'rgba(0,0,0,0.08)';
      ctx.lineWidth = 1;
      const floorY = 400, gridSize = 2000, step = 100;
      const points = project(state, w, h);
      const pointMap = new Map(points.map((p) => [p.node, p]));

      for (let i = -gridSize; i <= gridSize; i += step) {
        const p1 = { x: w / 2 + i * state.zoom, y: h / 2 + 400, z: 0 };
        const p2 = { x: w / 2 + i * state.zoom, y: h / 2 - 400, z: 0 };
        ctx.beginPath();
        ctx.moveTo(p1.x, p1.y);
        ctx.lineTo(p2.x, p2.y);
        ctx.stroke();
        const p3 = { x: w / 2 - 400, y: h / 2 + i * state.zoom, z: 0 };
        const p4 = { x: w / 2 + 400, y: h / 2 + i * state.zoom, z: 0 };
        ctx.beginPath();
        ctx.moveTo(p3.x, p3.y);
        ctx.lineTo(p4.x, p4.y);
        ctx.stroke();
      }

      if (state.nodes.length === 0) return;

      ctx.globalCompositeOperation = T0.dark ? 'lighter' : 'source-over';

      // Draw edges
      state.edges.forEach((e) => {
        const p1 = pointMap.get(e.source);
        const p2 = pointMap.get(e.target);
        if (p1 && p2) {
          const depthAlpha = Math.max(0, Math.min(1, (1000 - (p1.z + p2.z) / 2) / 1500));
          ctx.strokeStyle = 'rgba(0, 255, 255, ' + depthAlpha * 0.6 + ')';
          ctx.lineWidth = 1.5;
          ctx.beginPath();
          ctx.moveTo(p1.x, p1.y);
          ctx.lineTo(p2.x, p2.y);
          ctx.stroke();
        }
      });

      // Draw nodes
      for (const p of points) {
        if (p.x < -100 || p.x > w + 100 || p.y < -100 || p.y > h + 100) continue;
        const n = p.node;
        const radius = Math.max(4, Math.min(20, 14 * p.scale));
        const isHovered = n === state.hoveredNode || n === state.selectedNode;

        if (T0.dark) {
          const auraGrad = ctx.createRadialGradient(p.x, p.y, radius * 0.2, p.x, p.y, radius * 2.5);
          auraGrad.addColorStop(0, n.color);
          auraGrad.addColorStop(1, 'rgba(0,0,0,0)');
          ctx.fillStyle = auraGrad;
          ctx.beginPath();
          ctx.arc(p.x, p.y, radius * 2.5, 0, Math.PI * 2);
          ctx.fill();
        } else {
          ctx.shadowBlur = isHovered ? 10 : 0;
          ctx.shadowColor = 'rgba(0,0,0,0.3)';
        }

        ctx.fillStyle = T0.dark ? '#ffffff' : n.color;
        ctx.beginPath();
        if (n.type === 'Theorem') {
          const r = radius * 0.9;
          ctx.moveTo(p.x, p.y - r);
          ctx.lineTo(p.x + r, p.y);
          ctx.lineTo(p.x, p.y + r);
          ctx.lineTo(p.x - r, p.y);
          ctx.closePath();
        } else if (n.type === 'Definition') {
          ctx.fillRect(p.x - radius * 0.7, p.y - radius * 0.7, radius * 1.4, radius * 1.4);
        } else {
          ctx.arc(p.x, p.y, radius * 0.6, 0, Math.PI * 2);
        }
        ctx.fill();
        ctx.shadowBlur = 0;

        ctx.strokeStyle = T0.dark ? n.color : '#000000';
        ctx.lineWidth = isHovered ? 3 : 2;
        ctx.stroke();

        if (isHovered || p.scale > 0.8) {
          ctx.globalCompositeOperation = 'source-over';
          ctx.font = "bold 11px 'JetBrains Mono', monospace";
          ctx.textAlign = 'center';
          ctx.textBaseline = 'bottom';
          const textWidth = ctx.measureText(n.label).width;
          ctx.fillStyle = T0.dark ? 'rgba(0, 10, 20, 0.9)' : '#ffffff';
          ctx.beginPath();
          ctx.roundRect(p.x - textWidth / 2 - 6, p.y - radius - 26, textWidth + 12, 20, 10);
          ctx.fill();
          if (!T0.dark) {
            ctx.strokeStyle = '#000000';
            ctx.lineWidth = 1;
            ctx.stroke();
          }
          ctx.fillStyle = T0.dark ? n.color : '#000000';
          ctx.fillText(n.label, p.x, p.y - radius - 10);
          ctx.globalCompositeOperation = T0.dark ? 'lighter' : 'source-over';
        }
      }

      ctx.globalCompositeOperation = 'source-over';
    }

    function doResetCamera() {
      if (!state) return { ok: false, error: 'no graph state' };
      state.rotX = -0.2;
      state.rotY = 0.5;
      state.targetZoom = 1.0;
      state.zoom = 1.0;
      state.offsetX = 0;
      state.offsetY = 0;
      state.targetOffsetX = 0;
      state.targetOffsetY = 0;
      state.settled = false;
      state.iterations = 0;
      wakeLoop();
      return { ok: true };
    }

    function mount(containerEl) {
      el = document.createElement('div');
      el.className = 'graph-pane';
      el.style.cssText = `display: flex; flex-direction: column; height: 100%; background: #0a0a0f;`;

      // Header
      const header = document.createElement('div');
      header.className = 'graph-header';
      header.style.cssText = `
        display: flex;
        align-items: center;
        gap: 6px;
        padding: 4px 8px;
        border-bottom: 1px solid #333;
      `;

      const title = document.createElement('div');
      title.textContent = 'Graph';
      title.style.cssText = 'font-weight: 600; font-size: 12px; color: #fff;';

      const zoomOutBtn = document.createElement('button');
      zoomOutBtn.textContent = '−';
      zoomOutBtn.title = 'Zoom out';
      zoomOutBtn.style.cssText = `padding: 2px 8px; font-size: 12px; background: #1a1a2e; border: 1px solid #333; color: #fff; border-radius: 4px; cursor: pointer;`;
      zoomOutBtn.addEventListener('click', () => { if (state) zoomCam(state, 1); wakeLoop(); });

      const zoomInBtn = document.createElement('button');
      zoomInBtn.textContent = '+';
      zoomInBtn.title = 'Zoom in';
      zoomInBtn.style.cssText = `padding: 2px 8px; font-size: 12px; background: #1a1a2e; border: 1px solid #333; color: #fff; border-radius: 4px; cursor: pointer;`;
      zoomInBtn.addEventListener('click', () => { if (state) zoomCam(state, -1); wakeLoop(); });

      const resetBtn = document.createElement('button');
      resetBtn.textContent = '↻ Reset';
      resetBtn.title = 'Reset camera';
      resetBtn.style.cssText = `margin-left: auto; padding: 2px 10px; font-size: 11px; background: #00ffff; border: none; border-radius: 4px; color: #000; cursor: pointer; font-weight: 600;`;
      resetBtn.addEventListener('click', doResetCamera);

      header.append(title, zoomOutBtn, zoomInBtn, resetBtn);
      el.appendChild(header);

      // Canvas area
      const canvasWrap = document.createElement('div');
      canvasWrap.className = 'graph-canvas-wrap';
      canvasWrap.style.cssText = `flex: 1; overflow: hidden; position: relative; cursor: grab;`;

      canvas = document.createElement('canvas');
      canvas.className = 'graph-canvas';
      canvas.style.cssText = `display: block; width: 100%; height: 100%;`;
      const dpr = window.devicePixelRatio || 1;
      canvas.width = canvasWrap.clientWidth || 400;
      canvas.height = canvasWrap.clientHeight || 300;
      canvas.width *= dpr;
      canvas.height *= dpr;
      ctx = canvas.getContext('2d');
      ctx.scale(dpr, dpr);

      canvasWrap.appendChild(canvas);
      el.appendChild(canvasWrap);

      // Inspector panel
      inspectorWrap = document.createElement('div');
      inspectorWrap.className = 'graph-inspector';
      inspectorWrap.style.cssText = `
        display: none;
        padding: 8px;
        border-top: 1px solid #333;
        background: #12141e;
        font-size: 11px;
        color: #ccc;
      `;

      inspectorId = document.createElement('div');
      inspectorId.style.cssText = 'font-family: monospace; font-weight: 600; margin-bottom: 2px;';
      inspectorName = document.createElement('div');
      inspectorName.style.cssText = 'margin-bottom: 2px;';
      inspectorType = document.createElement('div');
      inspectorType.style.cssText = 'color: #00ffff; margin-bottom: 4px;';
      inspectorBody = document.createElement('div');
      inspectorBody.style.cssText = 'font-family: monospace; font-size: 10px; color: #888;';

      inspectorWrap.appendChild(inspectorId);
      inspectorWrap.appendChild(inspectorName);
      inspectorWrap.appendChild(inspectorType);
      inspectorWrap.appendChild(inspectorBody);
      el.appendChild(inspectorWrap);

      // Mouse handlers
      canvas.addEventListener('mousedown', (e) => {
        if (e.button === 1) e.preventDefault();
        isDragging = true;
        dragButton = e.button;
        lastMouseX = e.clientX;
        lastMouseY = e.clientY;
        canvas.style.cursor = 'grabbing';
        wakeLoop();
      });

      window.addEventListener('mouseup', () => {
        isDragging = false;
        dragButton = -1;
        canvas.style.cursor = 'grab';
      });

      canvas.addEventListener('mousemove', (e) => {
        const rect = canvas.getBoundingClientRect();
        const mx = e.clientX - rect.left;
        const my = e.clientY - rect.top;

        if (!isDragging) {
          const pts = project(state, rect.width, rect.height);
          state.hoveredNode = hit(pts, mx, my);
          scheduleDraw();
          return;
        }

        const dx = e.clientX - lastMouseX;
        const dy = e.clientY - lastMouseY;
        lastMouseX = e.clientX;
        lastMouseY = e.clientY;

        if (dragButton === 0) {
          rotate(state, dx, dy);
        } else if (dragButton === 1 || dragButton === 2) {
          pan(state, dx, dy, state.zoom);
        }
        wakeLoop();
      });

      canvas.addEventListener('click', async (e) => {
        const rect = canvas.getBoundingClientRect();
        const mx = e.clientX - rect.left;
        const my = e.clientY - rect.top;
        const pts = project(state, rect.width, rect.height);
        const node = hit(pts, mx, my);
        if (node) {
          state.selectedNode = node;
          portSelection.emit({ line: 0, col: 0, target: 'lean', nodeId: node.id });
          ws.emit('graph:select', { nodeId: node.id });

          if (inspectorWrap) inspectorWrap.style.display = 'block';
          if (inspectorId) inspectorId.textContent = node.id;
          if (inspectorName) inspectorName.textContent = node.label;
          if (inspectorType) inspectorType.textContent = (node.type || 'unknown').toUpperCase();
          if (inspectorBody) inspectorBody.innerHTML = '<i>Loading node body...</i>';

          try {
            const qbitPath = node.file ? moduleToQbit(node.file) : currentPath;
            const buf = window.buffers ? window.buffers.get(qbitPath) : null;
            let txt = null;
            if (buf && typeof buf.content === 'string') { txt = buf.content; }
            else {
              // Use the registered `file.get` endpoint instead of a hardcoded route.
              if (ws && ws.endpoint) {
                const fileEp = ws.endpoint('file.get');
                const fdata = fileEp && fileEp.call && fileEp.call.invoke
                  ? await fileEp.call.invoke.invoke({ path: qbitPath })
                  : null;
                /* the server's shape: `{ok, path, content}` (crates/api/src/handlers_gap.rs) */
                txt = fdata ? (fdata.content || '') : '';
              }
            }
            const lines = txt.split('\n');
            let foundIdx = -1;
            for (let i = 0; i < lines.length; i++) {
              if (lines[i].includes(node.id)) { foundIdx = i; break; }
            }
            if (foundIdx >= 0 && inspectorBody) {
              const snippet = lines.slice(foundIdx, foundIdx + 6).join('<br>');
              inspectorBody.innerHTML = '<div style="font-family:monospace; font-size:10px;">' + esc(snippet) + '</div>';
            } else if (inspectorBody) {
              inspectorBody.innerHTML = '<i style="color:#f44">Node not found in file.</i>';
            }
          } catch (err) {
            if (inspectorBody) inspectorBody.innerHTML = '<i style="color:#f44">Failed to load: ' + esc(err.message) + '</i>';
          }
        }
        scheduleDraw();
      });

      containerEl.appendChild(el);
      scheduleDraw();
    }

    function moduleToQbit(file) {
      if (!file) return currentPath;
      if (file.endsWith('.qbit')) return file;
      return file + '.qbit';
    }

    return {
      mount,
      unmount() {
        if (animFrame) cancelAnimationFrame(animFrame);
        animFrame = null;
        if (el && el.parentNode) el.parentNode.removeChild(el);
        el = null;
        canvas = null;
        ctx = null;
        state = null;
      },
      in: {
        highlight: portHighlight,
        graphData: portGraphData,
      },
      out: {
        selection: portSelection,
      },
      call: {
        resetCamera: callResetCamera,
      },
    };
  },
};
