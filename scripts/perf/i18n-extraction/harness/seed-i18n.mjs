// Explicit fixture setup only; never called by the measured request loop.
import { readFileSync, writeFileSync, lstatSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomBytes } from 'node:crypto';
import { authenticateAdministrator, rawRequest, hash, loopbackOrigin, ownedOrigin } from './contexts.mjs';

export const PACKS = [
  { file: 'es.rbit-language', id: 'language.es', locale: 'es', version: '1.1.0', sha256: '39a377a176a74f30339a4d9b5f4a7b198a5d9adaa47085be8e11bc8dd91a3a16' },
  { file: 'en-XA.rbit-language', id: 'language.en-xa', locale: 'en-XA', version: '1.1.0', sha256: '6cba3fb0bded0fe49295fa9b210e8f785dc5106e4fe9fe8f55a08c557fef06c2' },
  { file: 'ar-XB.rbit-language', id: 'language.ar-xb', locale: 'ar-XB', version: '1.1.0', sha256: '228d34d6a7ba05a05cfd49a9a893f5885859f76f9cf872be3afd06a2572dbb1f' },
];
const PATH = '/-/plugins/rbit.i18n-admin/packs', INSTALL = PATH + '/install';
function proof(html) {
  const form = html.split(`action="${INSTALL}"`)[1]?.split('</form>')[0];
  const value = form?.match(/name="_csrf" value="([^"<>\r\n&]{1,4096})"/)?.[1];
  if (!value) throw new Error('Administrator inventory did not issue the expected installation proof');
  return value;
}
export async function seedI18n(metadataFile, credentialsFile, archiveDirectory) {
  const origin = ownedOrigin(metadataFile);
  const jar = await authenticateAdministrator(origin, credentialsFile);
  const installed = [];
  for (const pack of PACKS) {
    const file = resolve(archiveDirectory, pack.file), stat = lstatSync(file);
    if (!stat.isFile() || stat.isSymbolicLink() || stat.size > 2 * 1024 * 1024) throw new Error('Fixture archive is not a bounded regular file');
    const bytes = readFileSync(file);
    if (hash(bytes) !== pack.sha256) throw new Error('Fixture archive differs from registered bytes');
    const page = await rawRequest(origin, PATH, { jar });
    if (page.status !== 200 || !page.body.includes('id="language-pack"')) throw new Error('Real administrator inventory is unavailable');
    const token = proof(page.body), boundary = 'rbit-perf-' + randomBytes(16).toString('hex');
    const body = Buffer.concat([
      Buffer.from(`--${boundary}\r\nContent-Disposition: form-data; name="_csrf"\r\n\r\n${token}\r\n--${boundary}\r\nContent-Disposition: form-data; name="pack"; filename="${pack.file}"\r\nContent-Type: application/zip\r\n\r\n`), bytes, Buffer.from(`\r\n--${boundary}--\r\n`),
    ]);
    const result = await rawRequest(origin, INSTALL, { jar, method: 'POST', body, contentType: `multipart/form-data; boundary=${boundary}` });
    const location = result.headers.location ? new URL(result.headers.location, origin) : null;
    if (result.status !== 303 || !location || location.origin !== origin || location.pathname !== PATH || location.search || location.hash) throw new Error('Fixture archive installation did not complete; no automatic retry or redirect');
    installed.push({ ...pack, bytes: bytes.length });
  }
  const final = await rawRequest(origin, PATH, { jar });
  if (final.status !== 200 || PACKS.some(pack => !final.body.includes(pack.id))) throw new Error('Final fixture inventory does not contain the registered archives');
  return { schema: 1, setupOnly: true, origin, realLoginAndAdministratorProofs: true, archives: installed, mutations: 'three explicit language-pack installations; no enable/disable/locale or policy mutations', inventoryBodySha256: hash(final.body), completedAtUTC: new Date().toISOString() };
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv.length !== 6) { console.error('usage: seed-i18n.mjs OWNED-SERVER-METADATA.json PRIVATE-CREDENTIALS.json ARCHIVE-DIRECTORY OUTPUT.json'); process.exitCode = 1; }
  else seedI18n(...process.argv.slice(2, 5)).then(result => { writeFileSync(process.argv[5], JSON.stringify(result, null, 2) + '\n', { flag: 'wx', mode: 0o600 }); console.log('Registered fixture archives installed through administrator proofs; stop/checkpoint/hash the seed before timing.'); }).catch(() => { console.error('Fixture setup failed; inspect host state privately. No automatic retry or rollback inference.'); process.exitCode = 1; });
}
