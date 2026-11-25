/**
 * lib/graph3d.js — Pure 3D graph physics library (M4.25).
 *
 * Pure functions only — no DOM, no canvas, no rAF.
 * Ported from app.js: typeColor, shadeColor, findNodeAt,
 * layoutGraph3D physics step, project3D.
 */

const DEFAULT_PHYSICS = { repulsion: 40000, linkDist: 120, gravity: 0.0015 };
const DEFAULT_THEME = {
  cy: { th: '#00ffff', def: '#ff00ff', ax: '#00ff00' },
  tokens: { number: '#ffff00' },
  ui: { acc: '#00ffff', bg: '#0a0a0f', dark: true },
  dark: true,
};

export function typeColor(type, theme) {
  const T = theme || DEFAULT_THEME;
  const c = T.cy || {};
  if (type === 'Theorem') return c.th || '#00ffff';
  if (type === 'Definition') return c.def || '#ff00ff';
  if (type === 'Calibration') return (T.tokens && T.tokens.number) || '#ffff00';
  return c.ax || '#00ff00';
}

export function shadeColor(hex, percent) {
  let R = parseInt(hex.substring(1, 3), 16);
  let G = parseInt(hex.substring(3, 5), 16);
  let B = parseInt(hex.substring(5, 7), 16);
  R = parseInt(R * (100 + percent) / 100);
  G = parseInt(G * (100 + percent) / 100);
  B = parseInt(B * (100 + percent) / 100);
  R = R < 255 ? R : 255;
  G = G < 255 ? G : 255;
  B = B < 255 ? B : 255;
  return '#' + R.toString(16).padStart(2, '0') + G.toString(16).padStart(2, '0') + B.toString(16).padStart(2, '0');
}

function mixHex(a, b, t) {
  const ra = parseInt(a.substring(1, 3), 16);
  const ga = parseInt(a.substring(3, 5), 16);
  const ba = parseInt(a.substring(5, 7), 16);
  const rb = parseInt(b.substring(1, 3), 16);
  const gb = parseInt(b.substring(3, 5), 16);
  const bb = parseInt(b.substring(5, 7), 16);
  const r = Math.round(ra + (rb - ra) * t);
  const g = Math.round(ga + (gb - ga) * t);
  const bb2 = Math.round(ba + (bb - ba) * t);
  return '#' + r.toString(16).padStart(2, '0') + g.toString(16).padStart(2, '0') + bb2.toString(16).padStart(2, '0');
}

export function createGraphState(nodes, edges, physics) {
  const P = Object.assign({}, DEFAULT_PHYSICS, physics || {});
  const nodeMap = new Map();
  const gNodes = (nodes || []).map((n) => {
    const id = n.id || (n.data && n.data.id);
    const label = n.label || (n.data && n.data.label) || id;
    const type = n.type || (n.data && n.data.type) || 'unknown';
    const file = n.file || (n.data && n.data.file);
    nodeMap.set(id, { id, label, type, file });
    return {
      id, label, type, color: typeColor(type, DEFAULT_THEME), file,
      x: (Math.random() - 0.5) * 200, y: (Math.random() - 0.5) * 200, z: (Math.random() - 0.5) * 200,
      vx: 0, vy: 0, vz: 0,
    };
  });
  const gEdges = (edges || []).filter((e) => {
    const src = nodeMap.get(e.source || (e.data && e.data.source));
    const tgt = nodeMap.get(e.target || (e.data && e.data.target));
    return src && tgt;
  }).map((e) => ({
    source: nodeMap.get(e.source || (e.data && e.data.source)),
    target: nodeMap.get(e.target || (e.data && e.data.target)),
  }));
  return {
    physics: P, nodes: gNodes, edges: gEdges,
    rotX: -0.2, rotY: 0.5, zoom: 1.0, targetZoom: 1.0,
    offsetX: 0, offsetY: 0, targetOffsetX: 0, targetOffsetY: 0,
    hoveredNode: null, selectedNode: null,
    isDragging: false, dragButton: -1,
    lastMouseX: 0, lastMouseY: 0,
    animFrame: null, settled: false, iterations: 0, energy: 0,
  };
}

const REPULSION_THRESHOLD_SQ = 100000;
const LINK_FORCE_FACTOR = 0.05;
const DRAG = 0.85;
const CLAMP = 800;
const MAX_ITERATIONS = 80;
const SETTLE_ENERGY_THRESHOLD = 0.01;
const FOCAL = 1000;

export function stepPhysics(state, dt) {
  const { nodes, edges, physics } = state;
  state.iterations++;
  for (let i = 0; i < nodes.length; i++) {
    for (let j = i + 1; j < nodes.length; j++) {
      const a = nodes[i];
      const b = nodes[j];
      let dx = a.x - b.x, dy = a.y - b.y, dz = a.z - b.z;
      let distSq = dx * dx + dy * dy + dz * dz + 0.1;
      if (distSq < REPULSION_THRESHOLD_SQ) {
        let force = physics.repulsion / distSq;
        let dist = Math.sqrt(distSq);
        a.vx += (dx / dist) * force;
        a.vy += (dy / dist) * force;
        a.vz += (dz / dist) * force;
        b.vx -= (dx / dist) * force;
        b.vy -= (dy / dist) * force;
        b.vz -= (dz / dist) * force;
      }
    }
  }
  for (const e of edges) {
    const a = e.source;
    const b = e.target;
    let dx = a.x - b.x, dy = a.y - b.y, dz = a.z - b.z;
    let dist = Math.sqrt(dx * dx + dy * dy + dz * dz + 0.1);
    let force = (dist - physics.linkDist) * LINK_FORCE_FACTOR;
    a.vx -= (dx / dist) * force;
    a.vy -= (dy / dist) * force;
    a.vz -= (dz / dist) * force;
    b.vx += (dx / dist) * force;
    b.vy += (dy / dist) * force;
    b.vz += (dz / dist) * force;
  }
  let energy = 0;
  for (const n of nodes) {
    n.vx -= n.x * physics.gravity;
    n.vy -= n.y * physics.gravity;
    n.vz -= n.z * physics.gravity;
    n.x += n.vx; n.y += n.vy; n.z += n.vz;
    n.vx *= DRAG; n.vy *= DRAG; n.vz *= DRAG;
    n.x = Math.max(-CLAMP, Math.min(CLAMP, n.x));
    n.y = Math.max(-CLAMP, Math.min(CLAMP, n.y));
    n.z = Math.max(-CLAMP, Math.min(CLAMP, n.z));
    energy += Math.abs(n.vx) + Math.abs(n.vy) + Math.abs(n.vz);
  }
  state.energy = energy;
  state.settled = state.iterations >= MAX_ITERATIONS || energy < SETTLE_ENERGY_THRESHOLD;
  return { energy, settled: state.settled };
}

export function rotate(state, dx, dy) {
  state.rotY += dx * 0.005;
  state.rotX += dy * 0.005;
}

export function pan(state, dx, dy, zoom) {
  state.targetOffsetX += dx / zoom;
  state.targetOffsetY += dy / zoom;
}

export function zoomCam(state, deltaY) {
  state.targetZoom = Math.max(0.25, Math.min(5.0, state.targetZoom * (deltaY > 0 ? 0.9 : 1.1)));
}

export function project(state, w, h) {
  const { rotX, rotY, zoom, offsetX, offsetY } = state;
  const cosY = Math.cos(rotY), sinY = Math.sin(rotY);
  const cosX = Math.cos(rotX), sinX = Math.sin(rotX);
  const focal = FOCAL;
  const points = state.nodes.map((n) => {
    let x1 = n.x * cosY - n.z * sinY;
    let z1 = n.x * sinY + n.z * cosY;
    let y1 = n.y * cosX - z1 * sinX;
    let z2 = n.y * sinX + z1 * cosX;
    z2 = Math.max(z2, -focal + 100);
    const scale = (focal / (focal + z2)) * zoom;
    return { node: n, x: w / 2 + x1 * scale + offsetX, y: h / 2 + y1 * scale + offsetY, scale: scale, z: z2 };
  });
  points.sort((a, b) => b.z - a.z);
  return points;
}

export function hit(points, x, y, radius) {
  const r = radius ?? 35;
  for (const p of points) {
    const rad = Math.max(6, Math.min(25, 16 * p.scale));
    if ((p.x - x) ** 2 + (p.y - y) ** 2 < rad ** 2) {
      return p.node;
    }
  }
  return null;
}
