// Validate committed message references without compiling the web template graph.
import { readFileSync, readdirSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = fileURLToPath(new URL('../', import.meta.url));
const messages = JSON.parse(readFileSync(join(root, 'crates/rbit-messages/src/catalog.json'), 'utf8'));
const index = new Map();
for (const message of messages) {
  const id = `${message.namespace}.${message.key}`;
  if (index.has(id) || !/^[a-z][a-z0-9_-]{0,159}$/.test(message.namespace) || !/^[a-z][a-z0-9_-]{0,159}$/.test(message.key)
      || typeof message.english !== 'string' || Buffer.byteLength(message.english) > 8192
      || Object.values(message.args ?? {}).some((kind) => !['text', 'number'].includes(kind))) throw new Error(`Invalid message: ${id}`);
  index.set(id, message);
}
function* files(path) {
  for (const entry of readdirSync(path, { withFileTypes: true })) {
    const file = join(path, entry.name);
    if (entry.isDirectory()) yield* files(file);
    else if (/\.(rs|html)$/.test(entry.name)) yield file;
  }
}
let references = 0;
for (const path of [...files(join(root, 'crates/rbit-web/src')), ...files(join(root, 'crates/rbit-web/templates'))]) {
  const source = readFileSync(path, 'utf8');
  for (const match of source.matchAll(/crate::i18n::text\(\s*"([^"]+)"\s*,\s*"([^"]+)"\s*\)/g)) {
    const id = `${match[1]}.${match[2]}`;
    const message = index.get(id);
    if (!message || Object.keys(message.args ?? {}).length) throw new Error(`${relative(root, path)}: unknown or non-static message ${id}`);
    references += 1;
  }
  if (/crate::i18n::(?:text|format_ref|pack_count|pack_coverage)\([^\n]*\)\s*\|\s*safe/.test(source)) throw new Error(`${relative(root, path)}: translated text must be HTML escaped`);
}
console.log(`${index.size} messages validated; ${references} static references resolve.`);
