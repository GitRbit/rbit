import assert from 'node:assert/strict';
import test from 'node:test';

const origin = 'https://git.example.com';
const scope = '/alice/repo';
const url = `${origin}${scope}/issues`;
const headers = { 'Rbit-Partial': scope, 'Rbit-Nav-Epoch': 'one' };
const html = (extra = '') => `<title>Issues</title><main id="main">${extra}</main><div id="repo-subnav" hx-swap-oob="true" data-repo-scope="${scope}" class="repo-subnav"></div>`;
let sequence = 0;

async function app() {
  globalThis.location = new URL(`${origin}${scope}`);
  const cache = await import(`../../crates/rbit-web/assets/repo-navigation-cache.js?test=${++sequence}`);
  class XHR extends EventTarget {
    constructor() {
      super();
      this.sent = [];
      this.nativeAborts = 0;
      this.timeout = 0;
      this.events = [];
      for (const type of ['loadstart', 'readystatechange', 'load', 'abort', 'timeout', 'loadend']) {
        this.addEventListener(type, (event) => {
          this.events.push(type);
          this[`on${type}`]?.call(this, event);
        });
      }
    }
    get readyState() { return 1; }
    get status() { return 0; }
    get statusText() { return ''; }
    get responseText() { return ''; }
    get response() { return ''; }
    get responseURL() { return ''; }
    getResponseHeader() { return null; }
    getAllResponseHeaders() { return ''; }
    send(body) { this.sent.push(body); }
    abort() { this.nativeAborts++; }
  }
  const response = (text = html(), overrides = {}) => {
    const values = new Headers({ 'Content-Type': 'text/html; charset=utf-8', 'Cache-Control': 'private, no-cache', 'Rbit-Partial': 'true', 'Server-Timing': 'render;dur=1.2', 'Rbit-Update-Available': '100;https://github.com/GitRbit/rbit/releases/tag/v99.0.0', ...overrides.headers });
    return { status: 200, url, text, getHeader: (name) => values.get(name), ...overrides };
  };
  const remember = (text, overrides = {}, destination = url, requestHeaders = headers) => cache.rememberNavigationResponse(destination, requestHeaders, response(text, { url: destination, ...overrides }));
  const request = ({ destination = url, tab = 'issues', requestHeaders = headers, verb = 'get', boosted = true } = {}) => {
    const xhr = new XHR();
    const detail = { xhr, elt: { dataset: { repoTab: tab }, closest: (selector) => selector === '#repo-subnav .repo-tabs' ? {} : null }, pathInfo: { finalRequestPath: destination }, requestConfig: { verb, boosted, path: destination } };
    return { xhr, detail, reused: cache.reuseNavigationResponse(detail, requestHeaders) };
  };
  return { ...cache, XHR, response, remember, request };
}

const flush = async () => { for (let i = 0; i < 12; i++) await Promise.resolve(); };
const deferred = () => { let resolve; const promise = new Promise((yes) => { resolve = yes; }); return { promise, resolve }; };
const fetched = (text = html(), overrides = {}) => new Response(text, { status: 200, headers: { 'Content-Type': 'text/html', 'Rbit-Partial': 'true', ...overrides } });

test('cached transport preserves htmx timing, getters and ordinary XHR events', async () => {
  const a = await app();
  assert.equal(a.remember(html('pristine')), true);
  const { xhr, reused } = a.request();
  assert.equal(reused, true);
  assert.equal(a.isCachedNavigationResponse(xhr), false);
  let loads = 0;
  xhr.onload = () => { loads++; assert.equal(xhr.readyState, 4); };
  assert.equal(xhr.readyState, 1);
  xhr.send(null);
  assert.equal(loads, 0, 'delivery follows htmx beforeSend/send rather than completing inside beforeRequest');
  await flush();
  assert.equal(loads, 1);
  assert.equal(a.isCachedNavigationResponse(xhr), true);
  assert.deepEqual(xhr.sent, []);
  assert.equal(xhr.response, html('pristine'));
  assert.equal(xhr.responseText, xhr.response);
  assert.equal(xhr.responseURL, url);
  assert.equal(xhr.status, 200);
  assert.equal(xhr.statusText, 'OK');
  assert.equal(xhr.getResponseHeader('server-timing'), 'render;dur=1.2');
  assert.equal(xhr.getResponseHeader('rbit-partial'), 'true');
  assert.equal(xhr.getResponseHeader('rbit-update-available'), '100;https://github.com/GitRbit/rbit/releases/tag/v99.0.0');
  assert.match(xhr.getAllResponseHeaders(), /content-type: text\/html/);
  assert.deepEqual(xhr.events, ['loadstart', 'readystatechange', 'load', 'loadend']);
});

test('exact URL query, repository scope and epoch isolate cache entries; hashes share one response', async () => {
  const a = await app();
  a.remember();
  assert.equal(a.request({ destination: `${url}#section` }).reused, true);
  assert.equal(a.request({ destination: `${url}?state=closed` }).reused, false);
  assert.equal(a.request({ requestHeaders: { ...headers, 'Rbit-Nav-Epoch': 'two' } }).reused, false);
  assert.equal(a.request({ destination: `${origin}/alice/other/issues`, requestHeaders: { ...headers, 'Rbit-Partial': '/alice/other' } }).reused, false);
  assert.equal(a.request({ destination: `https://elsewhere.example${scope}/issues` }).reused, false);
});

test('only explicit safe main tabs reuse navigation; forms and settings remain native', async () => {
  const a = await app();
  a.remember();
  for (const tab of ['settings', 'unknown', undefined]) assert.equal(a.request({ tab: tab ?? '' }).reused, false);
  assert.equal(a.request({ verb: 'post' }).reused, false);
  assert.equal(a.request({ boosted: false }).reused, false);
  for (const path of ['/issues/1', '/pull/1', '/settings', '/raw/main/README.md', '/releases/download/v1/a.zip']) {
    assert.equal(a.remember(html(), {}, `${origin}${scope}${path}`), false);
  }
});

test('unsafe or mismatched responses are rejected before reuse', async () => {
  const a = await app();
  for (const change of [
    { status: 404 }, { redirected: true }, { url: `${origin}/login` },
    { url: `${url}?other=1` }, { headers: { 'Content-Type': 'application/json' } },
    { headers: { 'Cache-Control': 'private, no-store' } }, { headers: { 'HX-Redirect': '/login' } },
    { headers: { 'HX-Location': '{}' } }, { headers: { 'HX-Refresh': 'true' } },
    { headers: { 'Set-Cookie': 'session=changed' } }, { headers: { 'HX-Trigger': 'change' } },
  ]) assert.equal(a.remember(html(), change), false, JSON.stringify(change));
  assert.equal(a.remember('<main id="main">Sign in</main>'), false);
  assert.equal(a.remember(html().replace(scope, '/alice/other')), false);
  assert.equal(a.request().reused, false);
});

test('in-flight preload is consumed once and shared with a click', async () => {
  const a = await app();
  const fetch = deferred();
  const preload = a.trackNavigationPreload(url, headers, fetch.promise);
  const { xhr, reused } = a.request();
  assert.equal(reused, true);
  xhr.send(null);
  fetch.resolve(fetched(html('from preload')));
  assert.equal(await preload, true);
  await flush();
  assert.equal(xhr.response, html('from preload'));
  assert.deepEqual(xhr.sent, []);
  assert.equal(a.request().reused, true);
});

test('failed preloads restore native methods and send the original body exactly once', async () => {
  const a = await app();
  const fetch = deferred();
  const preload = a.trackNavigationPreload(url, headers, fetch.promise);
  const { xhr } = a.request();
  xhr.send('body');
  fetch.resolve(new Response('denied', { status: 403 }));
  assert.equal(await preload, false);
  await flush();
  assert.deepEqual(xhr.sent, ['body']);
  assert.equal(Object.hasOwn(xhr, 'send'), false);
  assert.equal(Object.hasOwn(xhr, 'response'), false);
  assert.equal(xhr.status, 0);
  assert.equal(a.isCachedNavigationResponse(xhr), false);
  xhr.abort();
  assert.equal(xhr.nativeAborts, 1);
});

test('clear invalidates a waiting transport immediately and ignores a late completed preload', async () => {
  const a = await app();
  const fetch = deferred();
  const preload = a.trackNavigationPreload(url, headers, fetch.promise);
  const { xhr } = a.request();
  xhr.send(null);
  a.clearNavigationResponses();
  await flush();
  assert.deepEqual(xhr.sent, [null]);
  fetch.resolve(fetched());
  await preload;
  await flush();
  assert.equal(a.request().reused, false);
  assert.equal(xhr.events.includes('load'), false);
});

test('abort finishes once without canceling another owner\'s shared preload', async () => {
  const a = await app();
  const fetch = deferred();
  const signal = new AbortController();
  const preload = a.trackNavigationPreload(url, headers, fetch.promise, signal.signal);
  const { xhr } = a.request();
  let aborts = 0;
  xhr.onabort = () => aborts++;
  xhr.send(null);
  xhr.abort();
  xhr.abort();
  fetch.resolve(fetched());
  await preload;
  await flush();
  assert.equal(aborts, 1);
  assert.equal(signal.signal.aborted, false);
  assert.deepEqual(xhr.sent, []);
  assert.equal(xhr.events.includes('load'), false);
  assert.equal(a.request().reused, true);
});

test('a stalled preload delays the click for at most 250 ms then uses native transport', async () => {
  const a = await app();
  a.trackNavigationPreload(url, headers, new Promise(() => {}));
  const { xhr } = a.request();
  xhr.send(null);
  await new Promise((resolve) => setTimeout(resolve, 280));
  assert.deepEqual(xhr.sent, [null]);
  a.clearNavigationResponses();
});

test('response count and UTF-8 byte budgets evict old HTML rather than retain oversized pages', async () => {
  const a = await app();
  for (let i = 0; i < 9; i++) assert.equal(a.remember(html(), {}, `${url}?page=${i}`), true);
  assert.equal(a.request({ destination: `${url}?page=0` }).reused, false);
  assert.equal(a.request({ destination: `${url}?page=8` }).reused, true);
  a.clearNavigationResponses();
  for (let i = 0; i < 5; i++) assert.equal(a.remember(html('x'.repeat(900000)), {}, `${url}?page=${i}`), true);
  assert.equal(a.request({ destination: `${url}?page=0` }).reused, false);
  assert.equal(a.remember(html('é'.repeat(600000))), false, 'UTF-8 size rather than JS character count bounds memory');
});

test('oversized streaming preloads cancel reading before buffering the full response', async () => {
  const a = await app();
  let reads = 0;
  let canceled = false;
  const stream = new ReadableStream({
    pull(controller) { reads++; controller.enqueue(new Uint8Array(400000)); },
    cancel() { canceled = true; },
  });
  const result = await a.trackNavigationPreload(url, headers, Promise.resolve(new Response(stream, { headers: { 'Content-Type': 'text/html' } })));
  assert.equal(result, false);
  assert.equal(canceled, true);
  assert.ok(reads <= 4, 'stop after the first chunk crossing the byte ceiling');
  assert.equal(a.request().reused, false);
});

test('expired responses miss and expiration after beforeRequest falls back natively', async () => {
  const original = globalThis.performance;
  let now = 0;
  globalThis.performance = { now: () => now };
  try {
    const a = await app();
    a.remember();
    const { xhr } = a.request();
    now = 30000;
    xhr.send(null);
    await flush();
    assert.deepEqual(xhr.sent, [null]);
    assert.equal(a.request().reused, false);
  } finally { globalThis.performance = original; }
});

test('reading and repeatedly delivering a cached response never extends its original TTL', async () => {
  const original = globalThis.performance;
  let now = 0;
  globalThis.performance = { now: () => now };
  try {
    const a = await app();
    a.remember();
    for (now = 5000; now < 30000; now += 5000) {
      const { xhr, reused } = a.request();
      assert.equal(reused, true);
      xhr.send(null);
      await flush();
      assert.equal(a.isCachedNavigationResponse(xhr), true, 'integration skips remembering synthetic deliveries');
    }
    assert.equal(a.request().reused, false, 'entry expires 30 seconds after its first real response');
  } finally { globalThis.performance = original; }
});

test('freshness lookup skips seeded preloads without extending TTL and respects clearing', async () => {
  const original = globalThis.performance;
  let now = 0;
  globalThis.performance = { now: () => now };
  try {
    const a = await app();
    assert.equal(a.hasNavigationResponse(url, headers), false);
    a.remember();
    assert.equal(a.hasNavigationResponse(url, headers), true);
    now = 29999;
    assert.equal(a.hasNavigationResponse(url, headers), true);
    now = 30000;
    assert.equal(a.hasNavigationResponse(url, headers), false);
    a.remember();
    assert.equal(a.hasNavigationResponse(url, headers), true);
    a.clearNavigationResponses();
    assert.equal(a.hasNavigationResponse(url, headers), false);
  } finally { globalThis.performance = original; }
});

test('XHR timeout settles once and ignores an eventually successful preload', async () => {
  const a = await app();
  const fetch = deferred();
  const preload = a.trackNavigationPreload(url, headers, fetch.promise);
  const { xhr } = a.request();
  xhr.timeout = 5;
  let timeouts = 0;
  xhr.ontimeout = () => timeouts++;
  xhr.send(null);
  await new Promise((resolve) => setTimeout(resolve, 15));
  fetch.resolve(fetched());
  await preload;
  await flush();
  assert.equal(timeouts, 1);
  assert.deepEqual(xhr.sent, []);
  assert.equal(xhr.events.includes('load'), false);
});

test('aborted preload falls back without a late body resurrecting cached state', async () => {
  const a = await app();
  const fetch = deferred();
  const owner = new AbortController();
  const preload = a.trackNavigationPreload(url, headers, fetch.promise, owner.signal);
  const { xhr } = a.request();
  xhr.send(null);
  owner.abort();
  await flush();
  assert.deepEqual(xhr.sent, [null]);
  fetch.resolve(fetched());
  assert.equal(await preload, false);
  await flush();
  assert.equal(a.request().reused, false);
});

test('non-configurable browser properties refuse enhancement and restore every touched property', async () => {
  const a = await app();
  a.remember();
  const xhr = new a.XHR();
  Object.defineProperty(xhr, 'status', { get: () => 0, configurable: false });
  const { detail } = a.request();
  detail.xhr = xhr;
  assert.equal(a.reuseNavigationResponse(detail, headers), false);
  xhr.send('native');
  assert.deepEqual(xhr.sent, ['native']);
  assert.equal(Object.hasOwn(xhr, 'response'), false);
  assert.equal(Object.hasOwn(xhr, 'send'), false);
});

test('pending loads are bounded and duplicate registrations share one result', async () => {
  const a = await app();
  const fetches = Array.from({ length: 9 }, deferred);
  const preloads = fetches.map((fetch, index) => a.trackNavigationPreload(`${url}?page=${index}`, headers, fetch.promise));
  assert.equal(await preloads[0], false);
  assert.equal(a.request({ destination: `${url}?page=0` }).reused, false);
  fetches[0].resolve(fetched());
  await flush();
  assert.equal(a.request({ destination: `${url}?page=0` }).reused, false);
  a.clearNavigationResponses();
  const first = deferred();
  const initial = a.trackNavigationPreload(url, headers, first.promise);
  const duplicate = a.trackNavigationPreload(url, headers, Promise.resolve(fetched('redundant')));
  first.resolve(fetched());
  assert.equal(await initial, true);
  assert.equal(await duplicate, true);
});
