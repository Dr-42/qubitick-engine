export function debounce(fn, ms, { setTimeout, clearTimeout } = globalThis) {
  let timer = null;
  let lastArgs = null;
  const delayed = () => {
    timer = null;
    if (lastArgs) {
      const args = lastArgs;
      lastArgs = null;
      fn(...args);
    }
  };
  function debounced(...args) {
    lastArgs = args;
    if (timer) clearTimeout(timer);
    timer = setTimeout(delayed, ms);
  }
  debounced.cancel = () => {
    if (timer) clearTimeout(timer);
    timer = null;
    lastArgs = null;
  };
  debounced.flush = () => {
    if (timer) clearTimeout(timer);
    timer = null;
    if (lastArgs) {
      const args = lastArgs;
      lastArgs = null;
      fn(...args);
    }
  };
  return debounced;
}