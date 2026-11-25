export function createScope() {
  let disposed = false;
  const children = [];
  const unsubs = [];

  function add(unsub) {
    if (disposed) {
      unsub();
      return () => {};
    }
    unsubs.push(unsub);
    const index = unsubs.length - 1;
    return () => {
      if (index < unsubs.length && unsubs[index] === unsub) {
        unsubs[index] = undefined;
        unsub();
      }
    };
  }

  function child() {
    const childScope = createScope();
    children.push(childScope);
    return childScope;
  }

  function dispose() {
    if (disposed) return;
    disposed = true;

    // Dispose children first
    for (const childScope of children) {
      childScope.dispose();
    }
    children.length = 0;

    // Run unsubs in reverse order
    for (let i = unsubs.length - 1; i >= 0; i--) {
      const unsub = unsubs[i];
      if (typeof unsub === 'function') {
        try {
          unsub();
        } catch (err) {
          console.error(err);
        }
      }
    }
    unsubs.length = 0;
  }

  return {
    add,
    child,
    dispose,
    get disposed() {
      return disposed;
    },
  };
}