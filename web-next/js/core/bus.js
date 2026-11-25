export function createBus() {
  const listeners = new Map();

  function on(name, fn) {
    let list = listeners.get(name);
    if (!list) {
      list = [];
      listeners.set(name, list);
    }
    list.push(fn);
    return () => {
      const idx = list.indexOf(fn);
      if (idx >= 0) list.splice(idx, 1);
    };
  }

  function once(name, fn) {
    const unsub = on(name, (payload) => {
      unsub();
      fn(payload);
    });
    return unsub;
  }

  function emit(name, payload) {
    const list = listeners.get(name);
    if (!list) return 0;
    const copy = list.slice();
    for (const fn of copy) {
      try {
        fn(payload);
      } catch (err) {
        console.error(err);
      }
    }
    return copy.length;
  }

  function listenerCount(name) {
    const list = listeners.get(name);
    return list ? list.length : 0;
  }

  return { on, once, emit, listenerCount };
}