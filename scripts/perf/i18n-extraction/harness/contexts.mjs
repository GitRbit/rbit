// Benchmark-only private contexts. No production routes or dependencies change.
import http from 'node:http';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { lstatSync, readFileSync, existsSync } from 'node:fs';

export const hash = value => createHash('sha256').update(value).digest('hex');
export function loopbackOrigin(value) {
  const url = new URL(value);
  if (url.protocol !== 'http:' || !['127.0.0.1', '[::1]', 'localhost'].includes(url.hostname)
      || url.username || url.password || url.pathname !== '/' || url.search || url.hash) throw new Error('Expected a private HTTP loopback origin');
  return url.origin;
}
const allowed = {
  guest: new Set(['rbit-plugin-browser', 'rbit-locale']),
  'admin-session': new Set(['rbit_session', 'rbit-locale']),
};
export function ownedOrigin(metadataFile) {
  try {
    const metadata = JSON.parse(readFileSync(metadataFile, 'utf8'));
    const owner = JSON.parse(readFileSync(metadataFile + '.owner.json', 'utf8'));
    const origin = loopbackOrigin(metadata.baseUrl);
    if (existsSync(metadataFile + '.exit.json') || owner.schema !== 1 || owner.metadataPath !== resolve(metadataFile) || metadata.port !== owner.port || !Number.isSafeInteger(owner.launcherPid) || owner.launcherPid < 1 || Number(new URL(origin).port) !== owner.port) throw new Error();
    process.kill(owner.launcherPid, 0);
    return origin;
  } catch { throw new Error('Explicit owned live server metadata is required before fixture authentication'); }
}

export class OriginCookies {
  #origin; #kind; #cookies = new Map();
  constructor(origin, kind) {
    this.#origin = loopbackOrigin(origin);
    if (!allowed[kind]) throw new Error('Unknown cookie context');
    this.#kind = kind;
  }
  check(url) {
    if (new URL(url).origin !== this.#origin) throw new Error('Cookie target origin differs from its private context');
  }
  header(url) {
    this.check(url);
    for (const item of this.#cookies.values()) if (item.expires <= Date.now()) throw new Error('Private request cookie expired');
    return [...this.#cookies].sort(([a], [b]) => a.localeCompare(b)).map(([name, item]) => `${name}=${item.value}`).join('; ');
  }
  has(name) { return this.#cookies.has(name); }
  fingerprint(url) { const value = this.header(url); return value ? hash(value) : null; }
  absorb(url, lines = [], locked = false) {
    this.check(url);
    if (!Array.isArray(lines) || lines.length > 8) throw new Error('Invalid cookie response');
    const before = this.header(url);
    const next = new Map(this.#cookies);
    for (const line of lines) {
      if (typeof line !== 'string' || Buffer.byteLength(line) > 4096) throw new Error('Invalid cookie response');
      const [pair, ...rawAttributes] = line.split(';');
      const at = pair.indexOf('=');
      const name = pair.slice(0, at), value = pair.slice(at + 1);
      if (at <= 0 || !allowed[this.#kind].has(name) || !/^[\x21\x23-\x2B\x2D-\x3A\x3C-\x5B\x5D-\x7E]*$/.test(value)) throw new Error('Unexpected or malformed private cookie');
      const attrs = new Map();
      for (const raw of rawAttributes) {
        const part = raw.trim(), index = part.indexOf('=');
        const key = (index < 0 ? part : part.slice(0, index)).toLowerCase();
        if (attrs.has(key)) throw new Error('Duplicate cookie attribute');
        attrs.set(key, index < 0 ? '' : part.slice(index + 1));
      }
      if (attrs.has('domain') || attrs.has('secure') || attrs.get('path') !== '/') throw new Error('Cookie scope differs from loopback-only root context');
      if (!attrs.has('httponly') || attrs.get('samesite')?.toLowerCase() !== 'lax') throw new Error('Cookie lacks expected host security attributes');
      let expires = Infinity;
      if (attrs.has('expires')) {
        expires = Date.parse(attrs.get('expires'));
        if (!Number.isFinite(expires)) throw new Error('Invalid cookie expiry');
      }
      if (attrs.has('max-age')) {
        if (!/^-?\d+$/.test(attrs.get('max-age'))) throw new Error('Invalid cookie age');
        const seconds = Number(attrs.get('max-age'));
        if (!Number.isSafeInteger(seconds)) throw new Error('Invalid cookie age');
        expires = Date.now() + seconds * 1000;
      }
      if (expires <= Date.now() || value === '') next.delete(name);
      else next.set(name, { value, expires });
    }
    const after = [...next].sort(([a], [b]) => a.localeCompare(b)).map(([name, item]) => `${name}=${item.value}`).join('; ');
    if (Buffer.byteLength(after) > 8192 || (locked && after !== before)) throw new Error('Cookie context changed after bootstrap');
    this.#cookies = next;
  }
}

export function publicHeaders(headers) {
  return Object.fromEntries(Object.entries(headers).map(([name, value]) => [name, name.toLowerCase() === 'cookie' ? '[redacted]' : value]));
}
export function semantics(status, headers, body, page) {
  const errors = [];
  if (status !== 200) errors.push('Expected successful HTML document; redirects are not followed');
  if (!String(headers['content-type']).includes('text/html')) errors.push('Expected HTML content type');
  if (headers['content-encoding'] && headers['content-encoding'] !== 'identity') errors.push('Identity encoding required');
  for (const text of [page.contains ?? 'id="main"', ...(page.requiredMarkers ?? [])]) if (!body.includes(text)) errors.push('Required semantic marker missing');
  for (const text of page.forbiddenMarkers ?? []) if (body.includes(text)) errors.push('Forbidden semantic marker present');
  if (page.contentLanguage && headers['content-language'] !== page.contentLanguage) errors.push('Locale metadata differs from registered workload');
  return errors;
}
export function rawRequest(origin, path, { jar = null, method = 'GET', body, contentType = 'application/x-www-form-urlencoded', timeoutMs = 10000, maxBytes = 2 * 1024 * 1024 } = {}) {
  const base = loopbackOrigin(origin), url = new URL(path, base);
  if (url.origin !== base || !path.startsWith('/') || path.startsWith('//')) throw new Error('Bootstrap path escaped its fixed origin');
  const headers = { Accept: 'text/html', 'Accept-Encoding': 'identity', Origin: base };
  if (jar) { const cookie = jar.header(url); if (cookie) headers.Cookie = cookie; }
  if (body !== undefined) { headers['Content-Type'] = contentType; headers['Content-Length'] = Buffer.byteLength(body); }
  return new Promise((resolve, reject) => {
    let timer;
    const fail = () => { clearTimeout(timer); reject(new Error('Private bootstrap transport failed')); };
    const request = http.request(url, { method, headers }, response => {
      const chunks = []; let bytes = 0;
      response.on('data', chunk => { bytes += chunk.length; if (bytes > maxBytes) { fail(); response.destroy(); request.destroy(); } else chunks.push(chunk); });
      response.once('error', fail);
      response.once('end', () => { clearTimeout(timer); resolve({ status: response.statusCode, headers: response.headers, body: Buffer.concat(chunks).toString('utf8') }); });
    });
    request.once('error', fail);
    timer = setTimeout(() => { fail(); request.destroy(); }, timeoutMs);
    if (body !== undefined) request.end(body); else request.end();
  });
}
function credentials(path) {
  const stat = lstatSync(path);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size > 4096 || (stat.mode & 0o077) !== 0) throw new Error('Credentials require a private regular file');
  let result;
  try { result = JSON.parse(readFileSync(path, 'utf8')); } catch { throw new Error('Invalid private credentials file'); }
  if (Object.keys(result).sort().join(',') !== 'login,password' || typeof result.login !== 'string' || typeof result.password !== 'string' || !result.login || !result.password) throw new Error('Invalid private credentials file');
  return result;
}
export async function authenticateAdministrator(origin, credentialsFile) {
  origin = loopbackOrigin(origin);
  const jar = new OriginCookies(origin, 'admin-session');
  const login = credentials(credentialsFile);
  const reply = await rawRequest(origin, '/login', { jar, method: 'POST', body: new URLSearchParams({ ...login, next: '/' }).toString() });
  const location = reply.headers.location ? new URL(reply.headers.location, origin) : null;
  if (reply.status !== 303 || !location || location.origin !== origin || location.pathname !== '/' || location.search || location.hash || location.username || location.password) throw new Error('Login did not produce the expected local session; no redirect followed');
  jar.absorb(origin, reply.headers['set-cookie']);
  if (!jar.has('rbit_session')) throw new Error('Login did not issue a host session');
  return jar;
}

export async function bootstrapContexts(variant, config) {
  const origin = loopbackOrigin(variant.baseUrl);
  if (Object.values(config.requestContexts).some(spec => spec.kind !== 'anonymous') && ownedOrigin(variant.metadata) !== origin) throw new Error('Owned fixture origin differs from registered target');
  const contexts = new Map(), evidence = {};
  for (const [name, spec] of Object.entries(config.requestContexts)) {
    if (spec.kind === 'anonymous') { contexts.set(name, null); evidence[name] = { kind: 'anonymous', origin, cookieFingerprint: null }; continue; }
    const page = config.pages.find(page => page.context === name);
    if (!page || page.path !== spec.bootstrapPath) throw new Error('Cookie bootstrap must use its registered GET page');
    if (spec.kind === 'admin-session' && spec.loginPath !== '/login') throw new Error('Administrator bootstrap uses the fixed login endpoint');
    const jar = spec.kind === 'admin-session' ? await authenticateAdministrator(origin, spec.credentialsFile) : new OriginCookies(origin, spec.kind);
    const reply = await rawRequest(origin, page.path, { jar });
    if (semantics(reply.status, reply.headers, reply.body, page).length) throw new Error('Bootstrap document failed registered authority/locale semantics');
    jar.absorb(origin, reply.headers['set-cookie']);
    if (spec.kind === 'guest' && (!jar.has('rbit-plugin-browser') || jar.has('rbit_session'))) throw new Error('Guest bootstrap did not establish a distinct anonymous browser binding');
    contexts.set(name, jar);
    evidence[name] = { kind: spec.kind, origin, cookieFingerprint: jar.fingerprint(origin), semanticValid: true, bootstrapBodySha256: hash(reply.body), atUTC: new Date().toISOString() };
  }
  return { contexts, evidence };
}

export async function postflightContexts(variant, config, contexts) {
  const origin = loopbackOrigin(variant.baseUrl), evidence = [];
  for (const page of config.pages) {
    const jar = contexts.get(page.context), before = jar?.fingerprint(origin) ?? null;
    const reply = await rawRequest(origin, page.path, { jar, maxBytes: config.maxBytes });
    if (semantics(reply.status, reply.headers, reply.body, page).length) throw new Error('Postflight document failed registered semantics');
    jar?.absorb(origin, reply.headers['set-cookie'], true);
    if ((jar?.fingerprint(origin) ?? null) !== before) throw new Error('Postflight cookie context changed');
    evidence.push({ page: page.name, path: page.path, context: page.context, origin, cookieFingerprint: before, valid: true, contentLanguage: reply.headers['content-language'] ?? null, bodySha256: hash(reply.body), atUTC: new Date().toISOString() });
  }
  return evidence;
}
