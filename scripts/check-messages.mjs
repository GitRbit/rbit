// Validate committed message references without compiling the web template graph.
import { readFileSync, readdirSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { validateMessageReferences } from './message-references.mjs';
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
  references += validateMessageReferences(source, relative(root, path), index);
}
console.log(`${index.size} messages validated; ${references} static references resolve.`);
