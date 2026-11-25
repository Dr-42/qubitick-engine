const PRIMITIVES = new Set(['null', 'bool', 'num', 'str', 'bytes']);

const aliases = new Map([
  ['NodeId', { kind: 'primitive', name: 'str' }],
  ['Path', { kind: 'primitive', name: 'str' }],
]);

export function defineType(name, type) {
  aliases.set(name, normalize(type));
}

function normalize(type) {
  if (typeof type === 'string') {
    if (PRIMITIVES.has(type)) return { kind: 'primitive', name: type };
    return { kind: 'alias', name: type };
  }
  if (type && type.record) {
    const fields = {};
    for (const [k, v] of Object.entries(type.record)) {
      fields[k] = normalize(v);
    }
    return { kind: 'record', fields };
  }
  if (type && type.list !== undefined) {
    return { kind: 'list', item: normalize(type.list) };
  }
  if (type && type.union) {
    return { kind: 'union', members: type.union.map(normalize) };
  }
  if (type && type.optional !== undefined) {
    return { kind: 'optional', item: normalize(type.optional) };
  }
  return { kind: 'unknown' };
}

function checkPrim(name, value) {
  const map = {
    null: v => v === null,
    bool: v => typeof v === 'boolean',
    num: v => typeof v === 'number' && Number.isFinite(v),
    str: v => typeof v === 'string',
    bytes: v => v instanceof Uint8Array,
  };
  return map[name](value) ? { ok: true } : { ok: false, reason: `expected ${name}, got ${typeof value}` };
}

export function checkType(type, value) {
  if (typeof type === 'string') {
    if (PRIMITIVES.has(type)) {
      return checkPrim(type, value);
    }
    const resolved = aliases.get(type);
    if (!resolved) return { ok: true };
    return check(resolved, value);
  }
  return check(normalize(type), value);
}

function check(norm, value) {
  switch (norm.kind) {
    case 'primitive':
      return checkPrim(norm.name, value);
    case 'alias':
      {
        const resolved = aliases.get(norm.name);
        if (!resolved) return { ok: true };
        return check(resolved, value);
      }
    case 'record':
      {
        if (value === null || typeof value !== 'object' || Array.isArray(value)) {
          return { ok: false, reason: 'expected record' };
        }
        for (const [k, fieldType] of Object.entries(norm.fields)) {
          const r = check(fieldType, value[k]);
          if (!r.ok) return { ok: false, reason: `field ${k}: ${r.reason}` };
        }
        return { ok: true };
      }
    case 'list':
      {
        if (!Array.isArray(value)) {
          return { ok: false, reason: 'expected list' };
        }
        for (const item of value) {
          const r = check(norm.item, item);
          if (!r.ok) return { ok: false, reason: `list item: ${r.reason}` };
        }
        return { ok: true };
      }
    case 'union':
      {
        for (const member of norm.members) {
          const r = check(member, value);
          if (r.ok) return r;
        }
        return { ok: false, reason: 'no union member matched' };
      }
    case 'optional':
      {
        if (value === null || value === undefined) return { ok: true };
        return check(norm.item, value);
      }
    default:
      return { ok: true };
  }
}