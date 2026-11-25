export function clamp(v, a, b) {
  return Math.min(Math.max(v, a), b);
}

export function lerp(a, b, t) {
  return a + (b - a) * t;
}

export function easeInOut(t) {
  return t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2;
}

export function easeOutCubic(t) {
  return 1 - Math.pow(1 - t, 3);
}

export function approxEq(a, b, eps = 1e-9) {
  return Math.abs(a - b) <= eps;
}

export function percentile(arr, p) {
  const sorted = [...arr].sort((x, y) => x - y);
  const rank = Math.ceil((p / 100) * sorted.length);
  return sorted[Math.max(0, Math.min(sorted.length - 1, rank - 1))];
}