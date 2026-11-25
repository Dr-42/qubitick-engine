import { checkType } from './types.js';

export function valuePort(initial, { type, onTypeError } = {}) {
  let hasValue = initial !== undefined;
  let value = initial;
  const listeners = [];

  return {
    kind: 'value',
    type,
    hasValue() {
      return hasValue;
    },
    get() {
      return value;
    },
    set(v) {
      /* A port that DECLARES a type is checked against it (js/core/types.js). The option has
         been here since the port was written and nothing ever looked at it, so a declared
         type was documentation that could not be wrong. A mismatch is REPORTED, not thrown:
         the value is still delivered (a pane that sends the wrong shape is broken, but it is
         not the wire's job to stop the workspace), and `onTypeError` lets a caller decide. */
      if (type) {
        const result = checkType(type, v);
        if (!result.ok) {
          const detail = { type, reason: result.reason };
          if (typeof onTypeError === 'function') onTypeError(detail);
          else console.warn('[port] value does not match its declared type', detail);
        }
      }
      value = v;
      hasValue = true;
      const copy = listeners.slice();
      for (const fn of copy) {
        try {
          fn(v);
        } catch (err) {
          console.error(err);
        }
      }
    },
    subscribe(fn) {
      if (hasValue) {
        try {
          fn(value);
        } catch (err) {
          console.error(err);
        }
      }
      listeners.push(fn);
      return () => {
        const idx = listeners.indexOf(fn);
        if (idx >= 0) listeners.splice(idx, 1);
      };
    },
  };
}

export function eventPort({ type } = {}) {
  const listeners = [];
  return {
    kind: 'event',
    type,
    emit(payload) {
      const copy = listeners.slice();
      for (const fn of copy) {
        try {
          fn(payload);
        } catch (err) {
          console.error(err);
        }
      }
    },
    on(fn) {
      listeners.push(fn);
      return () => {
        const idx = listeners.indexOf(fn);
        if (idx >= 0) listeners.splice(idx, 1);
      };
    },
  };
}

export function callPort(fn, { type } = {}) {
  let bound = fn !== undefined;
  let impl = fn;
  return {
    kind: 'call',
    type,
    bind(f) {
      impl = f;
      bound = true;
    },
    async invoke(args) {
      if (!bound) {
        return Promise.reject(new Error('call port not bound'));
      }
      try {
        return await impl(args);
      } catch (err) {
        return Promise.reject(err);
      }
    },
  };
}