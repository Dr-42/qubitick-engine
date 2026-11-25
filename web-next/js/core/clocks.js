import { eventPort } from './ports.js';

export function createClocks({ governor, scope, paused = false } = {}) {
  const clocks = new Map();
  let isPaused_ = paused;
  let nextId = 0;

  function clock(msOrRaf) {
    const isRaf = msOrRaf === 'raf';
    const interval = isRaf ? null : msOrRaf;
    const id = isRaf ? `raf-${nextId++}` : `ms-${nextId++}`;

    if (clocks.has(id)) return clocks.get(id);

    let timer = null;
    let rafCancel = null;
    const tick = eventPort();

    function start() {
      stop();
      if (isRaf) {
        rafCancel = governor.requestFrame(id, () => {
          tick.emit();
          rafCancel = governor.requestFrame(id, () => {});
        });
      } else {
        const loop = () => {
          tick.emit();
          timer = setTimeout(loop, interval);
        };
        timer = setTimeout(loop, interval);
      }
    }

    function stop() {
      if (timer) clearTimeout(timer);
      timer = null;
      if (rafCancel) rafCancel();
      rafCancel = null;
    }

    const node = {
      id,
      kind: 'clock',
      out: { tick },
      _start: start,
      _stop: stop,
    };

    clocks.set(id, node);
    if (!isPaused_) start();
    return node;
  }

  function delay(ms) {
    return (port) => {
      const out = eventPort();
      let timer = null;
      const unsub = (port.subscribe || port.on).call(port, (v) => {
        if (timer) clearTimeout(timer);
        timer = setTimeout(() => { out.emit(v); }, ms);
      });
      const delayPort = { ...out };
      delayPort.dispose = () => {
        if (timer) clearTimeout(timer);
        timer = null;
        unsub();
      };
      return delayPort;
    };
  }

  function pauseAll() {
    isPaused_ = true;
    for (const node of clocks.values()) {
      node._stop();
    }
  }

  function resumeAll() {
    isPaused_ = false;
    for (const node of clocks.values()) {
      node._start();
    }
  }

  function stepAll() {
    for (const node of clocks.values()) {
      node.out.tick.emit();
    }
  }

  function isPaused() {
    return isPaused_;
  }

  if (scope && typeof scope.add === 'function') {
    scope.add(() => {
      for (const node of clocks.values()) {
        node._stop();
      }
      clocks.clear();
    });
  }

  return { clock, delay, pauseAll, resumeAll, stepAll, isPaused };
}