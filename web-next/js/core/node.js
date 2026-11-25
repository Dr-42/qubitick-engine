const VALID_KINDS = new Set(['value', 'event', 'call']);

function isPort(p) {
  return p && typeof p === 'object' && VALID_KINDS.has(p.kind);
}

function setNonEnumerable(port, key, val) {
  Object.defineProperty(port, key, {
    enumerable: false,
    configurable: false,
    writable: false,
    value: val,
  });
}

export function createNode({ id, kind, in: ins = {}, out: outs = {}, call: calls = {}, dispose }) {
  const allPorts = {};

  function validatePorts(map, portKind) {
    for (const [name, port] of Object.entries(map)) {
      if (!isPort(port)) {
        throw new TypeError(`invalid port ${name} in ${portKind}`);
      }
      if (portKind === 'call' && port.kind !== 'call') {
        throw new TypeError(`${name} is not a call port`);
      }
      setNonEnumerable(port, 'name', name);
      setNonEnumerable(port, 'owner', id);
      allPorts[name] = port;
    }
  }

  validatePorts(ins, 'in');
  validatePorts(outs, 'out');
  validatePorts(calls, 'call');

  const node = Object.freeze({
    id,
    kind,
    in: Object.freeze(ins),
    out: Object.freeze(outs),
    call: Object.freeze(calls),
    dispose: typeof dispose === 'function' ? dispose : () => {},
  });

  return node;
}