// Both request-local helpers and explicitly captured native-renderer snapshots
// must reference the host schema and retain Askama's HTML escaping.
export function validateMessageReferences(source, path, index) {
  let count = 0;
  const staticText = /\b(?:crate::i18n::text|(?:[A-Za-z_]\w*\.)+text)\(\s*"([^"]+)"\s*,\s*"([^"]+)"\s*\)/g;
  for (const match of source.matchAll(staticText)) {
    const id = `${match[1]}.${match[2]}`;
    const message = index.get(id);
    if (!message || Object.keys(message.args ?? {}).length) throw new Error(`${path}: unknown or non-static message ${id}`);
    count += 1;
  }
  const unsafeText = /(?:crate::i18n::(?:text|format_ref|pack_count|pack_coverage)|(?:[A-Za-z_]\w*\.)+(?:text|format))\([^\n]*\)\s*\|\s*safe/;
  if (unsafeText.test(source)) throw new Error(`${path}: translated text must be HTML escaped`);
  return count;
}
