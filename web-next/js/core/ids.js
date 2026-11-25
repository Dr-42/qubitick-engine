export function newId(prefix, existing = new Set()) {
  let n = 1;
  while (existing.has(`${prefix}${n}`)) n++;
  const id = `${prefix}${n}`;
  existing.add(id);
  return id;
}

export function slug(s) {
  return String(s)
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+/, '')
    .replace(/-+$/, '');
}