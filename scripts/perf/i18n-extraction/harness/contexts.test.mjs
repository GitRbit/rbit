import assert from 'node:assert/strict';
import test from 'node:test';
import http from 'node:http';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { once } from 'node:events';
import { OriginCookies, bootstrapContexts, publicHeaders, semantics, hash } from './contexts.mjs';
import { requestSample, validateConfig } from './measure.mjs';

async function server(t, handler) {
  const value = http.createServer(handler);
  value.listen(0, '127.0.0.1');
  await once(value, 'listening');
  t.after(() => new Promise(resolve => { value.closeAllConnections(); value.close(resolve); }));
  return `http://127.0.0.1:${value.address().port}`;
}
function owned(t, origin) {
  const directory = mkdtempSync(join(tmpdir(), 'rbit-owned-context-test-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const metadata = join(directory, 'server.json');
  writeFileSync(metadata, JSON.stringify({ baseUrl: origin, port: Number(new URL(origin).port) }));
  writeFileSync(metadata + '.owner.json', JSON.stringify({ schema: 1, metadataPath: metadata, launcherPid: process.pid, port: Number(new URL(origin).port) }));
  return { baseUrl: origin, metadata };
}

const cookie = (name, value) => `${name}=${value}; Path=/; HttpOnly; SameSite=Lax; Max-Age=3600`;
const options = { maxBytes: 2 * 1024 * 1024, timeoutMs: 2000 };
const guestPage = { name: 'picker', path: '/picker', context: 'guest', contains: 'id="locale"', requiredMarkers: ['name="_csrf"'], forbiddenMarkers: ['data-signed-in'], contentLanguage: 'es' };
const adminPage = { name: 'packs', path: '/packs', context: 'admin', contains: 'id="language-pack"', requiredMarkers: ['language.es', 'language.en-xa', 'language.ar-xb'], contentLanguage: 'es' };

test('cookie jars bind exact origin including port and refuse broader or unexpected cookies', () => {
  const jar = new OriginCookies('http://127.0.0.1:3045', 'admin-session');
  jar.absorb('http://127.0.0.1:3045', [cookie('rbit_session', 'UNIT_SECRET')]);
  assert.throws(() => jar.header('http://127.0.0.1:3046/packs'), /origin differs/);
  assert.throws(() => jar.header('http://example.test/packs'), /origin differs/);
  assert.throws(() => jar.absorb('http://127.0.0.1:3045', [cookie('rbit_session', 'x') + '; Domain=127.0.0.1']), /scope differs/);
  assert.throws(() => jar.absorb('http://127.0.0.1:3045', [cookie('rbit_session', 'x') + '; Secure']), /scope differs/);
  assert.throws(() => jar.absorb('http://127.0.0.1:3045', [cookie('rbit-plugin-browser', 'x')]), /Unexpected/);
  assert.throws(() => jar.absorb('http://127.0.0.1:3045', [cookie('rbit_session', 'ROTATED')], true), /changed after bootstrap/);
  assert.ok(!JSON.stringify(jar).includes('UNIT_SECRET'));
  assert.ok(!JSON.stringify(publicHeaders({ Cookie: 'UNIT_SECRET', Accept: 'text/html' })).includes('UNIT_SECRET'));
});

test('real loopback bootstrap logs in and retains distinct guest/admin cookies without serializing credentials', async t => {
  const secret = 'UNIT_SESSION_DO_NOT_LOG', browser = 'UNIT_BROWSER_DO_NOT_LOG', password = 'UNIT_PASSWORD_DO_NOT_LOG';
  let guestWithoutBinding = 0;
  const origin = await server(t, (req, res) => {
    if (req.url === '/login') {
      let body = ''; req.on('data', chunk => { body += chunk; });
      req.on('end', () => {
        assert.equal(req.headers.origin, origin);
        assert.ok(new URLSearchParams(body).get('password') === password);
        res.writeHead(303, { Location: '/', 'Set-Cookie': [cookie('rbit_session', secret)] }); res.end();
      }); return;
    }
    res.setHeader('Content-Type', 'text/html'); res.setHeader('Content-Language', 'es');
    if (req.url === '/picker') {
      assert.ok(!String(req.headers.cookie).includes('rbit_session'));
      if (!req.headers.cookie) { guestWithoutBinding++; res.setHeader('Set-Cookie', cookie('rbit-plugin-browser', browser)); }
      res.end('<div id="locale"><input name="_csrf" value="UNIT_PROOF_DO_NOT_LOG"></div>'); return;
    }
    assert.ok(String(req.headers.cookie).includes(`rbit_session=${secret}`));
    assert.ok(!String(req.headers.cookie).includes('rbit-plugin-browser'));
    res.end('<div id="language-pack">language.es language.en-xa language.ar-xb</div>');
  });
  const directory = mkdtempSync(join(tmpdir(), 'rbit-perf-auth-test-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const credentials = join(directory, 'private.json'); writeFileSync(credentials, JSON.stringify({ login: 'perf', password }), { mode: 0o600 });
  const config = { pages: [guestPage, adminPage], requestContexts: { guest: { kind: 'guest', bootstrapPath: '/picker' }, admin: { kind: 'admin-session', credentialsFile: credentials, loginPath: '/login', bootstrapPath: '/packs' } } };
  const result = await bootstrapContexts(owned(t, origin), config);
  const samples = [];
  for (let i = 0; i < 2; i++) for (const page of config.pages) samples.push(await requestSample(new URL(page.path, origin), page, options, undefined, result.contexts.get(page.context)));
  assert.ok(samples.every(s => s.valid));
  assert.equal(guestWithoutBinding, 1);
  for (const text of [secret, browser, password, 'UNIT_PROOF_DO_NOT_LOG']) assert.ok(!JSON.stringify({ evidence: result.evidence, samples }).includes(text));
  assert.equal(result.evidence.admin.cookieFingerprint, hash(`rbit_session=${secret}`));
  assert.ok(samples.every(s => s.requestHeaders.Cookie === '[redacted]'));
});

test('redirects are never followed or exposed with location tokens, even with a valid session', async t => {
  let targetCalls = 0;
  const target = await server(t, (_req, res) => { targetCalls++; res.end('unreachable'); });
  const origin = await server(t, (_req, res) => { res.writeHead(307, { Location: `${target}/steal?token=UNIT_LOCATION_SECRET`, 'Content-Type': 'text/html' }); res.end('<div id="language-pack">language.es language.en-xa language.ar-xb</div>'); });
  const jar = new OriginCookies(origin, 'admin-session'); jar.absorb(origin, [cookie('rbit_session', 'UNIT_SESSION_SECRET')]);
  const result = await requestSample(new URL('/packs', origin), adminPage, options, undefined, jar);
  assert.equal(result.valid, false); assert.equal(targetCalls, 0);
  assert.ok(!JSON.stringify(result).includes('UNIT_LOCATION_SECRET'));
  assert.ok(!JSON.stringify(result).includes('UNIT_SESSION_SECRET'));
});

test('semantic failures include late/split required or forbidden markers and locale mismatch', async t => {
  const origin = await server(t, (req, res) => {
    res.writeHead(200, { 'Content-Type': 'text/html', 'Content-Language': req.url === '/wrong-locale' ? 'en' : 'es' });
    res.write('<main id="main">' + 'x'.repeat(1024));
    if (req.url !== '/missing') { res.write('id="lan'); res.write('guage-pack" language.es language.en-xa language.ar-xb'); }
    if (req.url === '/forbidden') { res.write('data-signed'); res.write('-in'); }
    res.end('</main>');
  });
  const page = { ...adminPage, forbiddenMarkers: ['data-signed-in'] };
  assert.equal((await requestSample(new URL('/good', origin), page, options)).valid, true);
  for (const path of ['/missing', '/forbidden', '/wrong-locale']) assert.equal((await requestSample(new URL(path, origin), page, options)).valid, false);
  assert.ok(semantics(200, { 'content-type': 'text/html', 'content-language': 'es' }, '<main id="main">login screen</main>', adminPage).length > 0);
});

test('failed login/authority never becomes authenticated timing and private files are required', async t => {
  const origin = await server(t, (_req, res) => { res.writeHead(200, { 'Content-Type': 'text/html' }); res.end('<main id="main">login</main>'); });
  const directory = mkdtempSync(join(tmpdir(), 'rbit-perf-failed-auth-')); t.after(() => rmSync(directory, { recursive: true, force: true }));
  const file = join(directory, 'credentials.json'); writeFileSync(file, JSON.stringify({ login: 'perf', password: 'UNIT_PRIVATE_PASSWORD' }), { mode: 0o600 });
  const config = { pages: [adminPage], requestContexts: { admin: { kind: 'admin-session', loginPath: '/login', bootstrapPath: '/packs', credentialsFile: file } } };
  await assert.rejects(bootstrapContexts(owned(t, origin), config), /Login did not produce/);
  const open = join(directory, 'open.json'); writeFileSync(open, '{}', { mode: 0o644 }); config.requestContexts.admin.credentialsFile = open;
  await assert.rejects(bootstrapContexts(owned(t, origin), config), /private regular file/);
});

test('an unrelated live loopback listener cannot receive bootstrap credentials without owned metadata', async t => {
  let requests = 0;
  const origin = await server(t, (_req, res) => { requests++; res.end('must not authenticate here'); });
  await assert.rejects(bootstrapContexts({ baseUrl: origin, metadata: '/nonexistent/unowned-perf-server.json' }, { pages: [adminPage], requestContexts: { admin: { kind: 'admin-session', loginPath: '/login', bootstrapPath: '/packs', credentialsFile: '/nonexistent/private.json' } } }), /owned live server metadata/);
  assert.equal(requests, 0);
});

test('immutable postflight evidence fails closed after authority/locale document semantics change', async t => {
  const { postflightContexts } = await import('./contexts.mjs');
  let allowed = true;
  const origin = await server(t, (_req, res) => { res.writeHead(200, { 'Content-Type': 'text/html', 'Content-Language': 'es' }); res.end(allowed ? '<div id="language-pack">language.es language.en-xa language.ar-xb</div>' : '<main id="main">login screen</main>'); });
  const jar = new OriginCookies(origin, 'admin-session'); jar.absorb(origin, [cookie('rbit_session', 'UNIT_POSTFLIGHT_SECRET')]);
  const config = { pages: [adminPage], maxBytes: options.maxBytes };
  const evidence = await postflightContexts({ baseUrl: origin }, config, new Map([['admin', jar]]));
  assert.ok(evidence[0].valid);
  assert.ok(!JSON.stringify(evidence).includes('UNIT_POSTFLIGHT_SECRET'));
  allowed = false;
  await assert.rejects(postflightContexts({ baseUrl: origin }, config, new Map([['admin', jar]])), /Postflight document failed/);
});

test('anonymous or substituted pages cannot satisfy the registered enabled-i18n real-host smoke prerequisite', async () => {
  const { enabledSpec, smoke } = await import('./smoke.mjs');
  const { readFileSync } = await import('node:fs');
  const manifest = JSON.parse(readFileSync(new URL('../experiment.template.json', import.meta.url), 'utf8'));
  const enabled = manifest.profiles.find(p => p.name === 'i18n-enabled');
  const config = { profileSpec: enabled, pages: enabled.pages, requestContexts: enabled.requestContexts, seedFileManifestSha256: '1'.repeat(64), output: '/unused/samples.json', fixture: '/unused/fixture', notes: 'unit source validation only', rounds: 100, warmups: 20, variants: [{ name: 'baseline', baseUrl: 'http://127.0.0.1:3045', metadata: '/unused/A.json' }, { name: 'candidate', baseUrl: 'http://127.0.0.1:3046', metadata: '/unused/B.json' }] };
  assert.match(enabledSpec(config), /^[a-f0-9]{64}$/);
  const substituted = { ...config, pages: manifest.profiles[0].pages, requestContexts: manifest.profiles[0].requestContexts };
  await assert.rejects(smoke(substituted), /registered enabled-i18n profile/);
  const missingArchive = JSON.parse(JSON.stringify(config));
  missingArchive.pages[1].requiredMarkers = missingArchive.pages[1].requiredMarkers.filter(x => x !== 'language.ar-xb');
  missingArchive.profileSpec.pages = missingArchive.pages;
  await assert.rejects(smoke(missingArchive), /locale\/archive semantics/);
});
